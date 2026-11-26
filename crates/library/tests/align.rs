use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use resonance_fixtures::signal::Groove;
use resonance_library::align::measure_alignment;

const GROOVE: Groove = Groove {
    tempo_bpm: 124.0,
    seed: 7,
};

fn folder(test: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder");
    folder
}

/// Encodes `wav` the way a shop's MP3 would be: LAME through FFmpeg, with the encoder's delay
/// and padding recorded in its header.
fn mp3_from(wav: &Path, mp3: &Path, kbps: u32) {
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(wav)
        .args(["-c:a", "libmp3lame", "-b:a", &format!("{kbps}k")])
        .arg(mp3)
        .status()
        .expect("FFmpeg is installed for the alignment tests");
    assert!(status.success(), "FFmpeg encoded the MP3");
}

fn error_ms(measured_s: f64, expected_s: f64) -> f64 {
    (measured_s - expected_s).abs() * 1000.0
}

#[test]
fn known_shifts_are_measured_within_a_millisecond() {
    let folder = folder("align-known-shifts");
    let old = folder.join("old.wav");
    GROOVE
        .render(60.0, 44_100, 0.0, 1.0)
        .write_wav(&old)
        .expect("old written");
    for shift_ms in [0.5, 3.2, 47.8] {
        let new = folder.join(format!("new-{shift_ms}.wav"));
        GROOVE
            .render(60.0, 44_100, shift_ms / 1000.0, 1.0)
            .write_wav(&new)
            .expect("new written");
        let measured = measure_alignment(&old, &new).expect("measured");
        let error = error_ms(measured.new_at_old_zero_s, shift_ms / 1000.0);
        println!(
            "shift {shift_ms} ms: measured {:.4} ms, error {error:.4} ms, confidence {:.3}",
            measured.new_at_old_zero_s * 1000.0,
            measured.confidence
        );
        assert!(error < 1.0, "{shift_ms} ms shift measured {error} ms out");
        assert!(measured.is_confident());
        assert_eq!(measured.old_per_new, 1.0);
    }
}

#[test]
fn a_flac_at_another_sample_rate_lines_up_within_a_millisecond() {
    let folder = folder("align-other-rate");
    let old = folder.join("old.wav");
    GROOVE
        .render(60.0, 44_100, 0.0, 1.0)
        .write_wav(&old)
        .expect("old written");
    let new = folder.join("new.flac");
    GROOVE
        .render(60.0, 48_000, 0.0032, 1.0)
        .write_flac(&new)
        .expect("new written");
    let measured = measure_alignment(&old, &new).expect("measured");
    let error = error_ms(measured.new_at_old_zero_s, 0.0032);
    println!("48 kHz FLAC: error {error:.4} ms");
    assert!(error < 1.0, "measured {error} ms out");
    assert!(measured.is_confident());
}

#[test]
fn an_mp3_with_encoder_padding_lines_up_with_a_flac_within_a_millisecond() {
    let folder = folder("align-mp3-padding");
    let master = folder.join("master.wav");
    GROOVE
        .render(60.0, 44_100, 0.0, 1.0)
        .write_wav(&master)
        .expect("master written");
    let old = folder.join("old.mp3");
    mp3_from(&master, &old, 128);
    let new = folder.join("new.flac");
    GROOVE
        .render(60.0, 44_100, 0.0478, 1.0)
        .write_flac(&new)
        .expect("new written");
    let measured = measure_alignment(&old, &new).expect("measured");
    let error = error_ms(measured.new_at_old_zero_s, 0.0478);
    println!(
        "128 kbps MP3 against FLAC: error {error:.4} ms, confidence {:.3}",
        measured.confidence
    );
    assert!(error < 1.0, "measured {error} ms out");
    assert!(measured.is_confident());
}

#[test]
fn a_rip_at_another_speed_gets_a_tempo_ratio() {
    let folder = folder("align-speed");
    let old = folder.join("old.wav");
    GROOVE
        .render(150.0, 44_100, 0.0, 1.0)
        .write_wav(&old)
        .expect("old written");
    let new = folder.join("new.wav");
    // The rip plays 0.2% slow and starts 1.5 s in.
    GROOVE
        .render(150.0, 44_100, 1.5, 0.998)
        .write_wav(&new)
        .expect("new written");
    let measured = measure_alignment(&old, &new).expect("measured");
    println!(
        "speed: old per new {:.6}, start {:.4} s",
        measured.old_per_new, measured.new_at_old_zero_s
    );
    assert!((measured.old_per_new - 0.998).abs() < 1e-4);
    for old_s in [10.0, 70.0, 130.0] {
        let expected = 1.5 + old_s / 0.998;
        let error = error_ms(measured.new_seconds(old_s), expected);
        assert!(error < 5.0, "{old_s} s lands {error} ms out");
    }
}

#[test]
fn unrelated_music_does_not_line_up() {
    let folder = folder("align-unrelated");
    let old = folder.join("old.wav");
    GROOVE
        .render(60.0, 44_100, 0.0, 1.0)
        .write_wav(&old)
        .expect("old written");
    let new = folder.join("new.wav");
    Groove {
        tempo_bpm: 128.0,
        seed: 99,
    }
    .render(60.0, 44_100, 0.0, 1.0)
    .write_wav(&new)
    .expect("new written");
    let measured = measure_alignment(&old, &new);
    assert!(measured.is_err() || measured.is_ok_and(|m| !m.is_confident()));
}
