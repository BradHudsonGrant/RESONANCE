use std::fs;
use std::path::Path;

use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_db::{Database, FileStamp, SCHEMA_VERSION};

fn file(path: &str, title: &str) -> ScannedFile {
    ScannedFile {
        path: path.to_owned(),
        size_bytes: 4_200_000,
        modified_ns: 1_700_000_000_123_456_789,
        format: AudioFormat::Flac,
        audio: Some(AudioFacts {
            duration_ms: 301_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: Some(16),
            bitrate_kbps: Some(900),
        }),
        tags: vec![
            (TagField::Title, title.to_owned()),
            (TagField::Artist, "Ada Marsh".to_owned()),
        ],
    }
}

#[test]
fn what_a_scan_stored_reads_back_the_same() {
    let mut database = Database::open_in_memory().expect("a database opens");
    let source = database.source(r"D:\Music").expect("the source is added");
    let unreadable = ScannedFile {
        audio: None,
        tags: Vec::new(),
        format: AudioFormat::Mp3,
        ..file("House/broken.mp3", "")
    };
    let stored = [file("House/one.flac", "One"), unreadable];

    database
        .record_scan(source, &stored)
        .expect("the scan is stored");

    let tracks = database.tracks().expect("the tracks read");
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].file, stored[1]);
    assert_eq!(tracks[1].file, stored[0]);
    assert!(
        tracks
            .iter()
            .all(|track| track.source_folder == r"D:\Music")
    );
    assert_ne!(tracks[0].track_id, tracks[1].track_id);
}

#[test]
fn a_path_stored_again_keeps_its_track_and_takes_the_new_facts_and_tags() {
    let mut database = Database::open_in_memory().expect("a database opens");
    let source = database.source("music").expect("the source is added");
    database
        .record_scan(source, &[file("one.flac", "One")])
        .expect("the scan is stored");
    let before = database.tracks().expect("the tracks read");
    let retagged = ScannedFile {
        size_bytes: 4_200_100,
        tags: vec![(TagField::Title, "One (Edit)".to_owned())],
        ..file("one.flac", "")
    };

    database
        .record_scan(source, std::slice::from_ref(&retagged))
        .expect("the scan is stored");

    let after = database.tracks().expect("the tracks read");
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].track_id, before[0].track_id);
    assert_eq!(after[0].file, retagged);
}

#[test]
fn a_folder_is_one_source_however_often_it_is_asked_for() {
    let mut database = Database::open_in_memory().expect("a database opens");

    let first = database.source("music").expect("the source is added");
    let again = database.source("music").expect("the source is found");
    let other = database.source("more music").expect("the source is added");

    assert_eq!(first, again);
    assert_ne!(first, other);
}

#[test]
fn stamps_are_listed_for_one_source_only() {
    let mut database = Database::open_in_memory().expect("a database opens");
    let music = database.source("music").expect("the source is added");
    let other = database.source("more music").expect("the source is added");
    database
        .record_scan(music, &[file("one.flac", "One")])
        .expect("the scan is stored");
    database
        .record_scan(other, &[file("two.flac", "Two")])
        .expect("the scan is stored");

    let stamps = database.file_stamps(music).expect("the stamps read");

    assert_eq!(stamps.len(), 1);
    assert_eq!(
        stamps["one.flac"],
        FileStamp {
            size_bytes: 4_200_000,
            modified_ns: 1_700_000_000_123_456_789,
        }
    );
}

#[test]
fn a_database_file_is_made_with_its_folder_and_keeps_its_rows_between_openings() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("library_rows");
    let _ = fs::remove_dir_all(&folder);
    let path = folder.join("data").join("library.db");
    {
        let mut database = Database::open(&path).expect("the database file is made");
        let source = database.source("music").expect("the source is added");
        database
            .record_scan(source, &[file("one.flac", "One")])
            .expect("the scan is stored");
    }

    let reopened = Database::open(&path).expect("the database file opens again");

    assert_eq!(
        reopened.schema_version().expect("the version reads"),
        SCHEMA_VERSION
    );
    assert_eq!(reopened.tracks().expect("the tracks read").len(), 1);
}

#[test]
fn a_page_holds_the_tracks_of_that_part_of_the_library_in_order() {
    let mut database = Database::open_in_memory().expect("a database opens");
    let later = database.source("b-music").expect("the source is added");
    let earlier = database.source("a-music").expect("the source is added");
    database
        .record_scan(later, &[file("y.flac", "Y"), file("x.flac", "X")])
        .expect("the scan is stored");
    database
        .record_scan(earlier, &[file("z.flac", "Z"), file("a.flac", "A")])
        .expect("the scan is stored");

    let all = database.tracks().expect("the tracks read");
    let paths: Vec<_> = all
        .iter()
        .map(|track| format!("{}/{}", track.source_folder, track.file.path))
        .collect();
    assert_eq!(
        paths,
        [
            "a-music/a.flac",
            "a-music/z.flac",
            "b-music/x.flac",
            "b-music/y.flac"
        ]
    );

    assert_eq!(database.track_count().expect("the count reads"), 4);
    assert_eq!(
        database.tracks_page(1, 2).expect("the page reads"),
        all[1..3]
    );
    assert_eq!(
        database.tracks_page(3, 10).expect("the page reads"),
        all[3..]
    );
    assert!(
        database
            .tracks_page(4, 10)
            .expect("the page reads")
            .is_empty()
    );
}

#[test]
fn toggling_subfolders_and_ignored_names_hides_and_restores_assets_without_losing_track_ids() {
    let mut database = Database::open_in_memory().expect("a database opens");
    let source = database.source("music").expect("the source is added");
    database
        .record_scan(
            source,
            &[
                file("root.flac", "Root"),
                file("House/deep.flac", "Deep"),
                file("Samples/kick.flac", "Kick"),
            ],
        )
        .expect("the scan is stored");
    let initial = database.tracks().expect("tracks read");
    let deep_id = initial
        .iter()
        .find(|row| row.file.path == "House/deep.flac")
        .expect("deep exists")
        .track_id;

    let updated = database
        .update_source(source, true, false, &["Samples"])
        .expect("source updates")
        .expect("source exists");
    assert!(!updated.watch);
    assert_eq!(updated.excluded_subfolders, ["Samples"]);
    assert_eq!(updated.track_count, 2);
    assert_eq!(database.track_count().expect("count reads"), 2);

    database
        .update_source(source, false, false, &["Samples"])
        .expect("source updates");
    assert_eq!(database.track_count().expect("only root remains"), 1);

    database
        .update_source(source, true, true, &[] as &[&str])
        .expect("source restores");
    let restored = database.tracks().expect("tracks read");
    assert_eq!(restored.len(), 3);
    assert_eq!(
        restored
            .iter()
            .find(|row| row.file.path == "House/deep.flac")
            .expect("deep restored")
            .track_id,
        deep_id
    );

    database
        .set_ignored_folders(&["House"])
        .expect("ignored list updates");
    let after_ignore = database.tracks().expect("tracks read");
    assert_eq!(after_ignore.len(), 2);
    assert!(
        after_ignore
            .iter()
            .all(|row| !row.file.path.starts_with("House/"))
    );

    assert!(database.remove_source(source).expect("source removes"));
    assert_eq!(database.track_count().expect("empty after remove"), 0);
    assert!(database.sources().expect("sources read").is_empty());
}
