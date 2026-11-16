use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;

use resonance_core::identity::{
    AUDIO_HASH_DOMAIN, AssetIdentity, AudioHash, CHROMAPRINT_MAX_SECONDS, Chromaprint, ContentHash,
    DecodedAudioFacts, encode_base64_url_unpadded,
};
use resonance_db::{Database, DbError, IdentityMatch, PendingAsset};
use rusty_chromaprint::{Configuration, FingerprintCompressor, Fingerprinter};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::file_tags::open_audio_reader;

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

/// Builds a canonical path to `relative_path` under `source_folder` so Windows folders ending in a
/// space or dot remain openable.
pub fn full_asset_path(source_folder: &str, relative_path: &str) -> PathBuf {
    let mut path = fs::canonicalize(source_folder).unwrap_or_else(|_| PathBuf::from(source_folder));
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
    let byte_hash = compute_byte_hash(path)?;
    let (audio, chromaprint, decoded_facts) = decode_audio_identity(path);
    Ok(AssetIdentity {
        byte_hash,
        audio,
        chromaprint,
        decoded_facts,
    })
}

fn decode_audio_identity(
    path: &Path,
) -> (
    Option<AudioHash>,
    Option<Chromaprint>,
    Option<DecodedAudioFacts>,
) {
    let Ok(reader) = open_audio_reader(path) else {
        return (None, None, None);
    };
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(ext);
    }
    let Ok(mut format) = symphonia::default::get_probe().probe(
        &hint,
        MediaSourceStream::new(Box::new(reader), Default::default()),
        FormatOptions::default(),
        MetadataOptions::default(),
    ) else {
        return (None, None, None);
    };
    let Some(track) = format.default_track(TrackType::Audio) else {
        return (None, None, None);
    };
    let Some(params) = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .cloned()
    else {
        return (None, None, None);
    };
    let Some(sample_rate) = params.sample_rate.filter(|&rate| rate > 0) else {
        return (None, None, None);
    };
    let Some(channels) = params
        .channels
        .as_ref()
        .map(|ch| ch.count())
        .filter(|&count| count > 0 && u16::try_from(count).is_ok())
    else {
        return (None, None, None);
    };
    let track_id = track.id;
    let decoder_options = AudioDecoderOptions::default().gapless(true).verify(true);
    let Ok(mut decoder) =
        symphonia::default::get_codecs().make_audio_decoder(&params, &decoder_options)
    else {
        return (None, None, None);
    };

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
        return (None, None, None);
    }

    let checksum_ok = decoder.finalize().verify_ok;
    let decoded_facts = Some(DecodedAudioFacts {
        sample_rate,
        channels: channels as u16,
        frames: total_frames,
        decode_errors,
        checksum_ok,
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

    (audio, chromaprint, decoded_facts)
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
