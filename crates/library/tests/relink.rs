use std::f64::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lofty::config::WriteOptions;
use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use lofty::tag::{Tag, TagType};
use resonance_core::identity::RelinkMatchKind;
use resonance_core::library::{AudioFormat, TagField};
use resonance_core::search::TrackQuery;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::identity::identify_pending;
use resonance_library::relink::{
    RelinkError, apply_relink, find_relink_candidates, preview_relink,
};
use resonance_library::scan::scan;

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder created");
    folder
}

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

/// Simulates encoder delay + subtle lossy quantisation noise so `byte_hash` and `audio_hash`
/// differ while Chromaprint still recognises the recording on every CI runner.
fn padded_lossy_like_variant(signal: &Signal) -> Signal {
    let pad_frames = (signal.sample_rate as usize) / 25; // 40 ms leading padding
    let mut samples = vec![0.0_f32; pad_frames * usize::from(signal.channels)];
    for (idx, &sample) in signal.samples.iter().enumerate() {
        let dither = (((idx as u32).wrapping_mul(0x9e37_79b9) >> 24) as f32 / 255.0 - 0.5) * 0.002;
        samples.push((sample * 0.985 + dither).clamp(-0.99, 0.99));
    }
    Signal {
        sample_rate: signal.sample_rate,
        channels: signal.channels,
        samples,
    }
}

fn tag_wav(path: &Path, title: &str, artist: &str) {
    let mut tagged = lofty::read_from_path(path).expect("wav opens for tagging");
    let tag = match tagged.primary_tag_mut() {
        Some(tag) => tag,
        None => {
            tagged.insert_tag(Tag::new(TagType::Id3v2));
            tagged.primary_tag_mut().expect("inserted id3v2 tag")
        }
    };
    tag.set_title(title.to_owned());
    tag.set_artist(artist.to_owned());
    tagged
        .save_to_path(path, WriteOptions::default())
        .expect("wav tags saved");
}

#[test]
fn deleted_wav_flags_missing_and_relinks_to_reencoded_copy_preserving_id_favourite_and_collections()
{
    let root = test_folder("relink_chromaprint_preserve");
    let source_a = root.join("Club");
    let source_b = root.join("Archive");
    fs::create_dir_all(&source_a).unwrap();
    fs::create_dir_all(&source_b).unwrap();

    let phrase = musical_phrase(220.0, 126.0, 10.0, 44_100);
    let wav_path = source_a.join("Ada Marsh - First Light.wav");
    phrase.write_wav(&wav_path).unwrap();
    tag_wav(&wav_path, "First Light", "Ada Marsh");

    let mut db = Database::open_in_memory().unwrap();
    scan(&mut db, &source_a, &[]).unwrap();
    identify_pending(&mut db).unwrap();

    let initial = db.tracks().unwrap();
    assert_eq!(initial.len(), 1);
    let original_track_id = initial[0].track_id;
    assert_eq!(initial[0].missing_since, None);

    db.set_track_favourite(original_track_id, true).unwrap();
    let crate_col = db.create_collection("Peak Time").unwrap();
    db.add_tracks_to_collection(crate_col.id, &[original_track_id])
        .unwrap();

    // Build a re-encoded copy (real MP3 via ffmpeg when available, otherwise padded lossy AIFF)
    // BEFORE deleting the WAV, then delete the original WAV.
    let mp3_path = source_b.join("Ada Marsh - First Light (320).mp3");
    let ffmpeg_ok = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-i",
            wav_path.to_str().unwrap(),
            "-map_metadata",
            "-1",
            "-b:a",
            "192k",
            mp3_path.to_str().unwrap(),
        ])
        .status()
        .is_ok_and(|status| status.success());

    let (reencoded_path, expected_format) = if ffmpeg_ok && mp3_path.is_file() {
        (mp3_path, AudioFormat::Mp3)
    } else {
        let aiff_path = source_b.join("Ada Marsh - First Light (bounce).aiff");
        padded_lossy_like_variant(&phrase)
            .write_aiff(&aiff_path)
            .unwrap();
        (aiff_path, AudioFormat::Aiff)
    };

    fs::remove_file(&wav_path).unwrap();
    let rescan = scan(&mut db, &source_a, &[]).unwrap();
    assert_eq!(rescan.missing_changed, 1);

    // Missing track is NEVER auto-removed; missing_since and missing_only filter surface it.
    let after_delete = db.tracks().unwrap();
    assert_eq!(after_delete.len(), 1);
    assert_eq!(after_delete[0].track_id, original_track_id);
    assert!(after_delete[0].missing_since.is_some());
    assert_eq!(db.library_facets().unwrap().missing_count, 1);
    assert_eq!(
        db.query_track_count(&TrackQuery {
            missing_only: true,
            ..TrackQuery::default()
        })
        .unwrap(),
        1
    );

    // Scan source_b: because audio_hash differs, automatic exact reconciliation leaves it as a
    // second track row, and Chromaprint candidate discovery ranks it as SameRecording.
    scan(&mut db, &source_b, &[]).unwrap();
    identify_pending(&mut db).unwrap();
    assert_eq!(db.track_count().unwrap(), 2);

    let candidates = find_relink_candidates(&db, original_track_id, None).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(matches!(
        candidates[0].verdict,
        RelinkMatchKind::SameRecording { similarity } if similarity >= 0.85
    ));
    assert!(candidates[0].existing_track_id.is_some());

    let preview = apply_relink(&mut db, original_track_id, &reencoded_path, false).unwrap();
    assert!(!preview.requires_confirmation);
    assert_eq!(preview.candidate.format, expected_format);

    // Duplicate scanned track was merged and cleaned up; original track_id, favourite, collection,
    // and fallback tags remain 100% intact, and missing_since is cleared.
    let final_tracks = db.tracks().unwrap();
    assert_eq!(final_tracks.len(), 1);
    let kept = &final_tracks[0];
    assert_eq!(kept.track_id, original_track_id);
    assert_eq!(kept.missing_since, None);
    assert!(kept.favourite);
    assert_eq!(kept.collections.len(), 1);
    assert_eq!(kept.collections[0].name, "Peak Time");
    assert_eq!(kept.file.format, expected_format);
    assert_eq!(kept.file.title(), "First Light");
    assert!(
        kept.file
            .tags
            .iter()
            .any(|(f, v)| *f == TagField::Artist && v == "Ada Marsh")
    );
    assert_eq!(db.library_facets().unwrap().missing_count, 0);
}

#[test]
fn different_recording_warns_until_forced_and_external_folder_search_ranks_candidates() {
    let root = test_folder("relink_mismatch_and_external");
    let library_dir = root.join("Library");
    let external_dir = root.join("ExternalBackup");
    fs::create_dir_all(&library_dir).unwrap();
    fs::create_dir_all(&external_dir).unwrap();

    let song_a = musical_phrase(220.0, 124.0, 10.0, 44_100);
    let song_b = musical_phrase(370.0, 140.0, 10.0, 44_100);

    let orig_wav = library_dir.join("Track A.wav");
    song_a.write_wav(&orig_wav).unwrap();

    let mut db = Database::open_in_memory().unwrap();
    scan(&mut db, &library_dir, &[]).unwrap();
    identify_pending(&mut db).unwrap();
    let track_id = db.tracks().unwrap()[0].track_id;

    // Place an exact byte copy, a lossless AIFF copy, a lossy-like variant, and an unrelated track
    // in an un-scanned external folder, then delete the original WAV.
    let exact_copy = external_dir.join("03_exact_copy.wav");
    fs::copy(&orig_wav, &exact_copy).unwrap();
    let exact_audio = external_dir.join("02_transcode.aiff");
    song_a.write_aiff(&exact_audio).unwrap();
    let same_recording = external_dir.join("01_lossy_variant.wav");
    padded_lossy_like_variant(&song_a)
        .write_wav(&same_recording)
        .unwrap();
    let unrelated = external_dir.join("00_different_song.wav");
    song_b.write_wav(&unrelated).unwrap();

    fs::remove_file(&orig_wav).unwrap();
    scan(&mut db, &library_dir, &[]).unwrap();
    assert!(
        db.track_by_id(track_id)
            .unwrap()
            .unwrap()
            .missing_since
            .is_some()
    );

    // Folder search excludes the unrelated song and ranks ExactBytes -> ExactAudio -> SameRecording.
    let ranked = find_relink_candidates(&db, track_id, Some(&external_dir)).unwrap();
    assert_eq!(ranked.len(), 3);
    assert_eq!(ranked[0].verdict, RelinkMatchKind::ExactBytes);
    assert_eq!(ranked[1].verdict, RelinkMatchKind::ExactAudio);
    assert!(matches!(
        ranked[2].verdict,
        RelinkMatchKind::SameRecording { .. }
    ));

    // Previewing the unrelated track flags DifferentRecording and apply_relink refuses without force.
    let mismatch_preview = preview_relink(&db, track_id, &unrelated).unwrap();
    assert!(mismatch_preview.requires_confirmation);
    assert!(matches!(
        mismatch_preview.verdict,
        RelinkMatchKind::DifferentRecording { .. }
    ));

    let refused = apply_relink(&mut db, track_id, &unrelated, false);
    assert!(matches!(
        refused,
        Err(RelinkError::ConfirmationRequired { .. })
    ));
    assert!(
        db.track_by_id(track_id)
            .unwrap()
            .unwrap()
            .missing_since
            .is_some()
    );

    // Confirming with force = true relinks to the external file without importing sibling files.
    apply_relink(&mut db, track_id, &unrelated, true).unwrap();
    let updated = db.track_by_id(track_id).unwrap().unwrap();
    assert_eq!(updated.missing_since, None);
    assert_eq!(updated.file.path, "00_different_song.wav");
    assert_eq!(db.track_count().unwrap(), 1);
}
