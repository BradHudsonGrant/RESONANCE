#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::PathBuf;

use std::time::Duration;

use resonance_core::artwork::{ArtworkRefusal, ImageKind};
use resonance_core::library::{AudioFormat, ScannedFile};
use resonance_db::{ArtworkError, ArtworkStore, CachedArtworkSource, Database, StoredArtwork};

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("artwork-{name}"));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

fn jpeg(seed: u8) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0];
    bytes.extend(std::iter::repeat_n(seed, 64));
    bytes
}

fn scanned(path: &str) -> ScannedFile {
    ScannedFile {
        path: path.to_owned(),
        size_bytes: 1_000,
        modified_ns: 1,
        format: AudioFormat::Mp3,
        audio: None,
        tags: Vec::new(),
    }
}

fn library_of(paths: &[&str]) -> (Database, Vec<i64>) {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("C:/Music").unwrap();
    let files: Vec<ScannedFile> = paths.iter().map(|path| scanned(path)).collect();
    database.record_scan(source, &files).unwrap();
    let ids = database
        .tracks()
        .unwrap()
        .iter()
        .map(|track| track.track_id)
        .collect();
    (database, ids)
}

#[test]
fn the_same_picture_is_stored_once() {
    let folder = test_folder("dedup");
    let store = ArtworkStore::new(&folder);

    let first = store.put(&jpeg(1)).unwrap();
    let again = store.put(&jpeg(1)).unwrap();
    let other = store.put(&jpeg(2)).unwrap();

    assert_eq!(first, again);
    assert_ne!(first.hash, other.hash);
    assert_eq!(first.kind, ImageKind::Jpeg);
    assert_eq!(first.hash, blake3::hash(&jpeg(1)).to_hex().as_str());
    let mut names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut expected = vec![format!("{}.jpg", first.hash), format!("{}.jpg", other.hash)];
    expected.sort();
    assert_eq!(names, expected);
    assert_eq!(store.read(&first).unwrap(), jpeg(1));
    assert_eq!(
        store.find(&first.hash),
        Some((folder.join(format!("{}.jpg", first.hash)), ImageKind::Jpeg))
    );
}

#[test]
fn a_name_that_is_not_a_hash_finds_nothing() {
    let folder = test_folder("names");
    let store = ArtworkStore::new(folder.join("artwork"));
    fs::write(folder.join("library.db"), b"not a picture").unwrap();

    assert_eq!(store.find("../library"), None);
    assert_eq!(store.find(&"0".repeat(64)), None);
}

#[test]
fn something_that_is_not_a_picture_is_refused() {
    let store = ArtworkStore::new(test_folder("refused"));

    assert!(matches!(
        store.put(b"GIF89a"),
        Err(ArtworkError::Refused(ArtworkRefusal::NotJpegOrPng))
    ));
}

#[test]
fn a_file_opened_from_disk_keeps_pictures_beside_it() {
    let folder = test_folder("beside");
    let database = Database::open(&folder.join("library.db")).unwrap();

    assert_eq!(
        database.artwork_store(),
        Some(ArtworkStore::new(folder.join("artwork")))
    );
}

#[test]
fn an_album_of_tracks_shares_one_picture() {
    let folder = test_folder("album");
    let store = ArtworkStore::new(&folder);
    let cover = store.put(&jpeg(7)).unwrap();
    let (mut database, ids) = library_of(&["Album/01.mp3", "Album/02.mp3", "Single.mp3"]);
    let source = database.source("C:/Music").unwrap();
    assert_eq!(
        database.paths_without_artwork_read(source).unwrap().len(),
        3
    );

    database
        .record_scanned_artwork(
            source,
            &[
                ("Album/01.mp3".to_owned(), Some(cover.clone())),
                ("Album/02.mp3".to_owned(), Some(cover.clone())),
                ("Single.mp3".to_owned(), None),
            ],
        )
        .unwrap();

    let shown: Vec<Option<String>> = database
        .tracks()
        .unwrap()
        .into_iter()
        .map(|track| track.artwork)
        .collect();
    assert_eq!(
        shown,
        [Some(cover.hash.clone()), Some(cover.hash.clone()), None]
    );
    assert!(
        database
            .paths_without_artwork_read(source)
            .unwrap()
            .is_empty()
    );
    assert_eq!(database.track_artwork(ids[0]).unwrap(), Some(cover.clone()));
    assert_eq!(database.track_artwork(ids[2]).unwrap(), None);
}

#[test]
fn the_user_picture_shows_over_the_file_and_waits_to_be_written() {
    let store = ArtworkStore::new(test_folder("layers"));
    let in_file = store.put(&jpeg(1)).unwrap();
    let picked = store.put(&jpeg(2)).unwrap();
    let (mut database, ids) = library_of(&["One.mp3"]);
    let source = database.source("C:/Music").unwrap();
    database
        .record_scanned_artwork(source, &[("One.mp3".to_owned(), Some(in_file.clone()))])
        .unwrap();

    assert!(database.set_user_artwork(ids[0], &picked).unwrap());

    assert_eq!(
        database.tracks().unwrap()[0].artwork,
        Some(picked.hash.clone())
    );
    assert_eq!(
        database.track_artwork(ids[0]).unwrap(),
        Some(picked.clone())
    );
    assert_eq!(database.tracks_with_unwritten_tags(&[]).unwrap(), ids);
    let target = database.tag_write_target(ids[0]).unwrap().unwrap();
    assert_eq!(target.artwork, Some(picked.clone()));
    assert!(target.unwritten.is_empty());

    database
        .record_rewritten_file(
            target.asset_id,
            target.stamp,
            &resonance_core::identity::ContentHash::from_bytes([0; 32]),
            &[],
            Some(&picked),
        )
        .unwrap();

    assert!(database.tracks_with_unwritten_tags(&[]).unwrap().is_empty());
    assert_eq!(
        database.tag_write_target(ids[0]).unwrap().unwrap().artwork,
        None
    );
}

#[test]
fn a_proposal_is_fetched_once_and_waits_for_a_decision() {
    let store = ArtworkStore::new(test_folder("proposal"));
    let release = store.put(&jpeg(3)).unwrap();
    let (mut database, ids) = library_of(&["A.mp3", "B.mp3"]);
    let url = "https://coverartarchive.org/release/x/front-500";
    assert_eq!(
        database.cached_artwork_source(url, Duration::ZERO).unwrap(),
        None
    );

    for track_id in &ids {
        database
            .store_proposed_artwork(*track_id, &release, "musicbrainz", url)
            .unwrap();
    }

    assert_eq!(
        database.cached_artwork_source(url, Duration::ZERO).unwrap(),
        Some(CachedArtworkSource::Stored(release.clone()))
    );
    let proposed = database.proposed_artwork(ids[0]).unwrap().unwrap();
    assert!(proposed.pending);
    assert_eq!(proposed.provider, "musicbrainz");
    assert_eq!(database.tracks().unwrap()[0].artwork, None);

    assert!(database.accept_proposed_artwork(ids[0]).unwrap());
    assert!(database.skip_proposed_artwork(ids[1]).unwrap());

    assert_eq!(
        database.tracks().unwrap()[0].artwork,
        Some(release.hash.clone())
    );
    assert_eq!(database.tracks().unwrap()[1].artwork, None);
    assert!(!database.proposed_artwork(ids[1]).unwrap().unwrap().pending);
    assert!(!database.accept_proposed_artwork(ids[1]).unwrap());

    // The same picture again keeps the decision, a different one asks again.
    database
        .store_proposed_artwork(ids[1], &release, "musicbrainz", url)
        .unwrap();
    assert!(!database.proposed_artwork(ids[1]).unwrap().unwrap().pending);
    let other: StoredArtwork = store.put(&jpeg(4)).unwrap();
    database
        .store_proposed_artwork(ids[1], &other, "discogs", "https://i.discogs.com/x.jpg")
        .unwrap();
    assert!(database.proposed_artwork(ids[1]).unwrap().unwrap().pending);
    assert_eq!(
        database
            .cached_artwork_source("https://i.discogs.com/x.jpg", Duration::from_secs(60))
            .unwrap(),
        Some(CachedArtworkSource::Stored(other))
    );
}

#[test]
fn fetched_artwork_cache_stores_hits_and_negative_entries_without_proposals() {
    let store = ArtworkStore::new(test_folder("source-cache"));
    let cover = store.put(&jpeg(8)).unwrap();
    let (mut database, _) = library_of(&["A.mp3"]);
    let hit_url = "https://i.discogs.com/cached-hit.jpg";
    let miss_url = "https://coverartarchive.org/release/missing/front-500";

    database
        .record_fetched_artwork(hit_url, Some(&cover))
        .unwrap();
    assert_eq!(
        database
            .cached_artwork_source(hit_url, Duration::from_secs(60))
            .unwrap(),
        Some(CachedArtworkSource::Stored(cover))
    );

    database.record_fetched_artwork(miss_url, None).unwrap();
    assert_eq!(
        database
            .cached_artwork_source(miss_url, Duration::from_secs(60))
            .unwrap(),
        Some(CachedArtworkSource::Unavailable)
    );
    assert_eq!(
        database
            .cached_artwork_source(miss_url, Duration::ZERO)
            .unwrap(),
        None
    );
}
