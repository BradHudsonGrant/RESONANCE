use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::{AudioFile, TaggedFileExt};
use lofty::tag::Tag;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_fixtures::silence::{write_silent_m4a, write_silent_mp3};
use resonance_library::file_tags;
use resonance_library::identity::compute_identity;
use resonance_library::scan::scan;
use resonance_library::write_tags::{apply_tag_writes, preview_tag_writes, undo_operation};

const OLD_COVER: [u8; 12] = [0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F', 0, 1];
const OTHER_COVER: [u8; 12] = [0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F', 0, 2];
/// A one-pixel PNG, so the FLAC picture block can read its size.
const NEW_COVER: [u8; 67] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

struct Folders {
    music: PathBuf,
    data: PathBuf,
}

fn folders(name: &str) -> Folders {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("artwork_{name}"));
    let _ = fs::remove_dir_all(&root);
    let folders = Folders {
        music: root.join("music"),
        data: root.join("data"),
    };
    fs::create_dir_all(&folders.music).expect("music folder");
    folders
}

fn hash_of(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn add_cover(path: &Path, bytes: &[u8], kind: PictureType) {
    let mut tagged = lofty::read_from_path(path).expect("file reads");
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(tagged.primary_tag_type()));
    }
    let tag = tagged.primary_tag_mut().expect("primary tag");
    tag.push_picture(
        Picture::unchecked(bytes.to_vec())
            .pic_type(kind)
            .mime_type(MimeType::Jpeg)
            .build(),
    );
    tagged
        .save_to_path(path, WriteOptions::default())
        .expect("cover saved");
}

fn front_cover(path: &Path) -> Option<Vec<u8>> {
    file_tags::read(path).expect("file reads").front_cover
}

fn audio_hash(path: &Path) -> resonance_core::identity::AudioHash {
    compute_identity(path)
        .expect("identity")
        .audio
        .expect("audio decodes")
}

fn shown_artwork(database: &Database) -> Vec<Option<String>> {
    database
        .tracks()
        .expect("tracks")
        .into_iter()
        .map(|track| track.artwork)
        .collect()
}

fn stored_files(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("artwork folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn an_album_scanned_keeps_one_picture() {
    let folders = folders("album");
    fs::create_dir_all(folders.music.join("Album")).expect("album folder");
    for name in ["Album/01.mp3", "Album/02.mp3", "Other.mp3", "Plain.mp3"] {
        write_silent_mp3(&folders.music.join(name), 1.0).expect("mp3 written");
    }
    add_cover(
        &folders.music.join("Album/01.mp3"),
        &OLD_COVER,
        PictureType::CoverFront,
    );
    // A cover typed as something else is still the file's cover when it is the only picture.
    add_cover(
        &folders.music.join("Album/02.mp3"),
        &OLD_COVER,
        PictureType::Other,
    );
    add_cover(
        &folders.music.join("Other.mp3"),
        &OTHER_COVER,
        PictureType::CoverFront,
    );

    let mut database = Database::open(&folders.data.join("library.db")).expect("database");
    scan(&mut database, &folders.music, &[]).expect("scan");

    let album = Some(hash_of(&OLD_COVER));
    assert_eq!(
        shown_artwork(&database),
        [album.clone(), album, Some(hash_of(&OTHER_COVER)), None]
    );
    let mut expected = vec![
        format!("{}.jpg", hash_of(&OLD_COVER)),
        format!("{}.jpg", hash_of(&OTHER_COVER)),
    ];
    expected.sort();
    assert_eq!(stored_files(&folders.data.join("artwork")), expected);
}

#[test]
fn files_scanned_before_covers_were_read_get_them_on_the_next_scan() {
    let folders = folders("backfill");
    let path = folders.music.join("One.mp3");
    write_silent_mp3(&path, 1.0).expect("mp3 written");
    add_cover(&path, &OLD_COVER, PictureType::CoverFront);
    let mut database = Database::open_in_memory().expect("database");
    scan(&mut database, &folders.music, &[]).expect("first scan");
    assert_eq!(shown_artwork(&database), [None]);

    database.set_artwork_folder(folders.data.join("artwork"));
    let report = scan(&mut database, &folders.music, &[]).expect("second scan");

    assert_eq!(report.unchanged, 1, "the file itself is not read again");
    assert_eq!(shown_artwork(&database), [Some(hash_of(&OLD_COVER))]);
}

fn embeds_and_undoes(file_name: &str, make: impl Fn(&Path), with_cover: bool) {
    let folders = folders(&format!("embed_{file_name}_{with_cover}"));
    let path = folders.music.join(file_name);
    make(&path);
    if with_cover {
        add_cover(&path, &OLD_COVER, PictureType::CoverFront);
    }
    let before = audio_hash(&path);
    let old = with_cover.then(|| hash_of(&OLD_COVER));

    let mut database = Database::open(&folders.data.join("library.db")).expect("database");
    scan(&mut database, &folders.music, &[]).expect("scan");
    assert_eq!(shown_artwork(&database), [old], "{file_name}");
    let track_id = database.tracks().expect("tracks")[0].track_id;
    let store = database.artwork_store().expect("store");
    let picked = store.put(&NEW_COVER).expect("picture stored");
    assert!(
        database
            .set_user_artwork(track_id, &picked)
            .expect("picked")
    );
    let database = Mutex::new(database);

    let report = apply_tag_writes(&database, &[]).expect("apply");
    assert_eq!(report.written, 1, "{file_name}: {:?}", report.failed);
    assert_eq!(
        front_cover(&path).as_deref(),
        Some(NEW_COVER.as_slice()),
        "{file_name}"
    );
    assert_eq!(
        audio_hash(&path),
        before,
        "{file_name}: the audio is untouched"
    );
    {
        let library = database.lock().expect("lock");
        assert_eq!(shown_artwork(&library), [Some(picked.hash.clone())]);
        assert!(
            preview_tag_writes(&library, &[])
                .expect("preview")
                .is_empty(),
            "{file_name}: the picture is in the file"
        );
    }

    undo_operation(&database, report.operation_id.expect("operation")).expect("undo");

    assert_eq!(
        front_cover(&path).as_deref(),
        with_cover.then_some(OLD_COVER.as_slice()),
        "{file_name}: undo puts the old cover back"
    );
    assert_eq!(audio_hash(&path), before, "{file_name}");
    let library = database.lock().expect("lock");
    assert_eq!(
        shown_artwork(&library),
        [Some(picked.hash)],
        "{file_name}: the user's choice stays"
    );
    assert_eq!(
        preview_tag_writes(&library, &[]).expect("preview").len(),
        1,
        "{file_name}: and is unwritten again"
    );
}

fn tone() -> Signal {
    Signal::tone(440.0, 2.0, -6.0, 44_100)
}

#[test]
fn embeds_a_cover_in_mp3() {
    let make = |path: &Path| write_silent_mp3(path, 2.0).expect("mp3 written");
    embeds_and_undoes("track.mp3", make, true);
    embeds_and_undoes("track.mp3", make, false);
}

#[test]
fn embeds_a_cover_in_wav() {
    let make = |path: &Path| tone().write_wav(path).expect("wav written");
    embeds_and_undoes("track.wav", make, true);
    embeds_and_undoes("track.wav", make, false);
}

#[test]
fn embeds_a_cover_in_aiff() {
    let make = |path: &Path| tone().write_aiff(path).expect("aiff written");
    embeds_and_undoes("track.aiff", make, true);
    embeds_and_undoes("track.aiff", make, false);
}

#[test]
fn embeds_a_cover_in_flac() {
    let make = |path: &Path| tone().write_flac(path).expect("flac written");
    embeds_and_undoes("track.flac", make, true);
    embeds_and_undoes("track.flac", make, false);
}

#[test]
fn embeds_a_cover_in_m4a() {
    let make = |path: &Path| write_silent_m4a(path, 2.0).expect("m4a written");
    embeds_and_undoes("track.m4a", make, true);
    embeds_and_undoes("track.m4a", make, false);
}
