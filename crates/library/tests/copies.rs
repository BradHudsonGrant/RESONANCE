use std::f64::consts::TAU;
use std::fs;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use lofty::tag::{Tag, TagType};
use resonance_core::duplicates::{PairKind, VersionEvidence, shared_segment};
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::copies::{CopiesError, TrackCopies, find_copies, keep_copy};
use resonance_library::identity::identify_pending;
use resonance_library::scan::scan;

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder created");
    folder
}

/// A melody that never repeats: a new chord every quarter second, picked by a seeded generator,
/// so a fingerprint only lines up with itself at one offset.
fn song(seconds: f64, sample_rate: u32, seed: u32) -> Signal {
    let frames = (seconds * f64::from(sample_rate)).round() as usize;
    let step = sample_rate as usize / 4;
    let mut state = seed | 1;
    let mut chords = Vec::with_capacity(frames / step + 1);
    for _ in 0..=frames / step {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let root = 110.0 * 2f64.powf(f64::from(state % 36) / 12.0);
        let third = 2f64.powf(f64::from(3 + (state >> 8) % 2) / 12.0);
        chords.push((root, root * third, root * 1.498_307));
    }
    let samples = (0..frames)
        .map(|frame| {
            let (a, b, c) = chords[frame / step];
            let t = frame as f64 / f64::from(sample_rate);
            let envelope = 0.25 + 0.15 * (-(((frame % step) as f64) / step as f64) * 4.0).exp();
            (envelope
                * ((TAU * a * t).sin() * 0.5
                    + (TAU * b * t).sin() * 0.3
                    + (TAU * c * t).sin() * 0.2)) as f32
        })
        .collect();
    Signal {
        sample_rate,
        channels: 1,
        samples,
    }
}

/// Another encode of the same master: encoder delay and a little quantisation noise, so the
/// bytes and the decoded audio differ while the recording is the same.
fn reencoded(signal: &Signal) -> Signal {
    let pad = signal.sample_rate as usize / 25;
    let mut samples = vec![0.0_f32; pad];
    for (index, &sample) in signal.samples.iter().enumerate() {
        let noise = (((index as u32).wrapping_mul(0x9e37_79b9) >> 24) as f32 / 255.0 - 0.5) * 0.002;
        samples.push((sample * 0.985 + noise).clamp(-0.99, 0.99));
    }
    Signal { samples, ..*signal }
}

fn slice(signal: &Signal, from_s: f64, to_s: f64) -> Signal {
    let rate = f64::from(signal.sample_rate);
    Signal {
        samples: signal.samples[(from_s * rate) as usize..(to_s * rate) as usize].to_vec(),
        ..*signal
    }
}

fn tag(path: &Path, title: &str, artist: &str) {
    let mut tagged = lofty::read_from_path(path).expect("file opens for tagging");
    if tagged.primary_tag_mut().is_none() {
        tagged.insert_tag(Tag::new(TagType::Id3v2));
    }
    let tag = tagged.primary_tag_mut().expect("tag");
    tag.set_title(title.to_owned());
    tag.set_artist(artist.to_owned());
    tagged
        .save_to_path(path, WriteOptions::default())
        .expect("tags saved");
}

fn library(folder: &Path) -> Database {
    let mut database = Database::open_in_memory().expect("database");
    scan(&mut database, folder, &[]).expect("scan");
    identify_pending(&mut database).expect("identify");
    database
}

fn track_of(database: &Database, file_name: &str) -> i64 {
    database
        .tracks()
        .expect("tracks")
        .into_iter()
        .find(|track| track.file.path.ends_with(file_name))
        .unwrap_or_else(|| panic!("{file_name} scanned"))
        .track_id
}

fn kind_of(found: &TrackCopies, file_name: &str) -> PairKind {
    found
        .copies
        .iter()
        .chain(&found.versions)
        .find(|related| related.file.path.ends_with(file_name))
        .unwrap_or_else(|| panic!("{file_name} related: {found:#?}"))
        .kind
}

#[test]
fn copies_sort_into_byte_identical_audio_identical_and_another_encode() {
    let folder = test_folder("copies-tiers");
    let master = song(40.0, 22_050, 7);
    let wav = folder.join("First Light.wav");
    master.write_wav(&wav).expect("wav");
    tag(&wav, "First Light", "Ada Marsh");
    fs::create_dir_all(folder.join("Backup")).expect("backup folder");
    fs::copy(&wav, folder.join("Backup").join("First Light.wav")).expect("copy");
    let flac = folder.join("First Light.flac");
    master.write_flac(&flac).expect("flac");
    let other = folder.join("First Light (bounce).aiff");
    reencoded(&master).write_aiff(&other).expect("aiff");
    let unrelated = folder.join("Low Tide.wav");
    song(40.0, 22_050, 99)
        .write_wav(&unrelated)
        .expect("unrelated");

    let database = library(&folder);
    let found = find_copies(&database, track_of(&database, "Low Tide.wav")).expect("copies");
    assert!(
        found.copies.is_empty() && found.versions.is_empty(),
        "{found:#?}"
    );

    let track = database
        .tracks()
        .expect("tracks")
        .into_iter()
        .find(|track| track.file.path == "First Light.wav")
        .expect("wav scanned")
        .track_id;
    let found = find_copies(&database, track).expect("copies");
    assert_eq!(found.copies.len(), 3, "{found:#?}");
    assert!(found.versions.is_empty());
    assert_eq!(
        kind_of(&found, "Backup/First Light.wav"),
        PairKind::ByteIdentical
    );
    assert_eq!(
        kind_of(&found, "First Light.flac"),
        PairKind::AudioIdentical
    );
    assert!(
        matches!(
            kind_of(&found, "First Light (bounce).aiff"),
            PairKind::SameRecording { similarity } if similarity >= 0.88
        ),
        "{found:#?}"
    );
    assert!(found.keep_asset_id.is_some());
}

#[test]
fn a_radio_edit_and_an_extended_mix_of_one_stem_are_versions_and_never_removed() {
    let folder = test_folder("copies-versions");
    let stem = song(360.0, 22_050, 17);
    let extended = folder.join("First Light (Extended Mix).wav");
    stem.write_wav(&extended).expect("extended");
    tag(&extended, "First Light (Extended Mix)", "Ada Marsh");
    let radio = folder.join("First Light (Radio Edit).wav");
    slice(&stem, 60.0, 240.0)
        .write_wav(&radio)
        .expect("radio edit");
    tag(&radio, "First Light (Radio Edit)", "Ada Marsh");
    let longer = folder.join("First Light.wav");
    slice(&stem, 60.0, 250.0)
        .write_wav(&longer)
        .expect("longer cut");
    tag(&longer, "First Light (Radio Edit)", "Ada Marsh");

    let mut database = library(&folder);
    let radio_track = track_of(&database, "First Light (Radio Edit).wav");
    let found = find_copies(&database, radio_track).expect("copies");
    assert!(found.copies.is_empty(), "{found:#?}");
    assert_eq!(found.keep_asset_id, None);
    assert_eq!(
        kind_of(&found, "First Light (Extended Mix).wav"),
        PairKind::DifferentVersion {
            evidence: VersionEvidence::MixNames
        }
    );
    // The same title 10 s longer is a different length, so a different version.
    assert_eq!(
        kind_of(&found, "First Light.wav"),
        PairKind::DifferentVersion {
            evidence: VersionEvidence::Length
        }
    );

    // The real fingerprints share the minute both cuts hold, whatever the tags say.
    let fingerprint_of = |name: &str| {
        found
            .files
            .iter()
            .chain(&found.versions)
            .find(|related| related.file.path.ends_with(name))
            .and_then(|related| related.file.fingerprint.clone())
            .expect("fingerprinted")
    };
    let segment = shared_segment(
        &fingerprint_of("(Radio Edit).wav"),
        &fingerprint_of("(Extended Mix).wav"),
    )
    .expect("shared material");
    assert!(segment.items >= 400, "{segment:?}");

    // Keeping the extended mix in place of the radio edit is refused: a version is not a copy.
    let extended_asset = found
        .versions
        .iter()
        .find(|related| related.file.path.ends_with("(Extended Mix).wav"))
        .expect("extended mix")
        .file
        .asset_id;
    let mut removed = Vec::new();
    let kept = keep_copy(&mut database, radio_track, extended_asset, |path| {
        removed.push(path.to_owned());
        Ok(())
    });
    assert!(
        matches!(kept, Err(CopiesError::NotACopy { .. })),
        "{kept:?}"
    );
    assert!(removed.is_empty());
    assert_eq!(database.tracks().expect("tracks").len(), 3);
}

#[test]
fn keeping_one_copy_moves_the_others_out_and_keeps_collections_and_versions() {
    let folder = test_folder("copies-keep");
    let master = song(40.0, 22_050, 23);
    let wav = folder.join("First Light.wav");
    master.write_wav(&wav).expect("wav");
    tag(&wav, "First Light", "Ada Marsh");
    let copy = folder.join("First Light copy.wav");
    fs::copy(&wav, &copy).expect("copy");
    let flac = folder.join("First Light.flac");
    master.write_flac(&flac).expect("flac");
    let dub = folder.join("First Light (Dub).wav");
    slice(&master, 5.0, 30.0).write_wav(&dub).expect("dub");
    tag(&dub, "First Light (Dub)", "Ada Marsh");

    let mut database = library(&folder);
    let copy_track = track_of(&database, "First Light copy.wav");
    database
        .set_track_favourite(copy_track, true)
        .expect("favourite");
    let crate_collection = database.create_collection("Warm up").expect("collection");
    database
        .add_tracks_to_collection(crate_collection.id, &[copy_track])
        .expect("added");

    let wav_track = track_of(&database, "First Light.wav");
    let found = find_copies(&database, wav_track).expect("copies");
    assert_eq!(found.copies.len(), 2, "{found:#?}");
    assert_eq!(found.versions.len(), 1, "{found:#?}");
    let keep = found.files[0].file.asset_id;

    let mut removed = Vec::new();
    let kept = keep_copy(&mut database, copy_track, keep, |path| {
        removed.push(path.to_owned());
        fs::remove_file(path)
    })
    .expect("kept");
    assert_eq!(kept.track_id, wav_track);
    assert_eq!(removed.len(), 2);
    assert!(wav.is_file() && dub.is_file());
    assert!(!copy.exists() && !flac.exists());

    let tracks = database.tracks().expect("tracks");
    assert_eq!(tracks.len(), 2);
    let kept_row = tracks
        .iter()
        .find(|track| track.track_id == wav_track)
        .expect("kept track");
    assert!(kept_row.favourite);
    assert_eq!(
        kept_row
            .collections
            .iter()
            .map(|collection| collection.id)
            .collect::<Vec<_>>(),
        vec![crate_collection.id]
    );
    let after = find_copies(&database, wav_track).expect("copies");
    assert!(after.copies.is_empty());
    assert_eq!(after.versions.len(), 1);
}
