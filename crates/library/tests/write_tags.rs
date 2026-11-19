use std::fs::{self, File, OpenOptions};
use std::io::Seek;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use lofty::config::{ParseOptions, WriteOptions};
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, Id3v2Tag, PrivateFrame};
use lofty::iff::aiff::AiffFile;
use lofty::iff::wav::WavFile;
use lofty::mp4::{Atom, AtomData, AtomIdent, Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
use lofty::prelude::{Accessor, AudioFile};
use resonance_core::journal::OperationState;
use resonance_core::library::TagField;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_fixtures::silence::{write_silent_m4a, write_silent_mp3};
use resonance_library::identity::compute_identity;
use resonance_library::scan::scan;
use resonance_library::write_tags::{
    FileProblem, TagWriteError, apply_tag_writes, preview_tag_writes, undo_operation,
};

const PRIVATE_OWNER: &str = "com.example.dj";
const PRIVATE_DATA: [u8; 6] = [0, 1, 2, 3, 254, 255];
const CUSTOM_NAME: &str = "DJ NOTES";
const CUSTOM_VALUE: &str = "keep this";
const COVER: [u8; 8] = [0xff, 0xd8, 0xff, 0xd9, 1, 2, 3, 4];

fn cover() -> Picture {
    Picture::unchecked(COVER.to_vec())
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::Jpeg)
        .build()
}

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("music")).expect("test folder created");
    folder.join("music")
}

fn open_rw(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .expect("the file opens for writing")
}

fn seeded_id3v2() -> Id3v2Tag {
    let mut tag = Id3v2Tag::new();
    tag.set_title("Old Title".to_owned());
    tag.set_artist("Old Artist".to_owned());
    tag.set_comment("Old comment".to_owned());
    tag.insert(Frame::Private(PrivateFrame::new(
        PRIVATE_OWNER,
        PRIVATE_DATA.to_vec(),
    )));
    tag.insert_user_text(CUSTOM_NAME.to_owned(), CUSTOM_VALUE.to_owned());
    tag.insert_picture(cover());
    tag
}

fn id3v2_keeps_unknown_frames(tag: Option<&Id3v2Tag>) {
    let tag = tag.expect("the file still has an ID3v2 tag");
    assert_eq!(tag.get_user_text(CUSTOM_NAME), Some(CUSTOM_VALUE));
    let private = tag
        .into_iter()
        .find_map(|frame| match frame {
            Frame::Private(private) if private.owner == PRIVATE_OWNER => {
                Some(private.private_data.to_vec())
            }
            _ => None,
        })
        .expect("the private frame survives");
    assert_eq!(private, PRIVATE_DATA);
    assert!(
        tag.into_iter().any(
            |frame| matches!(frame, Frame::Picture(picture) if picture.picture.data() == COVER)
        ),
        "the picture survives"
    );
}

fn custom_atom() -> AtomIdent<'static> {
    AtomIdent::Freeform {
        mean: "com.example".into(),
        name: CUSTOM_NAME.into(),
    }
}

/// Writes the starting tags, an ID3v2.4 tag for the formats that take ID3, with frames the
/// library does not read.
fn seed(path: &Path) {
    let mut file = open_rw(path);
    let parse = ParseOptions::new();
    let v24 = WriteOptions::new().use_id3v23(false);
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("mp3") => {
            let mut audio = MpegFile::read_from(&mut file, parse).expect("mp3 reads");
            audio.set_id3v2(seeded_id3v2());
            file.rewind().expect("rewinds");
            audio.save_to(&mut file, v24).expect("mp3 seeds");
        }
        Some("wav") => {
            let mut audio = WavFile::read_from(&mut file, parse).expect("wav reads");
            audio.set_id3v2(seeded_id3v2());
            file.rewind().expect("rewinds");
            audio.save_to(&mut file, v24).expect("wav seeds");
        }
        Some("aiff") => {
            let mut audio = AiffFile::read_from(&mut file, parse).expect("aiff reads");
            audio.set_id3v2(seeded_id3v2());
            file.rewind().expect("rewinds");
            audio.save_to(&mut file, v24).expect("aiff seeds");
        }
        Some("flac") => {
            let mut audio = FlacFile::read_from(&mut file, parse).expect("flac reads");
            let mut tag = VorbisComments::default();
            tag.insert("TITLE".to_owned(), "Old Title".to_owned());
            tag.insert("ARTIST".to_owned(), "Old Artist".to_owned());
            tag.insert("COMMENT".to_owned(), "Old comment".to_owned());
            tag.insert(CUSTOM_NAME.to_owned(), CUSTOM_VALUE.to_owned());
            audio.set_vorbis_comments(tag);
            audio
                .insert_picture(cover(), Some(PictureInformation::default()))
                .expect("picture added");
            file.rewind().expect("rewinds");
            audio
                .save_to(&mut file, WriteOptions::new())
                .expect("flac seeds");
        }
        Some("m4a") => {
            let mut audio = Mp4File::read_from(&mut file, parse).expect("m4a reads");
            let mut tag = Ilst::default();
            tag.set_title("Old Title".to_owned());
            tag.set_artist("Old Artist".to_owned());
            tag.set_comment("Old comment".to_owned());
            tag.insert(Atom::new(
                custom_atom(),
                AtomData::UTF8(CUSTOM_VALUE.to_owned()),
            ));
            tag.insert_picture(cover());
            audio.set_ilst(tag);
            file.rewind().expect("rewinds");
            audio
                .save_to(&mut file, WriteOptions::new())
                .expect("m4a seeds");
        }
        other => panic!("no seed for {other:?}"),
    }
}

fn keeps_unknown_items(path: &Path) {
    let mut file = File::open(path).expect("opens");
    let parse = ParseOptions::new();
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("mp3") => id3v2_keeps_unknown_frames(
            MpegFile::read_from(&mut file, parse)
                .expect("reads")
                .id3v2(),
        ),
        Some("wav") => {
            id3v2_keeps_unknown_frames(WavFile::read_from(&mut file, parse).expect("reads").id3v2())
        }
        Some("aiff") => id3v2_keeps_unknown_frames(
            AiffFile::read_from(&mut file, parse)
                .expect("reads")
                .id3v2(),
        ),
        Some("flac") => {
            let audio = FlacFile::read_from(&mut file, parse).expect("reads");
            let tag = audio.vorbis_comments().expect("still has comments");
            assert_eq!(tag.get(CUSTOM_NAME), Some(CUSTOM_VALUE));
            assert!(
                audio
                    .pictures()
                    .iter()
                    .any(|(picture, _)| picture.data() == COVER)
            );
        }
        Some("m4a") => {
            let audio = Mp4File::read_from(&mut file, parse).expect("reads");
            assert!(
                audio
                    .ilst()
                    .and_then(Ilst::pictures)
                    .is_some_and(|mut pictures| pictures.any(|picture| picture.data() == COVER))
            );
            let atom = audio
                .ilst()
                .and_then(|tag| tag.get(&custom_atom()))
                .expect("the custom atom survives");
            assert!(
                atom.data()
                    .any(|data| matches!(data, AtomData::UTF8(text) if text == CUSTOM_VALUE))
            );
        }
        other => panic!("no check for {other:?}"),
    }
}

/// The major version byte of the ID3v2 header in the file, wherever the container keeps it.
fn id3v2_major_version(path: &Path) -> Option<u8> {
    let bytes = fs::read(path).expect("reads");
    bytes
        .windows(4)
        .find(|window| window[..3] == *b"ID3" && matches!(window[3], 2..=4))
        .map(|window| window[3])
}

fn audio_hash(path: &Path) -> resonance_core::identity::AudioHash {
    compute_identity(path)
        .expect("identity")
        .audio
        .expect("the audio decodes")
}

fn read_tags(path: &Path) -> Vec<(TagField, String)> {
    resonance_library::file_tags::read(path)
        .expect("tags read")
        .tags
}

fn library_of(folder: &Path) -> (Database, i64) {
    let mut database = Database::open_in_memory().expect("database");
    scan(&mut database, folder, &[]).expect("scan");
    let track_id = database.tracks().expect("tracks")[0].track_id;
    (database, track_id)
}

fn tag(tags: &[(TagField, String)], field: TagField) -> Option<&str> {
    tags.iter()
        .find(|(held, _)| *held == field)
        .map(|(_, value)| value.as_str())
}

fn writes_and_undoes(file_name: &str, make: impl Fn(&Path)) {
    let folder = test_folder(&format!("write_tags_{file_name}"));
    let path = folder.join(file_name);
    make(&path);
    seed(&path);
    let seeded_tags = read_tags(&path);
    let seeded_version = id3v2_major_version(&path);
    let before = audio_hash(&path);

    let (mut database, track_id) = library_of(&folder);
    database
        .set_user_tags(
            &[track_id],
            &[
                (TagField::Title, Some("New Title".to_owned())),
                (TagField::Bpm, Some("124".to_owned())),
                (TagField::Comment, Some(String::new())),
                (TagField::Mix, Some("Extended Mix".to_owned())),
            ],
        )
        .expect("user values set");

    let preview = preview_tag_writes(&database, &[]).expect("preview");
    assert_eq!(preview.len(), 1, "{file_name}");
    assert!(
        preview[0].blocked.is_none(),
        "{file_name}: {:?}",
        preview[0].blocked
    );
    let fields: Vec<TagField> = preview[0].writes.iter().map(|write| write.field).collect();
    assert_eq!(
        fields,
        [TagField::Title, TagField::Bpm, TagField::Comment],
        "{file_name}"
    );
    assert_eq!(read_tags(&path), seeded_tags, "a preview changes nothing");

    let database = Mutex::new(database);
    let report = apply_tag_writes(&database, &[]).expect("apply");
    assert_eq!(report.written, 1, "{file_name}: {:?}", report.failed);
    assert!(report.failed.is_empty());
    let operation_id = report.operation_id.expect("journaled");

    let written = read_tags(&path);
    assert_eq!(
        tag(&written, TagField::Title),
        Some("New Title"),
        "{file_name}"
    );
    assert_eq!(tag(&written, TagField::Bpm), Some("124"), "{file_name}");
    assert_eq!(tag(&written, TagField::Comment), None, "{file_name}");
    assert_eq!(
        tag(&written, TagField::Artist),
        Some("Old Artist"),
        "{file_name}"
    );
    assert_eq!(
        audio_hash(&path),
        before,
        "{file_name}: the audio is untouched"
    );
    keeps_unknown_items(&path);
    if seeded_version.is_some() {
        assert_eq!(id3v2_major_version(&path), Some(3), "{file_name}");
    }
    assert!(
        !folder.join(format!("{file_name}.resonance-tmp")).exists(),
        "no working copy is left behind"
    );
    {
        let library = database.lock().expect("lock");
        let target = library
            .tag_write_target(track_id)
            .expect("target")
            .expect("track");
        assert_eq!(target.file, written, "the library holds the new file tags");
        assert_eq!(
            target.unwritten,
            [(TagField::Mix, "Extended Mix".to_owned())],
            "only the library-only field stays unwritten"
        );
        assert!(
            preview_tag_writes(&library, &[])
                .expect("preview")
                .is_empty()
        );
        assert_eq!(
            target.stamp.size_bytes,
            fs::metadata(&path).expect("metadata").len(),
            "the library holds the new size, so the next scan leaves the file alone"
        );
        let recent = library.recent_operations(5).expect("journal");
        assert_eq!(recent[0].id, operation_id);
        assert_eq!(recent[0].state, OperationState::Applied);
        assert_eq!(recent[0].summary, "Wrote tags to 1 file");
    }

    let undo = undo_operation(&database, operation_id).expect("undo");
    assert_eq!(undo.restored, 1);
    assert_eq!(undo.operation.state, OperationState::Undone);
    assert_eq!(
        read_tags(&path),
        seeded_tags,
        "{file_name}: undo restores the tags"
    );
    assert_eq!(id3v2_major_version(&path), seeded_version, "{file_name}");
    assert_eq!(audio_hash(&path), before, "{file_name}");
    keeps_unknown_items(&path);
    {
        let library = database.lock().expect("lock");
        assert_eq!(
            preview_tag_writes(&library, &[]).expect("preview").len(),
            1,
            "after undo the user values are unwritten again"
        );
    }
    assert!(matches!(
        undo_operation(&database, operation_id),
        Err(TagWriteError::CannotUndo {
            state: OperationState::Undone,
            ..
        })
    ));
}

fn tone() -> Signal {
    Signal::tone(440.0, 2.0, -6.0, 44_100)
}

#[test]
fn writes_and_undoes_mp3() {
    writes_and_undoes("track.mp3", |path| {
        write_silent_mp3(path, 2.0).expect("mp3 written");
    });
}

#[test]
fn writes_and_undoes_wav() {
    writes_and_undoes("track.wav", |path| {
        tone().write_wav(path).expect("wav written");
    });
}

#[test]
fn writes_and_undoes_aiff() {
    writes_and_undoes("track.aiff", |path| {
        tone().write_aiff(path).expect("aiff written");
    });
}

#[test]
fn writes_and_undoes_flac() {
    writes_and_undoes("track.flac", |path| {
        tone().write_flac(path).expect("flac written");
    });
}

#[test]
fn writes_and_undoes_m4a() {
    writes_and_undoes("track.m4a", |path| {
        write_silent_m4a(path, 2.0).expect("m4a written");
    });
}

#[test]
fn an_m4a_behind_an_id3v2_block_is_refused_untouched() {
    let folder = test_folder("write_tags_leading_id3v2");
    let plain = folder.join("plain.m4a.part");
    write_silent_m4a(&plain, 2.0).expect("m4a written");
    let mut bytes = vec![b'I', b'D', b'3', 3, 0, 0, 0, 0, 0, 0];
    bytes.extend(fs::read(&plain).expect("reads"));
    fs::remove_file(&plain).expect("removed");
    let path = folder.join("track.m4a");
    fs::write(&path, &bytes).expect("written");

    let (mut database, track_id) = library_of(&folder);
    database
        .set_user_tags(&[track_id], &[(TagField::Title, Some("New".to_owned()))])
        .expect("user value set");
    let preview = preview_tag_writes(&database, &[]).expect("preview");
    assert!(matches!(
        preview[0].blocked,
        Some(FileProblem::LeadingId3v2)
    ));

    let database = Mutex::new(database);
    let report = apply_tag_writes(&database, &[]).expect("apply");
    assert_eq!(report.written, 0);
    assert_eq!(report.operation_id, None);
    assert!(matches!(
        report.failed[0].problem,
        FileProblem::LeadingId3v2
    ));
    assert_eq!(fs::read(&path).expect("reads"), bytes);
}

fn touch(path: &Path) {
    let later = SystemTime::now() + Duration::from_secs(120);
    open_rw(path).set_modified(later).expect("time set");
}

#[test]
fn a_file_changed_since_the_scan_is_not_written() {
    let folder = test_folder("write_tags_changed_on_disk");
    let path = folder.join("track.wav");
    tone().write_wav(&path).expect("wav written");
    let (mut database, track_id) = library_of(&folder);
    database
        .set_user_tags(&[track_id], &[(TagField::Title, Some("New".to_owned()))])
        .expect("user value set");
    touch(&path);
    let bytes = fs::read(&path).expect("reads");

    let database = Mutex::new(database);
    let report = apply_tag_writes(&database, &[]).expect("apply");
    assert_eq!(report.written, 0);
    assert!(matches!(
        report.failed[0].problem,
        FileProblem::ChangedOnDisk
    ));
    assert_eq!(fs::read(&path).expect("reads"), bytes);
}

#[test]
fn undo_refuses_a_file_changed_after_the_write() {
    let folder = test_folder("write_tags_undo_after_change");
    let path = folder.join("track.flac");
    tone().write_flac(&path).expect("flac written");
    let (mut database, track_id) = library_of(&folder);
    database
        .set_user_tags(&[track_id], &[(TagField::Title, Some("New".to_owned()))])
        .expect("user value set");
    let database = Mutex::new(database);
    let operation_id = apply_tag_writes(&database, &[])
        .expect("apply")
        .operation_id
        .expect("journaled");
    touch(&path);
    let bytes = fs::read(&path).expect("reads");

    assert!(matches!(
        undo_operation(&database, operation_id),
        Err(TagWriteError::ChangedSinceWrite(_))
    ));
    assert_eq!(fs::read(&path).expect("reads"), bytes);
    let library = database.lock().expect("lock");
    assert_eq!(
        library
            .operation(operation_id)
            .expect("journal")
            .map(|operation| operation.state),
        Some(OperationState::Applied)
    );
}

fn first_with_extension(folder: &Path, extension: &str) -> Option<PathBuf> {
    let mut folders = vec![folder.to_path_buf()];
    while let Some(next) = folders.pop() {
        for entry in fs::read_dir(&next).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
            {
                return Some(path);
            }
        }
    }
    None
}

/// Real files carry tags no generated fixture has. Copies of one per format, when the local set
/// is on this machine, are written and checked; the originals are only read.
#[test]
fn writes_copies_of_real_files_without_touching_their_audio() {
    let local = std::env::var_os("RESONANCE_FIXTURES_LOCAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures-local"));
    if !local.join("manifest.json").is_file() {
        return;
    }
    for extension in ["mp3", "flac", "aiff", "wav", "m4a"] {
        let Some(original) = first_with_extension(&local, extension) else {
            continue;
        };
        let folder = test_folder(&format!("write_tags_local_{extension}"));
        let path = folder.join(format!("copy.{extension}"));
        fs::copy(&original, &path).expect("copied");
        let before = audio_hash(&path);
        let (mut database, track_id) = library_of(&folder);
        database
            .set_user_tags(
                &[track_id],
                &[(TagField::Comment, Some("Checked".to_owned()))],
            )
            .expect("user value set");
        let database = Mutex::new(database);
        let report = apply_tag_writes(&database, &[]).expect("apply");
        if let Some(failed) = report.failed.first() {
            assert!(
                matches!(failed.problem, FileProblem::LeadingId3v2),
                "{extension}: {}",
                failed.problem
            );
            continue;
        }
        assert_eq!(report.written, 1, "{extension}");
        assert_eq!(audio_hash(&path), before, "{extension}");
        assert_eq!(tag(&read_tags(&path), TagField::Comment), Some("Checked"));
    }
}
