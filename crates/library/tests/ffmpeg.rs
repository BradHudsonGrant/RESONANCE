use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use resonance_core::convert::ConvertPreset;
use resonance_fixtures::signal::Groove;
use resonance_library::ffmpeg::{FfmpegError, locate};
use resonance_library::file_tags;
use resonance_library::identity::compute_identity;

const GROOVE: Groove = Groove {
    tempo_bpm: 126.0,
    seed: 3,
};

fn folder(test: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder");
    folder
}

/// A groove pushed to full scale, so a converter has to turn it down to meet a ceiling.
fn loud_wav(path: &Path, sample_rate: u32) {
    let mut signal = GROOVE.render(30.0, sample_rate, 0.0, 1.0);
    let peak = signal
        .samples
        .iter()
        .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    for sample in &mut signal.samples {
        *sample /= peak;
    }
    signal.write_wav(path).expect("WAV written");
}

#[test]
fn ffmpeg_is_found_with_its_version() {
    let ffmpeg = locate().expect("FFmpeg is installed for these tests");
    assert!(ffmpeg.version.starts_with("ffmpeg version"));
    assert!(ffmpeg.program.is_file());
}

#[test]
fn the_mp3_preset_holds_true_peak_under_minus_one_and_matches_its_source() {
    let folder = folder("ffmpeg-convert-mp3");
    let source = folder.join("loud.wav");
    loud_wav(&source, 44_100);
    let ffmpeg = locate().expect("FFmpeg is installed");
    let made = ffmpeg
        .convert(&source, ConvertPreset::Mp3Cbr320, &folder.join("out"))
        .expect("converted");
    let peak = made
        .identity
        .quality
        .as_ref()
        .and_then(|quality| quality.measurements.true_peak_dbtp)
        .expect("true peak measured");
    let audio = file_tags::read(&made.path).expect("read").audio;
    println!(
        "true peak {peak:.2} dBTP, similarity {:.3}",
        made.similarity
    );
    assert!(peak < -1.0, "true peak {peak} dBTP");
    assert_eq!(audio.bitrate_kbps, Some(320));
    assert!(made.similarity >= 0.88);
    assert!(source.is_file(), "the source is left alone");
}

#[test]
fn the_lossless_presets_write_the_formats_they_name() {
    let folder = folder("ffmpeg-convert-lossless");
    let source = folder.join("source.wav");
    loud_wav(&source, 48_000);
    let ffmpeg = locate().expect("FFmpeg is installed");

    let flac = ffmpeg
        .convert(&source, ConvertPreset::Flac24, &folder.join("flac"))
        .expect("FLAC");
    let flac_audio = file_tags::read(&flac.path).expect("read").audio;
    assert_eq!(flac.path.extension().and_then(|e| e.to_str()), Some("flac"));
    assert_eq!(flac_audio.bit_depth, Some(24));
    assert_eq!(flac_audio.sample_rate, Some(48_000));

    let aiff = ffmpeg
        .convert(&source, ConvertPreset::AiffCdj, &folder.join("aiff"))
        .expect("AIFF");
    let aiff_audio = file_tags::read(&aiff.path).expect("read").audio;
    assert_eq!(aiff.path.extension().and_then(|e| e.to_str()), Some("aiff"));
    assert_eq!(aiff_audio.bit_depth, Some(16));
    assert_eq!(aiff_audio.sample_rate, Some(44_100));
}

/// An ID3v2.3 tag holding one title frame, as some download tools put in front of an M4A.
fn id3v2_block() -> Vec<u8> {
    let title = b"\x00Stray tag";
    let mut frame = b"TIT2".to_vec();
    frame.extend_from_slice(&u32::try_from(title.len()).expect("small").to_be_bytes());
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(title);
    let size = u32::try_from(frame.len()).expect("small");
    let synchsafe = [
        ((size >> 21) & 0x7f) as u8,
        ((size >> 14) & 0x7f) as u8,
        ((size >> 7) & 0x7f) as u8,
        (size & 0x7f) as u8,
    ];
    let mut block = b"ID3\x03\x00\x00".to_vec();
    block.extend_from_slice(&synchsafe);
    block.extend_from_slice(&frame);
    block
}

#[test]
fn repair_strips_stray_id3v2_bytes_in_front_of_an_m4a_and_keeps_the_audio() {
    let folder = folder("ffmpeg-repair-m4a");
    let wav = folder.join("source.wav");
    loud_wav(&wav, 44_100);
    let clean = folder.join("clean.m4a");
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(&wav)
        .args(["-c:a", "aac", "-b:a", "192k"])
        .arg(&clean)
        .status()
        .expect("FFmpeg runs");
    assert!(status.success());
    let broken = folder.join("broken.m4a");
    let mut bytes = id3v2_block();
    bytes.extend_from_slice(&fs::read(&clean).expect("read"));
    fs::write(&broken, bytes).expect("written");

    let before = compute_identity(&broken).expect("identity").audio;
    assert!(before.is_some(), "the library still reads the broken file");
    let ffmpeg = locate().expect("FFmpeg is installed");
    let repaired = ffmpeg
        .repair(&broken, &folder.join("repaired"))
        .expect("repaired");
    let start = fs::read(&repaired.path).expect("read");
    assert_ne!(&start[..3], b"ID3");
    assert_eq!(&start[4..8], b"ftyp");
    assert_eq!(repaired.identity.audio, before);
    assert!(broken.is_file(), "the original is left alone");
}

#[test]
fn a_file_that_does_not_decode_is_refused() {
    let folder = folder("ffmpeg-refuse");
    let source = folder.join("noise.mp3");
    fs::write(&source, vec![0x55u8; 64 * 1024]).expect("written");
    let ffmpeg = locate().expect("FFmpeg is installed");
    let outcome = ffmpeg.convert(&source, ConvertPreset::Flac24, &folder.join("out"));
    assert!(matches!(
        outcome,
        Err(FfmpegError::Failed { .. } | FfmpegError::NotVerified { .. })
    ));
    assert!(!folder.join("out").join("noise.flac").exists());
}
