use std::f64::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use lofty::tag::{Tag, TagType};
use resonance_core::identity::{encode_base64_url_unpadded, fingerprint_similarity};
use resonance_core::library::AudioFormat;
use resonance_db::Database;
use resonance_fixtures::audio_file::pcm16;
use resonance_fixtures::signal::Signal;
use resonance_library::identity::{
    AUDIO_DECODER_VERSION, compute_byte_hash, compute_identity, identify_pending,
};
use resonance_library::scan::scan;
use rusty_chromaprint::{Configuration, FingerprintCompressor};

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder created");
    folder
}

/// Builds a rich stereo musical phrase so Chromaprint extracts varied chroma features across frames.
fn musical_phrase(base_hz: f64, tempo_bpm: f64, seconds: f64, sample_rate: u32) -> Signal {
    let frames = (seconds * f64::from(sample_rate)).round() as usize;
    let frames_per_beat = 60.0 / tempo_bpm * f64::from(sample_rate);
    let chord_ratios = [1.0, 1.25, 1.5, 1.875];
    let mut samples = Vec::with_capacity(frames * 2);

    for frame in 0..frames {
        let t = frame as f64 / f64::from(sample_rate);
        let beat_index = (frame as f64 / frames_per_beat).floor() as usize;
        let beat_phase = (frame as f64 % frames_per_beat) / frames_per_beat;
        let root = base_hz * chord_ratios[beat_index % chord_ratios.len()];
        let fifth = root * 1.498_307;
        let octave_mod = 1.0 + 0.35 * (TAU * 0.5 * t).sin();
        let envelope = (-3.2 * beat_phase).exp() * 0.35 + 0.12;

        let left = envelope
            * ((TAU * root * t).sin() * 0.65
                + (TAU * fifth * octave_mod * t).sin() * 0.35
                + (TAU * root * 2.0 * t).cos() * 0.18);
        let right = envelope
            * ((TAU * root * t).sin() * 0.60
                + (TAU * fifth * t).cos() * 0.38
                + (TAU * root * 3.0 * t).sin() * 0.16);
        samples.push(left.clamp(-0.95, 0.95) as f32);
        samples.push(right.clamp(-0.95, 0.95) as f32);
    }

    Signal {
        sample_rate,
        channels: 2,
        samples,
    }
}

/// Writes the exact 16-bit quantized PCM values of `signal` inside a 24-bit little-endian WAV
/// container (`sample_i16 << 8`) to prove bit-depth normalisation in `audio_hash`.
fn write_24bit_padded_wav(signal: &Signal, path: &Path) {
    let mut data = Vec::with_capacity(signal.samples.len() * 3);
    for &sample in &signal.samples {
        let shifted = i32::from(pcm16(sample)) << 8;
        let le = shifted.to_le_bytes();
        data.extend_from_slice(&le[..3]);
    }
    let data_size = u32::try_from(data.len()).expect("fits u32");
    let block_align = signal.channels * 3;
    let byte_rate = signal.sample_rate * u32::from(block_align);

    let mut wav = Vec::with_capacity(data.len() + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(data_size + 36).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&signal.channels.to_le_bytes());
    wav.extend_from_slice(&signal.sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&24u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(&data);
    fs::write(path, wav).expect("24-bit wav written");
}

fn retag_flac_with_artwork(path: &Path, title: &str, artist: &str) {
    let mut tagged = lofty::read_from_path(path).expect("flac opens for tagging");
    let tag = match tagged.primary_tag_mut() {
        Some(tag) => tag,
        None => {
            tagged.insert_tag(Tag::new(TagType::VorbisComments));
            tagged.primary_tag_mut().expect("inserted tag")
        }
    };
    tag.set_title(title.to_owned());
    tag.set_artist(artist.to_owned());
    tag.push_picture(
        Picture::unchecked(vec![0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4])
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build(),
    );
    tagged
        .save_to_path(path, WriteOptions::default())
        .expect("tags saved");
}

#[test]
fn lossless_wav_flac_aiff_24bit_and_retagged_copies_share_one_audio_hash_without_touching_files() {
    let root = test_folder("identity_lossless_invariance");
    let signal = musical_phrase(220.0, 124.0, 8.0, 44_100);

    let wav16 = root.join("master16.wav");
    let wav24 = root.join("master24.wav");
    let aiff = root.join("master.aiff");
    let flac = root.join("master.flac");
    let flac_tagged = root.join("master-tagged.flac");

    signal.write_wav(&wav16).expect("wav16");
    write_24bit_padded_wav(&signal, &wav24);
    signal.write_aiff(&aiff).expect("aiff");
    signal.write_flac(&flac).expect("flac");
    signal.write_flac(&flac_tagged).expect("flac_tagged");
    retag_flac_with_artwork(&flac_tagged, "Night Drive", "Kora Vale");

    let paths = [&wav16, &wav24, &aiff, &flac, &flac_tagged];
    let snapshots_before: Vec<_> = paths
        .iter()
        .map(|p| {
            let bytes = fs::read(p).expect("read before");
            let modified = fs::metadata(p).expect("meta").modified().expect("mtime");
            (bytes, modified)
        })
        .collect();

    let identities: Vec<_> = paths
        .iter()
        .map(|p| compute_identity(p).expect("identity computed"))
        .collect();

    for (path, (bytes_before, modified_before)) in paths.iter().zip(&snapshots_before) {
        assert_eq!(&fs::read(path).expect("read after"), bytes_before);
        assert_eq!(
            &fs::metadata(path)
                .expect("meta after")
                .modified()
                .expect("mtime after"),
            modified_before
        );
    }

    for i in 0..identities.len() {
        for j in (i + 1)..identities.len() {
            assert_ne!(
                identities[i].byte_hash, identities[j].byte_hash,
                "byte_hash must differ between {:?} and {:?}",
                paths[i], paths[j]
            );
        }
    }

    let baseline_audio = identities[0].audio.as_ref().expect("wav16 decoded");
    assert_eq!(baseline_audio.decoder, AUDIO_DECODER_VERSION);
    let baseline_fp = identities[0]
        .chromaprint
        .as_ref()
        .expect("wav16 fingerprinted");
    assert!(!baseline_fp.encoded.is_empty());
    assert!(!baseline_fp.encoded.contains('='));

    for (path, id) in paths.iter().zip(&identities).skip(1) {
        assert_eq!(
            id.audio.as_ref(),
            Some(baseline_audio),
            "audio_hash mismatch for {path:?}"
        );
        assert_eq!(
            id.chromaprint.as_ref(),
            Some(baseline_fp),
            "chromaprint mismatch for {path:?}"
        );
    }
}

#[test]
fn renaming_moving_or_retagging_outside_the_app_keeps_track_and_asset_identity() {
    let root = test_folder("identity_external_moves");
    let source_a = root.join("SourceA");
    let source_b = root.join("SourceB");
    fs::create_dir_all(&source_a).expect("source_a");
    fs::create_dir_all(&source_b).expect("source_b");

    let alpha = musical_phrase(196.0, 126.0, 6.0, 44_100);
    let beta = musical_phrase(293.66, 130.0, 6.0, 44_100);

    let alpha_path = source_a.join("alpha.flac");
    let beta_path = source_a.join("beta.wav");
    alpha.write_flac(&alpha_path).expect("alpha written");
    retag_flac_with_artwork(&alpha_path, "Alpha Original", "DJ One");
    beta.write_wav(&beta_path).expect("beta written");

    let mut db = Database::open_in_memory().expect("db");
    scan(&mut db, &source_a, &[]).expect("initial scan");
    let report = identify_pending(&mut db).expect("initial identify");
    assert_eq!(report.processed, 2);
    assert_eq!(report.decoded, 2);

    let tracks = db.tracks().expect("tracks");
    assert_eq!(tracks.len(), 2);
    let alpha_track_id = tracks
        .iter()
        .find(|row| row.file.path == "alpha.flac")
        .expect("alpha row")
        .track_id;
    let beta_track_id = tracks
        .iter()
        .find(|row| row.file.path == "beta.wav")
        .expect("beta row")
        .track_id;
    let alpha_asset_id = db
        .track_identity(alpha_track_id)
        .expect("query")
        .expect("alpha identity")
        .asset_id;
    let beta_asset_id = db
        .track_identity(beta_track_id)
        .expect("query")
        .expect("beta identity")
        .asset_id;

    // 1. External rename in place.
    let alpha_renamed = source_a.join("alpha-renamed.flac");
    fs::rename(&alpha_path, &alpha_renamed).expect("renamed on disk");
    let rename_scan = scan(&mut db, &source_a, &[]).expect("scan after rename");
    assert_eq!(rename_scan.added, 0);
    assert_eq!(rename_scan.changed, 1);
    assert_eq!(db.track_count().expect("count after rename"), 2);
    let stored_alpha = db
        .track_identity(alpha_track_id)
        .expect("query")
        .expect("alpha survives rename");
    assert_eq!(stored_alpha.asset_id, alpha_asset_id);
    assert_eq!(stored_alpha.path, "alpha-renamed.flac");

    // 2. External move into a subfolder + container change (WAV -> FLAC) + new tags/artwork.
    let subfolder = source_a.join("Deep");
    fs::create_dir_all(&subfolder).expect("subfolder");
    let beta_repackaged = subfolder.join("beta-retagged.flac");
    beta.write_flac(&beta_repackaged).expect("beta flac");
    retag_flac_with_artwork(&beta_repackaged, "Beta Club Mix", "DJ Two");
    fs::remove_file(&beta_path).expect("old beta wav removed");

    let repack_scan = scan(&mut db, &source_a, &[]).expect("scan after move + transcode");
    assert_eq!(repack_scan.added, 0);
    assert_eq!(repack_scan.changed, 1);
    assert_eq!(db.track_count().expect("count after move + transcode"), 2);
    let stored_beta = db
        .track_identity(beta_track_id)
        .expect("query")
        .expect("beta survives move and container change");
    assert_eq!(stored_beta.asset_id, beta_asset_id);
    assert_eq!(stored_beta.path, "Deep/beta-retagged.flac");
    let refreshed_tracks = db.tracks().expect("tracks");
    let beta_row = refreshed_tracks
        .iter()
        .find(|row| row.track_id == beta_track_id)
        .expect("beta track row");
    assert_eq!(beta_row.file.title(), "Beta Club Mix");

    // 3. External move across two separate sources.
    let cross_dir = source_b.join("Crate");
    fs::create_dir_all(&cross_dir).expect("cross_dir");
    let alpha_in_b = cross_dir.join("alpha-club.flac");
    fs::rename(&alpha_renamed, &alpha_in_b).expect("moved across sources");

    let cross_scan = scan(&mut db, &source_b, &[]).expect("scan source_b");
    assert_eq!(cross_scan.added, 0);
    assert_eq!(cross_scan.changed, 1);
    assert_eq!(db.track_count().expect("count after cross-source move"), 2);
    let stored_alpha_b = db
        .track_identity(alpha_track_id)
        .expect("query")
        .expect("alpha survives cross-source move");
    assert_eq!(stored_alpha_b.asset_id, alpha_asset_id);
    assert_eq!(stored_alpha_b.path, "Crate/alpha-club.flac");

    // 4. A live duplicate while the original still exists stays a separate track, and retagging
    //    that duplicate after the original disappears preserves the duplicate's own track_id.
    let duplicate_path = source_b.join("Crate/alpha-duplicate.flac");
    fs::copy(&alpha_in_b, &duplicate_path).expect("copied live duplicate");
    scan(&mut db, &source_b, &[]).expect("scan duplicate");
    identify_pending(&mut db).expect("identify duplicate");
    assert_eq!(db.track_count().expect("three tracks with live copy"), 3);

    let duplicate_track_id = db
        .tracks()
        .expect("tracks")
        .into_iter()
        .find(|row| row.file.path == "Crate/alpha-duplicate.flac")
        .expect("duplicate row")
        .track_id;
    assert_ne!(duplicate_track_id, alpha_track_id);

    fs::remove_file(&alpha_in_b).expect("remove first copy");
    retag_flac_with_artwork(&duplicate_path, "Alpha Duplicate Retagged", "DJ One");
    scan(&mut db, &source_b, &[]).expect("scan retagged duplicate");
    let retag_report = identify_pending(&mut db).expect("identify retagged duplicate");
    assert_eq!(retag_report.reconciled_moves, 0);
    let surviving_dup = db
        .track_identity(duplicate_track_id)
        .expect("query")
        .expect("duplicate keeps its own track_id");
    assert_eq!(surviving_dup.path, "Crate/alpha-duplicate.flac");
}

#[test]
fn chromaprint_matches_shifted_audio_and_separates_distinct_recordings() {
    let root = test_folder("identity_chromaprint_similarity");
    let base = musical_phrase(220.0, 122.0, 16.0, 44_100);
    let shifted = base.delayed_by(22_050); // 500 ms leading silence
    let unrelated = musical_phrase(329.63, 140.0, 16.0, 44_100);

    let base_path = root.join("base.flac");
    let shifted_path = root.join("shifted.wav");
    let unrelated_path = root.join("unrelated.flac");

    base.write_flac(&base_path).expect("base");
    shifted.write_wav(&shifted_path).expect("shifted");
    unrelated.write_flac(&unrelated_path).expect("unrelated");

    let base_fp = compute_identity(&base_path)
        .expect("base id")
        .chromaprint
        .expect("base fp");
    let shifted_fp = compute_identity(&shifted_path)
        .expect("shifted id")
        .chromaprint
        .expect("shifted fp");
    let unrelated_fp = compute_identity(&unrelated_path)
        .expect("unrelated id")
        .chromaprint
        .expect("unrelated fp");

    let same_score = fingerprint_similarity(&base_fp.raw, &shifted_fp.raw, 16);
    let diff_score = fingerprint_similarity(&base_fp.raw, &unrelated_fp.raw, 16);

    assert!(
        same_score > 0.88,
        "expected high Chromaprint similarity for 500 ms shifted copy, got {same_score}"
    );
    assert!(
        diff_score < 0.65,
        "expected low Chromaprint similarity for distinct phrase, got {diff_score}"
    );
}

#[test]
fn decoded_stream_matches_reported_hz_channels_duration_and_catches_truncated_files() {
    let root = test_folder("identity_decoded_stream_truth");
    let stereo_48k = musical_phrase(220.0, 125.0, 4.0, 48_000);
    let mono_44k = Signal::tone(440.0, 3.0, -6.0, 44_100);

    let wav_48k = root.join("club48.wav");
    let aiff_48k = root.join("club48.aiff");
    let flac_44k = root.join("mono44.flac");
    stereo_48k.write_wav(&wav_48k).expect("wav48");
    stereo_48k.write_aiff(&aiff_48k).expect("aiff48");
    mono_44k.write_flac(&flac_44k).expect("flac44");

    let mut db = Database::open_in_memory().expect("db");
    scan(&mut db, &root, &[]).expect("scan");
    for track in db.tracks().expect("tracks") {
        let full_path = root.join(&track.file.path);
        let header = track.file.audio.expect("header facts");
        let id = compute_identity(&full_path).expect("identity");
        let decoded = id.decoded_facts.expect("decoded facts");

        assert_eq!(Some(decoded.sample_rate), header.sample_rate);
        assert_eq!(Some(decoded.channels as u8), header.channels);
        assert_eq!(decoded.decode_errors, 0);
        assert_ne!(decoded.checksum_ok, Some(false));
        assert!(
            decoded.duration_ms().abs_diff(header.duration_ms) <= 5,
            "duration mismatch on {}: decoded {} ms vs header {} ms",
            track.file.path,
            decoded.duration_ms(),
            header.duration_ms
        );
        let measured_kbps = decoded
            .effective_bitrate_kbps(track.file.size_bytes)
            .expect("bitrate");
        if let Some(header_kbps) = header.bitrate_kbps {
            assert!(
                measured_kbps.abs_diff(header_kbps) <= 32,
                "bitrate mismatch on {}: measured {} kbps vs header {} kbps",
                track.file.path,
                measured_kbps,
                header_kbps
            );
        }
    }

    // Truncated WAV: header claims 4.0 s at 48 kHz, file bytes cut in half (~2.0 s).
    let truncated = root.join("truncated.wav");
    let mut bytes = fs::read(&wav_48k).expect("read wav48");
    bytes.truncate(44 + (bytes.len() - 44) / 2);
    fs::write(&truncated, &bytes).expect("write truncated");

    let trunc_id = compute_identity(&truncated).expect("truncated identity");
    let trunc_facts = trunc_id.decoded_facts.expect("truncated decoded facts");
    assert_eq!(trunc_facts.sample_rate, 48_000);
    assert_eq!(trunc_facts.channels, 2);
    assert!(
        trunc_facts.duration_ms().abs_diff(2_000) <= 5,
        "expected ~2000 ms of actual decoded audio, got {} ms",
        trunc_facts.duration_ms()
    );

    // Check one real track of each format in fixtures-local (plus any leading-ID3v2 M4A) when
    // present on this machine; measure_local_fixtures exercises all 50 in release mode.
    let local_fixtures = std::env::var_os("RESONANCE_FIXTURES_LOCAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures-local"));
    if local_fixtures.join("manifest.json").is_file()
        && let Ok(canonical_local) = fs::canonicalize(&local_fixtures)
    {
        let mut local_db = Database::open_in_memory().expect("local db");
        let report = scan(&mut local_db, &canonical_local, &[]).expect("local scan");
        assert_eq!(report.found, 50);
        let mut seen_formats = std::collections::HashSet::new();
        for track in local_db.tracks().expect("local tracks") {
            let is_milk_house = track.file.path.contains("Milk House");
            if !seen_formats.insert(track.file.format) && !is_milk_house {
                continue;
            }
            let Some(header) = track.file.audio else {
                continue;
            };
            let path = resonance_library::identity::full_asset_path(
                &track.source_folder,
                &track.file.path,
            );
            let id = compute_identity(&path).expect("local identity");
            let decoded = id.decoded_facts.expect("local decoded facts");
            if let Some(rate) = header.sample_rate {
                assert_eq!(
                    decoded.sample_rate, rate,
                    "sample rate mismatch on {}",
                    track.file.path
                );
            }
            if let Some(ch) = header.channels {
                assert_eq!(
                    decoded.channels,
                    u16::from(ch),
                    "channel mismatch on {}",
                    track.file.path
                );
            }
            assert_ne!(
                decoded.checksum_ok,
                Some(false),
                "checksum failed on {}",
                track.file.path
            );
            let tolerance_ms = (header.duration_ms / 50).max(1_000);
            assert!(
                decoded.duration_ms().abs_diff(header.duration_ms) <= tolerance_ms,
                "duration drift on {}: decoded {} ms vs header {} ms",
                track.file.path,
                decoded.duration_ms(),
                header.duration_ms
            );
        }
    }
}

fn collect_audio_files(folder: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_audio_files(&path, out);
        } else if path.is_file() && AudioFormat::of(&path).is_some() {
            out.push(path);
        }
    }
}

fn run_fpcalc_line(fpcalc: &Path, args: &[&str], audio: &Path) -> Option<String> {
    let output = Command::new(fpcalc).args(args).arg(audio).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("FINGERPRINT="))
        .map(str::trim)
        .map(str::to_owned)
}

#[test]
#[ignore = "measures throughput and fpcalc parity over fixtures-local"]
fn measure_local_fixtures() {
    let fixtures_root = std::env::var_os("RESONANCE_FIXTURES_LOCAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures-local"));
    let canonical_root = fs::canonicalize(&fixtures_root).expect("fixtures-local exists");

    let mut files = Vec::new();
    collect_audio_files(&canonical_root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "expected audio files in fixtures-local");

    let total_bytes: u64 = files
        .iter()
        .map(|p| fs::metadata(p).expect("meta").len())
        .sum();
    let total_gib = total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_gb_decimal = total_bytes as f64 / 1_000_000_000.0;
    println!(
        "fixtures-local: {} files, {} bytes ({:.3} GiB / {:.3} GB)",
        files.len(),
        total_bytes,
        total_gib,
        total_gb_decimal
    );

    // 1. Real lossless FLAC -> WAV -> AIFF -> ALAC transcode check via ffmpeg if available.
    if let Some(first_flac) = files
        .iter()
        .find(|p| AudioFormat::of(p) == Some(AudioFormat::Flac))
    {
        let transcode_dir = test_folder("identity_real_lossless_transcode");
        let wav = transcode_dir.join("copy.wav");
        let aiff = transcode_dir.join("copy.aiff");
        let alac = transcode_dir.join("copy.m4a");

        let wav_ok = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(first_flac)
            .args(["-map", "0:a:0", "-c:a", "pcm_s24le"])
            .arg(&wav)
            .status()
            .is_ok_and(|s| s.success());
        let aiff_ok = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(first_flac)
            .args(["-map", "0:a:0", "-c:a", "pcm_s24be"])
            .arg(&aiff)
            .status()
            .is_ok_and(|s| s.success());
        let alac_ok = Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(first_flac)
            .args(["-map", "0:a:0", "-c:a", "alac"])
            .arg(&alac)
            .status()
            .is_ok_and(|s| s.success());

        if wav_ok && aiff_ok && alac_ok {
            let id_flac = compute_identity(first_flac).expect("flac id");
            let id_wav = compute_identity(&wav).expect("wav id");
            let id_aiff = compute_identity(&aiff).expect("aiff id");
            let id_alac = compute_identity(&alac).expect("alac id");
            println!(
                "real-lossless source: {}",
                first_flac.file_name().unwrap_or_default().to_string_lossy()
            );
            println!("  flac audio_hash: {:?}", id_flac.audio);
            println!("  wav  audio_hash: {:?}", id_wav.audio);
            println!("  aiff audio_hash: {:?}", id_aiff.audio);
            println!("  alac audio_hash: {:?}", id_alac.audio);
            assert_eq!(id_flac.audio, id_wav.audio);
            assert_eq!(id_flac.audio, id_aiff.audio);
            assert_eq!(id_flac.audio, id_alac.audio);
            assert_ne!(id_flac.byte_hash, id_wav.byte_hash);
            assert_ne!(id_flac.byte_hash, id_alac.byte_hash);
        }
    }

    // 2. Compare rusty-chromaprint against official fpcalc when RESONANCE_FPCALC is supplied.
    let mut our_identities = Vec::with_capacity(files.len());
    for file in &files {
        our_identities.push(compute_identity(file).expect("compute_identity"));
    }
    let decoded_count = our_identities
        .iter()
        .filter(|id| id.audio.is_some())
        .count();
    let fp_count = our_identities
        .iter()
        .filter(|id| id.chromaprint.is_some())
        .count();
    println!(
        "resonance decoded {}/{} files, fingerprinted {}/{} files",
        decoded_count,
        files.len(),
        fp_count,
        files.len()
    );
    assert_eq!(decoded_count, files.len());
    assert_eq!(fp_count, files.len());

    if let Some(fpcalc_path) = std::env::var_os("RESONANCE_FPCALC").map(PathBuf::from)
        && fpcalc_path.exists()
    {
        let cfg = Configuration::preset_test2();
        let mut fpcalc_ok = 0usize;
        let mut wire_exact = 0usize;
        let mut similarities = Vec::new();
        let mut exact_raw = 0usize;

        for (file, ours) in files.iter().zip(&our_identities) {
            let simplified = dunce::simplified(file);
            let Some(raw_line) = run_fpcalc_line(&fpcalc_path, &["-raw"], simplified) else {
                println!(
                    "  fpcalc failed to decode: {}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                );
                continue;
            };
            let Some(encoded_line) = run_fpcalc_line(&fpcalc_path, &[], simplified) else {
                continue;
            };
            let raw_words: Vec<u32> = raw_line
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse::<u32>().expect("u32 word"))
                .collect();
            fpcalc_ok += 1;

            let reencoded =
                encode_base64_url_unpadded(&FingerprintCompressor::from(&cfg).compress(&raw_words));
            if reencoded == encoded_line {
                wire_exact += 1;
            }

            let our_fp = ours.chromaprint.as_ref().expect("our fp");
            if our_fp.raw == raw_words {
                exact_raw += 1;
            }
            let sim = fingerprint_similarity(&our_fp.raw, &raw_words, 8);
            similarities.push((
                sim,
                file.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ));
        }

        similarities.sort_by(|a, b| a.0.total_cmp(&b.0));
        let min = similarities.first().map(|x| x.0).unwrap_or(0.0);
        let median = similarities
            .get(similarities.len() / 2)
            .map(|x| x.0)
            .unwrap_or(0.0);
        let p10 = similarities
            .get(similarities.len() / 10)
            .map(|x| x.0)
            .unwrap_or(0.0);
        let max = similarities.last().map(|x| x.0).unwrap_or(0.0);
        let over_95 = similarities.iter().filter(|(s, _)| *s >= 0.95).count();
        let over_90 = similarities.iter().filter(|(s, _)| *s >= 0.90).count();
        println!(
            "fpcalc parity: {fpcalc_ok}/{} decoded by fpcalc, wire compressor exact {wire_exact}/{fpcalc_ok}, raw exact {exact_raw}/{fpcalc_ok}",
            files.len()
        );
        println!(
            "  bitwise similarity: min={min:.4}, p10={p10:.4}, median={median:.4}, max={max:.4}, >=0.95: {over_95}/{fpcalc_ok}, >=0.90: {over_90}/{fpcalc_ok}"
        );
        for (sim, name) in similarities.iter().take(5) {
            println!("  lowest: {sim:.4} ({name})");
        }
        assert_eq!(wire_exact, fpcalc_ok);
    }

    // 3. Throughput measurements (3 runs each: byte_hash, single-thread full, parallel full).
    let mut byte_runs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        for file in &files {
            std::hint::black_box(compute_byte_hash(file).expect("byte hash"));
        }
        byte_runs.push(start.elapsed().as_secs_f64());
    }

    let mut single_runs = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        for file in &files {
            std::hint::black_box(compute_identity(file).expect("identity"));
        }
        single_runs.push(start.elapsed().as_secs_f64());
    }

    let workers = thread::available_parallelism()
        .map(|n| n.get().min(12))
        .unwrap_or(4);
    let mut parallel_runs = Vec::new();
    for _ in 0..3 {
        let index = AtomicUsize::new(0);
        let start = Instant::now();
        thread::scope(|scope| {
            for _ in 0..workers {
                let index_ref = &index;
                let files_ref = &files;
                scope.spawn(move || {
                    loop {
                        let i = index_ref.fetch_add(1, Ordering::Relaxed);
                        let Some(file) = files_ref.get(i) else {
                            break;
                        };
                        std::hint::black_box(compute_identity(file).expect("identity"));
                    }
                });
            }
        });
        parallel_runs.push(start.elapsed().as_secs_f64());
    }

    let report_series = |label: &str, mut runs: Vec<f64>| {
        let raw = runs.clone();
        runs.sort_by(f64::total_cmp);
        let med = runs[runs.len() / 2];
        let mb_per_s = (total_bytes as f64 / 1_000_000.0) / med;
        let tracks_per_s = files.len() as f64 / med;
        // 113 GB in decimal (113_000 MB) and binary (113 GiB = 121_332.8 MB)
        let proj_113gb_s = (113_000_000_000.0 / total_bytes as f64) * med;
        let proj_113gib_s = ((113.0 * 1024.0 * 1024.0 * 1024.0) / total_bytes as f64) * med;
        println!(
            "{label}: median {:.2} s ({:.2}, {:.2}, {:.2}) | {:.1} MB/s | {:.1} tracks/s | 113 GB: {:.1} min ({:.0} s) | 113 GiB: {:.1} min ({:.0} s)",
            med,
            raw[0],
            raw[1],
            raw[2],
            mb_per_s,
            tracks_per_s,
            proj_113gb_s / 60.0,
            proj_113gb_s,
            proj_113gib_s / 60.0,
            proj_113gib_s
        );
    };

    report_series("byte_hash (1 thread)", byte_runs);
    report_series("full identity (1 thread)", single_runs);
    report_series(&format!("full identity ({workers} threads)"), parallel_runs);
}
