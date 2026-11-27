use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, RwLock};
use std::thread;

use resonance_core::identity::{
    AUDIO_HASH_DOMAIN, AssetIdentity, AssetQuality, AudioHash, CHROMAPRINT_MAX_SECONDS,
    Chromaprint, ContentHash, DecodedAudioFacts, encode_base64_url_unpadded,
};
use resonance_core::library::AudioFormat;
use resonance_core::quality::{self, QualityInput, QualityMeasurements};
use resonance_core::waveform::{Overview, Summariser};
use resonance_db::{Database, DbError, IdentityMatch, PendingAsset};
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
use symphonia::core::codecs::audio::well_known::{
    CODEC_ID_ALAC, CODEC_ID_FLAC, CODEC_ID_PCM_F64BE_PLANAR, CODEC_ID_PCM_S32LE,
};
use symphonia::core::codecs::audio::{
    AudioCodecId, AudioCodecParameters, AudioDecoder, AudioDecoderOptions,
};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;

use crate::file_tags::open_audio_reader;
use crate::quality::QualityMeter;

const EXCERPT_TARGET_SAMPLE_RATE: u32 = 11_025;
/// Symphonia hands out `i32` samples with full scale at 2^31.
const I32_FULL_SCALE: f32 = 2_147_483_648.0;
const MAX_EXCERPT_FALLBACK_SAMPLES: usize = 15 * 60 * EXCERPT_TARGET_SAMPLE_RATE as usize;

struct OpenedAudio {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    total_frames: Option<u64>,
    lossless: bool,
}

/// PCM (but not A-law or mu-law), FLAC and ALAC keep every sample.
fn is_lossless(codec: AudioCodecId) -> bool {
    (CODEC_ID_PCM_S32LE..=CODEC_ID_PCM_F64BE_PLANAR).contains(&codec)
        || codec == CODEC_ID_FLAC
        || codec == CODEC_ID_ALAC
}

fn open_audio_decoder(
    source: Box<dyn MediaSource>,
    ext_hint: Option<&str>,
    verify: bool,
) -> Result<OpenedAudio, LibraryAudioError> {
    let mut hint = Hint::new();
    if let Some(ext) = ext_hint {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            MediaSourceStream::new(source, Default::default()),
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|_| LibraryAudioError::Probe)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or(LibraryAudioError::NoAudioTrack)?;
    let params: AudioCodecParameters = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .cloned()
        .ok_or(LibraryAudioError::NoAudioTrack)?;
    let sample_rate = params
        .sample_rate
        .filter(|&rate| rate > 0)
        .ok_or(LibraryAudioError::NoAudioTrack)?;
    let channels = params
        .channels
        .as_ref()
        .map(|ch| ch.count())
        .filter(|&count| count > 0 && u16::try_from(count).is_ok())
        .ok_or(LibraryAudioError::NoAudioTrack)?;
    let track_id = track.id;
    let total_frames = track.num_frames;
    let lossless = is_lossless(params.codec);
    let mut decoder_options = AudioDecoderOptions::default().gapless(true);
    if verify {
        decoder_options = decoder_options.verify(true);
    }
    let decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &decoder_options)
        .map_err(|_| LibraryAudioError::Decode)?;
    Ok(OpenedAudio {
        format,
        decoder,
        track_id,
        sample_rate,
        channels,
        total_frames,
        lossless,
    })
}

/// Decodes a whole file gaplessly, handing each packet to `visit` with the sample rate as mono
/// samples between -1 and 1, and returns the sample rate.
pub(crate) fn decode_mono(
    path: &Path,
    mut visit: impl FnMut(u32, &[f32]),
) -> Result<u32, LibraryAudioError> {
    let reader = open_audio_reader(path).map_err(LibraryAudioError::Open)?;
    let ext = path.extension().and_then(|ext| ext.to_str());
    let OpenedAudio {
        mut format,
        mut decoder,
        track_id,
        sample_rate,
        channels,
        ..
    } = open_audio_decoder(Box::new(reader), ext, false)?;
    let mut interleaved: Vec<i32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    let mut frames = 0usize;
    let scale = I32_FULL_SCALE * channels as f32;
    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(_) => return Err(LibraryAudioError::Decode),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(DecodeError::DecodeError(_)) => continue,
            Err(_) => return Err(LibraryAudioError::Decode),
        };
        decoded.copy_to_vec_interleaved(&mut interleaved);
        mono.clear();
        mono.extend(
            interleaved
                .chunks_exact(channels)
                .map(|frame| frame.iter().map(|&sample| sample as f32).sum::<f32>() / scale),
        );
        frames += mono.len();
        visit(sample_rate, &mono);
    }
    if frames == 0 {
        return Err(LibraryAudioError::EmptyAudio);
    }
    Ok(sample_rate)
}

/// Errors returned when decoding audio bytes or excerpts for identification.
#[derive(Debug)]
pub enum LibraryAudioError {
    Open(io::Error),
    Probe,
    NoAudioTrack,
    Decode,
    EmptyAudio,
}

impl fmt::Display for LibraryAudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(source) => write!(f, "The audio file could not be opened: {source}"),
            Self::Probe => f.write_str("The audio container could not be read."),
            Self::NoAudioTrack => f.write_str("No decodable audio stream was found."),
            Self::Decode => f.write_str("The audio stream could not be decoded."),
            Self::EmptyAudio => f.write_str("The decoded audio stream held no samples."),
        }
    }
}

impl std::error::Error for LibraryAudioError {}

struct MemoryMedia {
    len: u64,
    cursor: Cursor<Vec<u8>>,
}

impl MemoryMedia {
    fn new(bytes: Vec<u8>) -> Self {
        let len = bytes.len() as u64;
        Self {
            len,
            cursor: Cursor::new(bytes),
        }
    }
}

impl Read for MemoryMedia {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }
}

impl Seek for MemoryMedia {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(pos)
    }
}

impl MediaSource for MemoryMedia {
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.len)
    }
}

/// Recorded in `asset.audio_decoder` so a decoder upgrade can re-hash lossy assets while keeping
/// their track identity through Chromaprint.
pub const AUDIO_DECODER_VERSION: &str = "symphonia/0.6.1";

#[derive(Debug)]
pub enum IdentityError {
    Database(DbError),
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for IdentityError {}

impl From<DbError> for IdentityError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

/// Summary of an `identify_pending` pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdentifyReport {
    pub processed: usize,
    pub decoded: usize,
    pub reconciled_moves: usize,
    pub skipped_unreadable: usize,
}

const SOURCE_ROOT_CACHE_CAP: usize = 64;
static SOURCE_ROOT_CACHE: OnceLock<RwLock<HashMap<String, PathBuf>>> = OnceLock::new();

fn canonical_source_folder(source_folder: &str) -> PathBuf {
    let cache = SOURCE_ROOT_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(guard) = cache.read()
        && let Some(cached) = guard.get(source_folder)
    {
        return cached.clone();
    }
    match fs::canonicalize(source_folder) {
        Ok(canon) => {
            if let Ok(mut guard) = cache.write() {
                if guard.len() >= SOURCE_ROOT_CACHE_CAP {
                    guard.clear();
                }
                guard.insert(source_folder.to_owned(), canon.clone());
            }
            canon
        }
        Err(_) => PathBuf::from(source_folder),
    }
}

/// Builds a canonical path to `relative_path` under `source_folder` so Windows folders ending in a
/// space or dot remain openable.
pub fn full_asset_path(source_folder: &str, relative_path: &str) -> PathBuf {
    let mut path = canonical_source_folder(source_folder);
    for segment in relative_path.split('/').filter(|part| !part.is_empty()) {
        path.push(segment);
    }
    path
}

/// Computes the BLAKE3 hash of `path`'s raw bytes through a read-only file handle.
pub fn compute_byte_hash(path: &Path) -> io::Result<ContentHash> {
    let mut file = File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(ContentHash::from_bytes(*hasher.finalize().as_bytes()))
}

/// Computes `byte_hash`, normalised `i32` little-endian `audio_hash` and 120-second Chromaprint
/// `fingerprint` for `path`, reading the file without modifying bytes or timestamps.
///
/// If `path` opens as a file but cannot be decoded as audio, `byte_hash` is returned with `audio`
/// and `chromaprint` set to `None`.
pub fn compute_identity(path: &Path) -> io::Result<AssetIdentity> {
    identity_of(path, false)
}

/// The identity of a file offered to replace another, which also measures the high end of a
/// lossy file's spectrum so the compare sheet can show where its encoder cut it off.
pub fn compute_identity_for_compare(path: &Path) -> io::Result<AssetIdentity> {
    identity_of(path, true)
}

fn identity_of(path: &Path, always_measure_spectrum: bool) -> io::Result<AssetIdentity> {
    let byte_hash = compute_byte_hash(path)?;
    let decoded = decode_audio_identity(path, always_measure_spectrum);
    let quality = AudioFormat::of(path).map(|format| {
        let measurements = decoded.measurements.unwrap_or_default();
        let flags = quality::assess(&QualityInput {
            format,
            facts: decoded.facts,
            measurements,
        });
        AssetQuality {
            measurements,
            flags,
        }
    });
    Ok(AssetIdentity {
        byte_hash,
        audio: decoded.audio,
        chromaprint: decoded.chromaprint,
        decoded_facts: decoded.facts,
        quality,
        overview: decoded.overview,
    })
}

#[derive(Default)]
struct DecodedIdentity {
    audio: Option<AudioHash>,
    chromaprint: Option<Chromaprint>,
    facts: Option<DecodedAudioFacts>,
    measurements: Option<QualityMeasurements>,
    overview: Option<Overview>,
}

/// Decodes every packet once, feeding the audio hash, the fingerprint, the quality meter and the
/// waveform.
fn decode_audio_identity(path: &Path, always_measure_spectrum: bool) -> DecodedIdentity {
    let Ok(reader) = open_audio_reader(path) else {
        return DecodedIdentity::default();
    };
    let ext = path.extension().and_then(|ext| ext.to_str());
    let Ok(OpenedAudio {
        mut format,
        mut decoder,
        track_id,
        sample_rate,
        channels,
        total_frames: header_frames,
        lossless,
    }) = open_audio_decoder(Box::new(reader), ext, true)
    else {
        return DecodedIdentity::default();
    };
    let spectrum = always_measure_spectrum
        || AudioFormat::of(path)
            .is_some_and(|format| quality::checks_for_transcode(format, lossless, sample_rate));
    let mut meter = QualityMeter::new(sample_rate, channels, spectrum);
    let mut summariser = Summariser::new(sample_rate, channels);

    let mut audio_hasher = blake3::Hasher::new();
    audio_hasher.update(AUDIO_HASH_DOMAIN);
    audio_hasher.update(&sample_rate.to_le_bytes());
    audio_hasher.update(&(channels as u16).to_le_bytes());

    let fp_config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&fp_config);
    let fp_enabled = u32::try_from(channels)
        .ok()
        .is_some_and(|ch| printer.start(sample_rate, ch).is_ok());
    let max_fp_samples = (CHROMAPRINT_MAX_SECONDS as usize)
        .saturating_mul(sample_rate as usize)
        .saturating_mul(channels);
    let mut fp_samples_fed = 0usize;

    let mut pcm32: Vec<i32> = Vec::new();
    let mut pcm32_bytes: Vec<u8> = Vec::new();
    let mut pcm_f32: Vec<f32> = Vec::new();
    let mut pcm16: Vec<i16> = Vec::new();
    let mut total_frames: u64 = 0;
    let mut decode_errors: u32 = 0;

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(DecodeError::DecodeError(_)) => {
                decode_errors = decode_errors.saturating_add(1);
                continue;
            }
            Err(_) => {
                decode_errors = decode_errors.saturating_add(1);
                break;
            }
        };
        decoded.copy_to_vec_interleaved(&mut pcm32);
        let whole_samples = pcm32.len() - (pcm32.len() % channels);
        if whole_samples == 0 {
            continue;
        }
        total_frames = total_frames.saturating_add((whole_samples / channels) as u64);

        pcm32_bytes.clear();
        pcm32_bytes.reserve(whole_samples * 4);
        for &sample in &pcm32[..whole_samples] {
            pcm32_bytes.extend_from_slice(&sample.to_le_bytes());
        }
        audio_hasher.update(&pcm32_bytes);

        pcm_f32.clear();
        pcm_f32.extend(
            pcm32[..whole_samples]
                .iter()
                .map(|&sample| sample as f32 / I32_FULL_SCALE),
        );
        meter.push(&pcm_f32);
        summariser.push(&pcm_f32);

        if fp_enabled && fp_samples_fed < max_fp_samples {
            decoded.copy_to_vec_interleaved(&mut pcm16);
            let remaining = max_fp_samples - fp_samples_fed;
            let take = remaining.min(pcm16.len().min(whole_samples));
            if take > 0 {
                printer.consume(&pcm16[..take]);
                fp_samples_fed += take;
            }
        }
    }

    if total_frames == 0 {
        return DecodedIdentity::default();
    }

    let checksum_ok = decoder.finalize().verify_ok;
    let facts = Some(DecodedAudioFacts {
        sample_rate,
        channels: channels as u16,
        frames: total_frames,
        decode_errors,
        checksum_ok,
        header_frames,
        lossless,
    });

    let audio = Some(AudioHash {
        hash: ContentHash::from_bytes(*audio_hasher.finalize().as_bytes()),
        decoder: AUDIO_DECODER_VERSION.to_owned(),
    });

    let chromaprint = if fp_enabled && fp_samples_fed > 0 {
        printer.finish();
        let raw = printer.fingerprint().to_vec();
        if raw.is_empty() {
            None
        } else {
            let compressed = FingerprintCompressor::from(&fp_config).compress(&raw);
            let encoded = encode_base64_url_unpadded(&compressed);
            let duration_s =
                u32::try_from(total_frames / u64::from(sample_rate)).unwrap_or(u32::MAX);
            Some(Chromaprint {
                encoded,
                raw,
                duration_s,
            })
        }
    } else {
        None
    };

    DecodedIdentity {
        audio,
        chromaprint,
        facts,
        measurements: Some(meter.finish()),
        overview: Some(summariser.finish().overview()),
    }
}

/// Decodes in-memory audio bytes (such as a 30-second MP3/M4A preview) into raw Chromaprint
/// subfingerprints without touching the disk.
pub fn fingerprint_audio_bytes(
    bytes: &[u8],
    ext_hint: Option<&str>,
) -> Result<Vec<u32>, LibraryAudioError> {
    let media = Box::new(MemoryMedia::new(bytes.to_vec()));
    decode_chromaprint_from_source(media, ext_hint, None)
}

/// Decodes an entire audio file on disk (no 120-second limit) into raw Chromaprint
/// subfingerprints, for matching 30-second previews located later in extended club mixes.
pub fn fingerprint_file_full(path: &Path) -> Result<Vec<u32>, LibraryAudioError> {
    let reader = open_audio_reader(path).map_err(LibraryAudioError::Open)?;
    let ext = path.extension().and_then(|e| e.to_str());
    decode_chromaprint_from_source(Box::new(reader), ext, None)
}

/// Extracts a mono `11,025 Hz` 16-bit RIFF/WAVE excerpt starting around `start_ratio`
/// (`0.0..=0.85`) of the track, kept strictly in RAM for opt-in audio recognisers.
pub fn extract_pcm_wav_excerpt(
    path: &Path,
    start_ratio: f32,
    duration_s: u32,
) -> Result<Vec<u8>, LibraryAudioError> {
    if duration_s == 0 {
        return Err(LibraryAudioError::EmptyAudio);
    }
    let reader = open_audio_reader(path).map_err(LibraryAudioError::Open)?;
    let ext = path.extension().and_then(|ext| ext.to_str());
    let OpenedAudio {
        mut format,
        mut decoder,
        track_id,
        sample_rate,
        channels,
        total_frames,
        ..
    } = open_audio_decoder(Box::new(reader), ext, false)?;

    let known_window = total_frames.filter(|&frames| frames > 0).map(|frames| {
        let wanted_src = u64::from(duration_s).saturating_mul(u64::from(sample_rate));
        let clamped_ratio = f64::from(start_ratio.clamp(0.0, 0.85));
        let raw_start = (frames as f64 * clamped_ratio).round() as u64;
        let start_frame = raw_start.min(frames.saturating_sub(wanted_src));
        let end_frame = start_frame.saturating_add(wanted_src).min(frames);
        (start_frame, end_frame)
    });

    let mut pcm16: Vec<i16> = Vec::new();
    let mut mono_11k: Vec<i16> = Vec::new();
    let step = sample_rate as f64 / f64::from(EXCERPT_TARGET_SAMPLE_RATE);
    let mut frames_seen: u64 = 0;
    let mut src_i: u64 = 0;
    let mut out_idx: u64 = 0;
    let mut acc: i64 = 0;
    let mut acc_n: u64 = 0;

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(DecodeError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        decoded.copy_to_vec_interleaved(&mut pcm16);
        let packet_frames = pcm16.len() / channels;
        if packet_frames == 0 {
            continue;
        }

        let (skip, take_end) = match known_window {
            Some((start_f, end_f)) => {
                if frames_seen + packet_frames as u64 <= start_f {
                    frames_seen += packet_frames as u64;
                    continue;
                }
                let s = start_f.saturating_sub(frames_seen) as usize;
                let e = (end_f.saturating_sub(frames_seen) as usize).min(packet_frames);
                (s, e)
            }
            None => (0, packet_frames),
        };

        for frame in pcm16[skip * channels..take_end * channels].chunks_exact(channels) {
            let sum: i32 = frame.iter().map(|&s| i32::from(s)).sum();
            acc += i64::from(sum / channels as i32);
            acc_n += 1;
            src_i += 1;
            let target = ((out_idx + 1) as f64 * step).round() as u64;
            if src_i >= target {
                mono_11k.push((acc / acc_n as i64).clamp(-32768, 32767) as i16);
                acc = 0;
                acc_n = 0;
                out_idx += 1;
                if known_window.is_none() && mono_11k.len() >= MAX_EXCERPT_FALLBACK_SAMPLES {
                    break;
                }
            }
        }

        frames_seen += packet_frames as u64;
        if let Some((_, end_f)) = known_window
            && frames_seen >= end_f
        {
            break;
        }
        if known_window.is_none() && mono_11k.len() >= MAX_EXCERPT_FALLBACK_SAMPLES {
            break;
        }
    }

    if mono_11k.is_empty() && acc_n > 0 {
        mono_11k.push((acc / acc_n as i64).clamp(-32768, 32767) as i16);
    }
    if mono_11k.is_empty() {
        return Err(LibraryAudioError::EmptyAudio);
    }

    let slice = if known_window.is_some() {
        &mono_11k[..]
    } else {
        let wanted = (duration_s as usize).saturating_mul(EXCERPT_TARGET_SAMPLE_RATE as usize);
        let clamped_ratio = f64::from(start_ratio.clamp(0.0, 0.85));
        let raw_start = (mono_11k.len() as f64 * clamped_ratio).round() as usize;
        let start = raw_start.min(mono_11k.len().saturating_sub(wanted));
        let end = (start + wanted).min(mono_11k.len());
        &mono_11k[start..end]
    };

    let data_bytes = u32::try_from(slice.len().saturating_mul(2)).unwrap_or(u32::MAX);
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(data_bytes.saturating_add(36)).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&EXCERPT_TARGET_SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(EXCERPT_TARGET_SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for &sample in slice {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    Ok(wav)
}

fn decode_chromaprint_from_source(
    source: Box<dyn MediaSource>,
    ext_hint: Option<&str>,
    max_seconds: Option<u32>,
) -> Result<Vec<u32>, LibraryAudioError> {
    let OpenedAudio {
        mut format,
        mut decoder,
        track_id,
        sample_rate,
        channels,
        ..
    } = open_audio_decoder(source, ext_hint, false)?;

    let fp_config = Configuration::preset_test2();
    let mut printer = Fingerprinter::new(&fp_config);
    let ch_u32 = u32::try_from(channels).map_err(|_| LibraryAudioError::NoAudioTrack)?;
    printer
        .start(sample_rate, ch_u32)
        .map_err(|_| LibraryAudioError::Decode)?;

    let max_samples = max_seconds.map(|secs| {
        (secs as usize)
            .saturating_mul(sample_rate as usize)
            .saturating_mul(channels)
    });
    let mut fed = 0usize;
    let mut pcm16: Vec<i16> = Vec::new();

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(audio) => audio,
            Err(DecodeError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        decoded.copy_to_vec_interleaved(&mut pcm16);
        let whole = pcm16.len() - (pcm16.len() % channels);
        if whole == 0 {
            continue;
        }
        let take = match max_samples {
            Some(limit) => limit.saturating_sub(fed).min(whole),
            None => whole,
        };
        if take > 0 {
            printer.consume(&pcm16[..take]);
            fed += take;
        }
        if max_samples.is_some_and(|limit| fed >= limit) {
            break;
        }
    }

    if fed == 0 {
        return Err(LibraryAudioError::EmptyAudio);
    }
    printer.finish();
    let raw = printer.fingerprint().to_vec();
    if raw.is_empty() {
        Err(LibraryAudioError::EmptyAudio)
    } else {
        Ok(raw)
    }
}

pub(crate) fn first_missing_candidate(
    candidates: &[IdentityMatch],
    exclude_asset_id: Option<i64>,
) -> Option<&IdentityMatch> {
    candidates.iter().find(|candidate| {
        exclude_asset_id != Some(candidate.asset_id)
            && !full_asset_path(&candidate.source_folder, &candidate.path).exists()
    })
}

/// Applies `identity` for `pending`: if `pending` has never been identified before and another
/// asset with the same `byte_hash` or `audio_hash` is no longer at its old path on disk,
/// reconciles `pending` onto that original asset so its `track_id` and `asset.id` survive;
/// otherwise stores `identity` on `pending.asset_id`.
///
/// Returns `Ok(true)` if a moved asset was reconciled.
pub fn apply_computed_identity(
    database: &mut Database,
    pending: &PendingAsset,
    identity: &AssetIdentity,
) -> Result<bool, DbError> {
    if pending.is_new {
        let by_bytes = database.assets_with_byte_hash(&identity.byte_hash)?;
        if let Some(original) = first_missing_candidate(&by_bytes, Some(pending.asset_id))
            && database.reconcile_moved_asset(original.asset_id, pending.asset_id, identity)?
        {
            return Ok(true);
        }

        if let Some(audio) = &identity.audio {
            let by_audio = database.assets_with_audio_hash(&audio.hash)?;
            if let Some(original) = first_missing_candidate(&by_audio, Some(pending.asset_id))
                && database.reconcile_moved_asset(original.asset_id, pending.asset_id, identity)?
            {
                return Ok(true);
            }
        }
    }

    database.record_asset_identity(pending.asset_id, pending.stamp, identity)?;
    Ok(false)
}

/// Applies a batch of computed identities without holding `database` across filesystem `.exists()`
/// checks, committing standard updates inside a single SQLite WAL transaction.
pub fn apply_computed_identity_batch(
    database: &Mutex<Database>,
    batch: &[PendingAsset],
    computed: Vec<Option<AssetIdentity>>,
) -> Result<IdentifyReport, DbError> {
    let mut report = IdentifyReport::default();
    let mut standard_updates: Vec<(i64, resonance_db::FileStamp, AssetIdentity)> =
        Vec::with_capacity(batch.len());

    for (pending, maybe_identity) in batch.iter().zip(computed) {
        let Some(identity) = maybe_identity else {
            report.skipped_unreadable += 1;
            continue;
        };
        report.processed += 1;
        if identity.audio.is_some() {
            report.decoded += 1;
        }

        if pending.is_new {
            let (by_bytes, by_audio) = {
                let db = database
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let by_bytes = db.assets_with_byte_hash(&identity.byte_hash)?;
                let by_audio = match &identity.audio {
                    Some(audio) => db.assets_with_audio_hash(&audio.hash)?,
                    None => Vec::new(),
                };
                (by_bytes, by_audio)
            };
            let matched_id = first_missing_candidate(&by_bytes, Some(pending.asset_id))
                .or_else(|| first_missing_candidate(&by_audio, Some(pending.asset_id)))
                .map(|m| m.asset_id);
            if let Some(original_id) = matched_id {
                let reconciled = {
                    let mut db = database
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    db.reconcile_moved_asset(original_id, pending.asset_id, &identity)?
                };
                if reconciled {
                    report.reconciled_moves += 1;
                    continue;
                }
            }
        }

        standard_updates.push((pending.asset_id, pending.stamp, identity));
    }

    if !standard_updates.is_empty() {
        let mut db = database
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        db.record_asset_identities(&standard_updates)?;
    }

    Ok(report)
}

/// Computes `AssetIdentity` across `batch` on up to half the machine's logical cores (capped at 8)
/// so large library passes finish quickly while leaving CPU headroom for the UI and audio engine.
pub fn compute_batch(
    batch: &[PendingAsset],
    shutdown: Option<&AtomicBool>,
) -> Vec<Option<AssetIdentity>> {
    if batch.is_empty() {
        return Vec::new();
    }
    let workers = thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 8))
        .unwrap_or(2)
        .min(batch.len());
    let results: Mutex<Vec<Option<AssetIdentity>>> = Mutex::new(vec![None; batch.len()]);
    let next = AtomicUsize::new(0);

    thread::scope(|scope| {
        for _ in 0..workers {
            let next_ref = &next;
            let results_ref = &results;
            scope.spawn(move || {
                loop {
                    if shutdown.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                        break;
                    }
                    let index = next_ref.fetch_add(1, Ordering::Relaxed);
                    let Some(pending) = batch.get(index) else {
                        break;
                    };
                    let path = full_asset_path(&pending.source_folder, &pending.path);
                    if let Ok(identity) = compute_identity(&path)
                        && let Ok(mut guard) = results_ref.lock()
                    {
                        guard[index] = Some(identity);
                    }
                }
            });
        }
    });

    results
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Computes and records identity for every included asset in `database` that does not have an
/// up-to-date identity yet.
pub fn identify_pending(database: &mut Database) -> Result<IdentifyReport, IdentityError> {
    const BATCH_SIZE: u32 = 32;
    let mut report = IdentifyReport::default();
    let mut after_asset_id = 0;
    loop {
        let batch =
            database.pending_identity_assets(after_asset_id, AUDIO_DECODER_VERSION, BATCH_SIZE)?;
        let Some(last) = batch.last() else {
            break;
        };
        after_asset_id = last.asset_id;
        let computed = compute_batch(&batch, None);
        for (pending, maybe_identity) in batch.into_iter().zip(computed) {
            let Some(identity) = maybe_identity else {
                report.skipped_unreadable += 1;
                continue;
            };
            report.processed += 1;
            if identity.audio.is_some() {
                report.decoded += 1;
            }
            if apply_computed_identity(database, &pending, &identity)? {
                report.reconciled_moves += 1;
            }
        }
    }
    Ok(report)
}
