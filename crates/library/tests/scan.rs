use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lofty::config::WriteOptions;
use lofty::prelude::{ItemKey, TagExt, TaggedFileExt};
use lofty::tag::Tag;
use resonance_core::library::{AudioFormat, TagField};
use resonance_db::{Database, TrackRow};
use resonance_fixtures::signal::{ClickTrack, Signal};
use resonance_fixtures::silence::{write_silent_m4a, write_silent_mp3};
use resonance_library::scan::{ScanReport, scan};

struct Library {
    music: PathBuf,
    program: PathBuf,
}

fn folder_for(path: &Path) {
    fs::create_dir_all(path.parent().expect("the file has a folder"))
        .expect("the folder can be made");
}

fn write_tags(path: &Path, fields: &[(ItemKey, &str)]) {
    let kind = lofty::read_from_path(path)
        .expect("the fixture reads")
        .primary_tag_type();
    let mut tag = Tag::new(kind);
    for (key, text) in fields {
        assert!(
            tag.insert_text(*key, (*text).to_owned()),
            "{key:?} cannot be written to {}",
            path.display()
        );
    }
    tag.save_to_path(path, WriteOptions::default())
        .expect("the tags save");
}

fn tone() -> Signal {
    Signal::tone(440.0, 1.5, -6.0, 48_000)
}

/// A music folder with a track in each format, the folders a scan must stay out of, and a copy
/// of the app inside it, as on the machine this is built on.
fn library(name: &str) -> Library {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("scan")
        .join(name);
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");

    let mp3 = music.join("House/first.mp3");
    folder_for(&mp3);
    write_silent_mp3(&mp3, 2.0).expect("the MP3 writes");
    write_tags(
        &mp3,
        &[
            (ItemKey::TrackTitle, "First Light"),
            (ItemKey::TrackArtist, "Ada Marsh"),
            (ItemKey::AlbumTitle, "Early Hours"),
            (ItemKey::AlbumArtist, "Various Artists"),
            (ItemKey::Genre, "House"),
            (ItemKey::RecordingDate, "2019-05-03"),
            (ItemKey::TrackNumber, "3"),
            (ItemKey::IntegerBpm, "124"),
            (ItemKey::InitialKey, "8A"),
            (ItemKey::Label, "Night Shift"),
            (ItemKey::Comment, "Warm-up"),
        ],
    );

    let flac = music.join("House/EP/second.flac");
    folder_for(&flac);
    ClickTrack::at_tempo(128.0, 8, 44_100)
        .signal
        .write_flac(&flac)
        .expect("the FLAC writes");
    write_tags(
        &flac,
        &[
            (ItemKey::TrackTitle, "Second Wind"),
            (ItemKey::TrackArtist, "Ada Marsh"),
            (ItemKey::Bpm, "128"),
            (ItemKey::InitialKey, "Am"),
        ],
    );

    let wav = music.join("Garage/third.wav");
    folder_for(&wav);
    tone().write_wav(&wav).expect("the WAV writes");
    write_tags(
        &wav,
        &[
            (ItemKey::TrackTitle, "Third Rail"),
            (ItemKey::TrackArtist, "Kofi Bell"),
        ],
    );

    let aiff = music.join("Garage/fourth.aiff");
    Signal::tone_at_loudness(-14.0, 1.0, 44_100)
        .write_aiff(&aiff)
        .expect("the AIFF writes");
    write_tags(
        &aiff,
        &[
            (ItemKey::TrackTitle, "Fourth Wall"),
            (ItemKey::Genre, "UK Garage"),
        ],
    );

    let m4a = music.join("fifth.m4a");
    write_silent_m4a(&m4a, 2.0).expect("the M4A writes");
    write_tags(
        &m4a,
        &[
            (ItemKey::TrackTitle, "Fifth Floor"),
            (ItemKey::TrackArtist, "Nia Okoro"),
            (ItemKey::AlbumTitle, "Lift Music"),
        ],
    );

    let untagged = music.join("House/untagged.wav");
    tone().write_wav(&untagged).expect("the WAV writes");
    fs::write(music.join("House/cover.jpg"), [0xFF, 0xD8, 0xFF]).expect("the image writes");

    for ignored in [
        "House/_replaced/old.wav",
        "Garage/deep/_trash/thrown.wav",
        "_wrong_match/guess.wav",
        "Garage/_converted_originals/before.wav",
        ".hidden/secret.wav",
        "RESONANCE/fixtures-local/copy.wav",
    ] {
        let path = music.join(ignored);
        folder_for(&path);
        tone().write_wav(&path).expect("the WAV writes");
    }
    let program = music.join("RESONANCE/target/debug/resonance");
    folder_for(&program);
    fs::write(&program, b"program").expect("the program file writes");

    Library { music, program }
}

fn scanned(database: &mut Database, library: &Library) -> ScanReport {
    scan(
        database,
        &library.music,
        std::slice::from_ref(&library.program),
    )
    .expect("the scan succeeds")
}

fn database() -> Database {
    Database::open_in_memory().expect("a database opens")
}

fn track<'a>(tracks: &'a [TrackRow], path: &str) -> &'a TrackRow {
    tracks
        .iter()
        .find(|track| track.file.path == path)
        .unwrap_or_else(|| panic!("{path} is not in the library"))
}

fn tag(track: &TrackRow, field: TagField) -> Option<&str> {
    track
        .file
        .tags
        .iter()
        .find(|(name, _)| *name == field)
        .map(|(_, value)| value.as_str())
}

fn files_under(folder: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).expect("the folder lists") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .expect("the file has a modified time");
            found.push((
                path.clone(),
                fs::read(&path).expect("the file reads"),
                modified,
            ));
        }
    }
    found.sort();
    found
}

#[test]
fn a_scan_lists_the_audio_files_and_stays_out_of_ignored_and_own_folders() {
    let library = library("lists");
    let mut database = database();

    let report = scanned(&mut database, &library);

    let paths: Vec<String> = database
        .tracks()
        .expect("the tracks read")
        .into_iter()
        .map(|track| track.file.path)
        .collect();
    assert_eq!(
        paths,
        [
            "Garage/fourth.aiff",
            "Garage/third.wav",
            "House/EP/second.flac",
            "House/first.mp3",
            "House/untagged.wav",
            "fifth.m4a",
        ]
    );
    assert_eq!(
        (report.found, report.added, report.changed, report.unchanged),
        (6, 6, 0, 0)
    );
    assert_eq!(report.unreadable, []);
}

#[test]
fn tags_are_read_from_every_format() {
    let library = library("tags");
    let mut database = database();

    scanned(&mut database, &library);

    let tracks = database.tracks().expect("the tracks read");
    let mp3 = track(&tracks, "House/first.mp3");
    assert_eq!(
        mp3.file.tags,
        [
            (TagField::Title, "First Light".to_owned()),
            (TagField::Artist, "Ada Marsh".to_owned()),
            (TagField::Album, "Early Hours".to_owned()),
            (TagField::AlbumArtist, "Various Artists".to_owned()),
            (TagField::Genre, "House".to_owned()),
            (TagField::Year, "2019".to_owned()),
            (TagField::TrackNumber, "3".to_owned()),
            (TagField::Bpm, "124".to_owned()),
            (TagField::Key, "8A".to_owned()),
            (TagField::Label, "Night Shift".to_owned()),
            (TagField::Comment, "Warm-up".to_owned()),
        ]
    );
    let flac = track(&tracks, "House/EP/second.flac");
    assert_eq!(tag(flac, TagField::Title), Some("Second Wind"));
    assert_eq!(tag(flac, TagField::Bpm), Some("128"));
    assert_eq!(tag(flac, TagField::Key), Some("Am"));
    let wav = track(&tracks, "Garage/third.wav");
    assert_eq!(tag(wav, TagField::Title), Some("Third Rail"));
    assert_eq!(tag(wav, TagField::Artist), Some("Kofi Bell"));
    let aiff = track(&tracks, "Garage/fourth.aiff");
    assert_eq!(tag(aiff, TagField::Title), Some("Fourth Wall"));
    assert_eq!(tag(aiff, TagField::Genre), Some("UK Garage"));
    let m4a = track(&tracks, "fifth.m4a");
    assert_eq!(tag(m4a, TagField::Title), Some("Fifth Floor"));
    assert_eq!(tag(m4a, TagField::Album), Some("Lift Music"));
    assert_eq!(track(&tracks, "House/untagged.wav").file.tags, []);
}

#[test]
fn file_facts_are_read_from_every_format() {
    let library = library("facts");
    let mut database = database();

    scanned(&mut database, &library);

    let tracks = database.tracks().expect("the tracks read");
    let facts = |path: &str| {
        let track = track(&tracks, path);
        let audio = track.file.audio.expect("the file read as audio");
        (track.file.format, audio)
    };

    let (format, mp3) = facts("House/first.mp3");
    assert_eq!(format, AudioFormat::Mp3);
    assert!(
        mp3.duration_ms.abs_diff(2_000) < 60,
        "{} ms",
        mp3.duration_ms
    );
    assert_eq!((mp3.sample_rate, mp3.channels), (Some(44_100), Some(2)));
    assert_eq!(mp3.bitrate_kbps, Some(128));

    let (format, flac) = facts("House/EP/second.flac");
    assert_eq!(format, AudioFormat::Flac);
    assert_eq!(flac.duration_ms, 3_750);
    assert_eq!(
        (flac.sample_rate, flac.channels, flac.bit_depth),
        (Some(44_100), Some(1), Some(16))
    );

    let (format, wav) = facts("Garage/third.wav");
    assert_eq!(format, AudioFormat::Wav);
    assert_eq!(wav.duration_ms, 1_500);
    assert_eq!(
        (wav.sample_rate, wav.channels, wav.bit_depth),
        (Some(48_000), Some(1), Some(16))
    );

    let (format, aiff) = facts("Garage/fourth.aiff");
    assert_eq!(format, AudioFormat::Aiff);
    assert_eq!(aiff.duration_ms, 1_000);
    assert_eq!(
        (aiff.sample_rate, aiff.channels, aiff.bit_depth),
        (Some(44_100), Some(2), Some(16))
    );

    let (format, m4a) = facts("fifth.m4a");
    assert_eq!(format, AudioFormat::M4a);
    assert!(
        m4a.duration_ms.abs_diff(2_000) < 60,
        "{} ms",
        m4a.duration_ms
    );
    assert_eq!((m4a.sample_rate, m4a.channels), (Some(44_100), Some(2)));
}

#[test]
fn a_scan_leaves_every_byte_and_modified_time_as_it_was() {
    let library = library("read_only");
    let before = files_under(&library.music);
    let mut database = database();

    scanned(&mut database, &library);
    scanned(&mut database, &library);

    assert_eq!(files_under(&library.music), before);
}

#[test]
fn a_second_scan_changes_nothing() {
    let library = library("second_scan");
    let mut database = database();
    scanned(&mut database, &library);
    let first = database.tracks().expect("the tracks read");

    let report = scanned(&mut database, &library);

    assert_eq!(
        (report.found, report.added, report.changed, report.unchanged),
        (6, 0, 0, 6)
    );
    assert_eq!(database.tracks().expect("the tracks read"), first);
}

#[test]
fn a_file_with_new_tags_is_read_again_and_keeps_its_track() {
    let library = library("retagged");
    let mut database = database();
    scanned(&mut database, &library);
    let before = database.tracks().expect("the tracks read");
    write_tags(
        &library.music.join("Garage/third.wav"),
        &[(ItemKey::TrackTitle, "Third Rail (Dub)")],
    );

    let report = scanned(&mut database, &library);

    assert_eq!((report.added, report.changed, report.unchanged), (0, 1, 5));
    let after = database.tracks().expect("the tracks read");
    let wav = track(&after, "Garage/third.wav");
    assert_eq!(tag(wav, TagField::Title), Some("Third Rail (Dub)"));
    assert_eq!(tag(wav, TagField::Artist), None);
    assert_eq!(wav.track_id, track(&before, "Garage/third.wav").track_id);
}

#[test]
fn a_file_added_since_the_last_scan_becomes_a_new_track() {
    let library = library("added");
    let mut database = database();
    scanned(&mut database, &library);
    tone()
        .write_wav(&library.music.join("Garage/sixth.wav"))
        .expect("the WAV writes");

    let report = scanned(&mut database, &library);

    assert_eq!(
        (report.found, report.added, report.changed, report.unchanged),
        (7, 1, 0, 6)
    );
    let tracks = database.tracks().expect("the tracks read");
    let ids: std::collections::HashSet<i64> = tracks.iter().map(|track| track.track_id).collect();
    assert_eq!(ids.len(), 7);
}

#[test]
fn a_file_that_is_not_really_audio_is_listed_without_facts_and_reported_once() {
    let library = library("not_audio");
    fs::write(
        library.music.join("House/broken.mp3"),
        b"this is not an MP3",
    )
    .expect("the file writes");
    let mut database = database();

    let report = scanned(&mut database, &library);

    assert_eq!(report.added, 7);
    assert_eq!(report.unreadable.len(), 1);
    assert_eq!(report.unreadable[0].path, "House/broken.mp3");
    let tracks = database.tracks().expect("the tracks read");
    let broken = track(&tracks, "House/broken.mp3");
    assert_eq!(broken.file.audio, None);
    assert_eq!(broken.file.tags, []);

    let again = scanned(&mut database, &library);
    assert_eq!((again.unchanged, again.unreadable.len()), (7, 0));
}

#[test]
fn a_folder_whose_name_ends_in_a_space_is_scanned_like_any_other() {
    let library = library("trailing_space");
    let music = fs::canonicalize(&library.music).expect("the music folder resolves");
    let awkward = music.join("House/Solid Kick /seventh.wav");
    folder_for(&awkward);
    tone().write_wav(&awkward).expect("the WAV writes");
    let mut database = database();

    let report = scanned(&mut database, &library);

    assert_eq!(report.unreadable, []);
    let tracks = database.tracks().expect("the tracks read");
    assert!(
        tracks
            .iter()
            .any(|track| track.file.path == "House/Solid Kick /seventh.wav")
    );
}

#[test]
fn an_m4a_with_an_id3_tag_stuck_in_front_of_it_is_read_as_the_m4a_it_is() {
    let library = library("id3_before_mp4");
    let path = library.music.join("fifth.m4a");
    let m4a = fs::read(&path).expect("the M4A reads");
    // An ID3v2.3 header that declares 300 bytes, all of them padding.
    let mut prefixed = b"ID3    ,".to_vec();
    prefixed.extend([0; 300]);
    prefixed.extend(m4a);
    fs::write(&path, prefixed).expect("the prefixed file writes");
    let mut database = database();

    let report = scanned(&mut database, &library);

    assert_eq!(report.unreadable, []);
    let tracks = database.tracks().expect("the tracks read");
    let m4a = track(&tracks, "fifth.m4a");
    let audio = m4a.file.audio.expect("the file read as audio");
    assert!(audio.duration_ms.abs_diff(2_000) < 60);
    assert_eq!((audio.sample_rate, audio.channels), (Some(44_100), Some(2)));
    assert_eq!(tag(m4a, TagField::Title), Some("Fifth Floor"));
}

#[cfg(windows)]
#[test]
fn a_folder_marked_hidden_in_windows_is_not_scanned() {
    let library = library("hidden_attribute");
    let hidden = library.music.join("Garage/Stash");
    let inside = hidden.join("eighth.wav");
    folder_for(&inside);
    tone().write_wav(&inside).expect("the WAV writes");
    let marked = std::process::Command::new("attrib")
        .arg("+h")
        .arg(&hidden)
        .status()
        .expect("attrib runs");
    assert!(marked.success());
    let mut database = database();

    let report = scanned(&mut database, &library);

    assert_eq!(report.found, 6);
}

#[test]
fn the_library_is_kept_in_a_file_between_runs() {
    let library = library("kept");
    let file = library
        .music
        .parent()
        .expect("the music folder has a parent")
        .join("data")
        .join("library.db");
    {
        let mut database = Database::open(&file).expect("the database file opens");
        scanned(&mut database, &library);
    }

    let mut reopened = Database::open(&file).expect("the database file opens again");
    let report = scanned(&mut reopened, &library);

    assert_eq!((report.added, report.unchanged), (0, 6));
    assert_eq!(reopened.tracks().expect("the tracks read").len(), 6);
}

#[test]
fn a_folder_that_does_not_exist_is_reported_by_its_path() {
    let library = library("missing");
    let mut database = database();

    let error = scan(&mut database, &library.music.join("Nowhere"), &[])
        .expect_err("a missing folder cannot be scanned");

    assert!(error.to_string().contains("Nowhere"));
}

/// The real tracks stay on the machine that copied them, so this checks them where they exist
/// and passes without looking anywhere else.
#[test]
fn the_local_set_of_real_tracks_scans_cleanly_where_this_machine_has_one() {
    let local_set = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures-local");
    if !local_set.join("manifest.json").is_file() {
        return;
    }
    let before = files_under(&local_set);
    let mut database = database();

    let first = scan(&mut database, &local_set, &[]).expect("the local set scans");
    let second = scan(&mut database, &local_set, &[]).expect("the local set scans again");

    assert_eq!(first.found, 50);
    assert_eq!(first.added, 50);
    assert_eq!(first.unreadable, []);
    assert_eq!((second.added, second.changed, second.unchanged), (0, 0, 50));
    let tracks = database.tracks().expect("the tracks read");
    assert!(tracks.iter().all(|track| track.file.audio.is_some()));
    assert_eq!(files_under(&local_set), before);
}

#[test]
fn scan_stores_embedded_isrc_and_marks_isrc_read() {
    let library = library("isrc-scan");
    write_tags(
        &library.music.join("House/first.mp3"),
        &[
            (ItemKey::TrackTitle, "First Light"),
            (ItemKey::TrackArtist, "Ada Marsh"),
            (ItemKey::Isrc, "USRC19900001"),
        ],
    );
    let mut database = database();
    let report = scanned(&mut database, &library);
    assert_eq!(report.added, 6);

    let source_id = database
        .source(&library.music.to_string_lossy())
        .expect("source resolves");
    assert!(
        database
            .paths_without_isrc_read(source_id)
            .expect("unread query succeeds")
            .is_empty()
    );
    let inputs = database
        .identification_inputs(resonance_db::IdentificationPass::Local, 0, 20)
        .expect("inputs read");
    let first = inputs
        .iter()
        .find(|input| input.path == "House/first.mp3")
        .expect("mp3 input present");
    assert_eq!(first.file_isrc.as_deref(), Some("USRC19900001"));
}
