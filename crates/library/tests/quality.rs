use std::f64::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};

use resonance_core::columns::LibraryColumn;
use resonance_core::filter::{Condition, FieldFilter, FilterTerm};
use resonance_core::identity::AssetQuality;
use resonance_core::quality::{FlagCertainty, QualityFlag, QualityKind};
use resonance_core::search::TrackQuery;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::identity::{compute_identity, identify_pending};
use resonance_library::scan::scan;

const RATE: u32 = 44_100;

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder created");
    folder
}

fn quality_of(path: &Path) -> AssetQuality {
    compute_identity(path)
        .expect("identity")
        .quality
        .expect("quality checked")
}

fn flags_of(path: &Path, kind: QualityKind) -> Vec<QualityFlag> {
    quality_of(path)
        .flags
        .into_iter()
        .filter(|flag| flag.kind == kind)
        .collect()
}

/// Sines every 100 Hz from 100 Hz up to `top_hz`, with phases scattered so the sum stays under
/// full scale. `gain` sets each sine's level from its frequency.
fn comb(top_hz: f64, seconds: f64, gain: impl Fn(f64) -> f64) -> Signal {
    let mut seed: u32 = 0x9E37_79B9;
    let partials: Vec<(f64, f64, f64)> = (1..)
        .map(|step| f64::from(step) * 100.0)
        .take_while(|&hz| hz < top_hz)
        .map(|hz| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let phase = f64::from(seed) / f64::from(u32::MAX) * TAU;
            (TAU * hz / f64::from(RATE), phase, 0.008 * gain(hz))
        })
        .collect();
    let frames = (seconds * f64::from(RATE)) as usize;
    let samples = (0..frames)
        .map(|n| {
            partials
                .iter()
                .map(|&(step, phase, level)| level * (step * n as f64 + phase).sin())
                .sum::<f64>() as f32
        })
        .collect();
    Signal {
        sample_rate: RATE,
        channels: 1,
        samples,
    }
}

#[test]
fn a_clean_tone_measures_its_loudness_and_earns_no_flags() {
    let folder = test_folder("quality-clean");
    let path = folder.join("tone.wav");
    Signal::tone_at_loudness(-14.0, 20.0, RATE)
        .write_wav(&path)
        .expect("wav");

    let quality = quality_of(&path);
    assert!(quality.flags.is_empty(), "{:?}", quality.flags);
    let measured = quality.measurements;
    let lufs = measured.integrated_lufs.expect("loudness");
    assert!((lufs + 14.0).abs() < 0.5, "integrated {lufs} LUFS");
    let peak = measured.sample_peak_dbfs.expect("sample peak");
    assert!((peak + 14.0).abs() < 0.1, "sample peak {peak} dBFS");
    let true_peak = measured.true_peak_dbtp.expect("true peak");
    assert!(
        true_peak >= peak - 0.01 && true_peak < -13.5,
        "true peak {true_peak}"
    );
    assert!(measured.loudness_range_lu.expect("range") < 1.0);
    assert_eq!(measured.clipped_runs, 0);
    assert_eq!(measured.dropouts, 0);
}

#[test]
fn a_truncated_wav_is_confirmed_damaged() {
    let folder = test_folder("quality-truncated");
    let path = folder.join("cut.wav");
    Signal::tone(440.0, 10.0, -6.0, RATE)
        .write_wav(&path)
        .expect("wav");
    let bytes = fs::read(&path).expect("read");
    fs::write(&path, &bytes[..44 + (RATE as usize * 2 * 4)]).expect("truncate");

    let damaged = flags_of(&path, QualityKind::Damaged);
    assert_eq!(damaged.len(), 1, "{damaged:?}");
    assert_eq!(damaged[0].certainty, FlagCertainty::Confirmed);
    assert_eq!(
        damaged[0].reason,
        "The file says it lasts 0:10 but only 0:04 decodes"
    );
}

#[test]
fn a_flac_whose_audio_fails_its_md5_is_confirmed_damaged() {
    let folder = test_folder("quality-md5");
    let path = folder.join("bad-md5.flac");
    Signal::tone(440.0, 3.0, -6.0, RATE)
        .write_flac(&path)
        .expect("flac");
    let mut bytes = fs::read(&path).expect("read");
    // The MD5 sits in the last 16 bytes of the 34-byte stream info, after the 4-byte marker and
    // 4-byte block header.
    bytes[26..42].copy_from_slice(&[0xA5; 16]);
    fs::write(&path, bytes).expect("write");

    let damaged = flags_of(&path, QualityKind::Damaged);
    assert_eq!(damaged.len(), 1, "{damaged:?}");
    assert_eq!(damaged[0].certainty, FlagCertainty::Confirmed);
    assert_eq!(
        damaged[0].reason,
        "The audio does not match the checksum stored in the file"
    );
}

#[test]
fn a_hard_clipped_sine_is_confirmed_clipping() {
    let folder = test_folder("quality-clipped");
    let path = folder.join("clipped.wav");
    let mut signal = Signal::tone(100.0, 2.0, 6.0, RATE);
    for sample in &mut signal.samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    signal.write_wav(&path).expect("wav");

    let quality = quality_of(&path);
    assert!(quality.measurements.clipped_runs >= 100);
    let clipping: Vec<_> = quality
        .flags
        .iter()
        .filter(|flag| flag.kind == QualityKind::Clipping)
        .collect();
    assert_eq!(clipping.len(), 1);
    assert_eq!(clipping[0].certainty, FlagCertainty::Confirmed);
}

#[test]
fn a_gap_spliced_into_a_tone_is_a_likely_dropout() {
    let folder = test_folder("quality-dropout");
    let path = folder.join("dropout.wav");
    let mut signal = Signal::tone(1_000.0, 3.0, -6.0, RATE);
    // 1 kHz peaks 11 samples into each cycle, so the cut lands on a steep edge.
    let start = RATE as usize + 11;
    signal.samples[start..start + RATE as usize / 10].fill(0.0);
    signal.write_wav(&path).expect("wav");

    let dropouts = flags_of(&path, QualityKind::Dropout);
    assert_eq!(dropouts.len(), 1, "{dropouts:?}");
    assert_eq!(dropouts[0].certainty, FlagCertainty::Likely);
    assert_eq!(
        dropouts[0].reason,
        "The sound cuts out abruptly 1 time, the first at 0:01"
    );
}

#[test]
fn a_flac_cut_off_at_16_khz_is_a_likely_transcode() {
    let folder = test_folder("quality-transcode");
    let path = folder.join("from-mp3.flac");
    comb(16_000.0, 8.0, |_| 1.0)
        .write_flac(&path)
        .expect("flac");

    let quality = quality_of(&path);
    let cutoff = quality
        .measurements
        .spectral_cutoff_hz
        .expect("cutoff found");
    assert!((15_800.0..=16_200.0).contains(&cutoff), "cutoff {cutoff}");
    let transcode: Vec<_> = quality
        .flags
        .iter()
        .filter(|flag| flag.kind == QualityKind::Transcode)
        .collect();
    assert_eq!(transcode.len(), 1);
    assert_eq!(transcode[0].certainty, FlagCertainty::Likely);
    assert!(transcode[0].reason.contains("128 kbps MP3"));
}

#[test]
fn a_gentle_low_pass_is_never_called_a_transcode() {
    let folder = test_folder("quality-gentle");
    let path = folder.join("warm.flac");
    // 12 dB an octave above 2 kHz, as a record's high end might fall away.
    comb(21_500.0, 8.0, |hz| (2_000.0 / hz).min(1.0).powi(2))
        .write_flac(&path)
        .expect("flac");

    let quality = quality_of(&path);
    assert_eq!(quality.measurements.spectral_cutoff_hz, None);
    assert!(
        quality
            .flags
            .iter()
            .all(|flag| flag.kind != QualityKind::Transcode),
        "{:?}",
        quality.flags
    );
}

#[test]
fn a_lossless_file_below_44_khz_is_not_checked_for_a_transcode() {
    let folder = test_folder("quality-low-rate");
    let path = folder.join("low-rate.wav");
    let mut signal = comb(10_000.0, 4.0, |_| 1.0);
    signal.sample_rate = 32_000;
    signal.write_wav(&path).expect("wav");
    assert_eq!(quality_of(&path).measurements.spectral_cutoff_hz, None);
}

#[test]
fn identifying_a_library_stores_quality_beside_identity() {
    let folder = test_folder("quality-library");
    let music = folder.join("music");
    fs::create_dir_all(&music).expect("music");
    Signal::tone_at_loudness(-10.0, 5.0, RATE)
        .write_wav(&music.join("clean.wav"))
        .expect("clean");
    let mut clipped = Signal::tone(100.0, 2.0, 6.0, RATE);
    for sample in &mut clipped.samples {
        *sample = sample.clamp(-1.0, 1.0);
    }
    clipped
        .write_wav(&music.join("clipped.wav"))
        .expect("clipped");

    let mut db = Database::open_in_memory().expect("db");
    scan(&mut db, &music, &[]).expect("scan");
    let report = identify_pending(&mut db).expect("identify");
    assert_eq!(report.processed, 2);
    assert_eq!(identify_pending(&mut db).expect("again").processed, 0);

    let tracks = db.tracks().expect("tracks");
    for track in &tracks {
        let stored = db
            .track_quality(track.track_id)
            .expect("read")
            .expect("checked");
        if track.file.path.ends_with("clipped.wav") {
            assert_eq!(stored.flags.len(), 1);
            assert_eq!(stored.flags[0].kind, QualityKind::Clipping);
            assert_eq!(track.quality.as_ref(), stored.flags.first());
        } else {
            assert!(stored.flags.is_empty());
            assert_eq!(track.quality, None);
            let lufs = stored.measurements.integrated_lufs.expect("loudness");
            assert!((lufs + 10.0).abs() < 0.5);
        }
    }

    let paths = |condition: Condition| -> Vec<String> {
        let query = TrackQuery {
            filters: vec![FilterTerm::one(
                FieldFilter::new(LibraryColumn::Quality, condition).expect("filter"),
            )],
            ..TrackQuery::default()
        };
        db.query_tracks_page(&query, 0, 10)
            .expect("page")
            .into_iter()
            .map(|row| row.file.path)
            .collect()
    };
    assert_eq!(paths(Condition::Present), ["clipped.wav"]);
    assert_eq!(paths(Condition::Missing), ["clean.wav"]);
    assert_eq!(
        paths(Condition::AnyOf(vec!["clipping".into()])),
        ["clipped.wav"]
    );
    assert_eq!(
        paths(Condition::AnyOf(vec!["transcode".into()])),
        Vec::<String>::new()
    );
    assert_eq!(
        paths(Condition::NoneOf(vec!["clipping".into()])),
        ["clean.wav"]
    );
}
