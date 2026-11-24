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
use resonance_core::staging::{Proposal, ProposalStatus};
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_fixtures::silence::{write_silent_m4a, write_silent_mp3};
use resonance_library::identity::compute_identity;
use resonance_library::scan::scan;
use resonance_library::write_tags::{
    FileProblem, TagWriteError, accept_and_write, accept_reviewed_and_write, apply_tag_writes,
    preview_tag_writes, undo_operation,
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
fn accepting_and_undoing_a_field_keeps_other_drafts_and_tracks_untouched() {
    let folder = test_folder("accept_scoped");
    let path = folder.join("01.mp3");
    let other = folder.join("02.mp3");
    for file in [&path, &other] {
        write_silent_mp3(file, 1.0).expect("mp3");
    }
    let (mut library, _) = library_of(&folder);
    let tracks = library.tracks().expect("tracks");
    let first = tracks
        .iter()
        .find(|r| r.file.path.ends_with("01.mp3"))
        .expect("first")
        .track_id;
    let second = tracks
        .iter()
        .find(|r| r.file.path.ends_with("02.mp3"))
        .expect("second")
        .track_id;
    let props: Vec<_> = [
        (TagField::Title, "Suggested"),
        (TagField::Artist, "Proposed artist"),
        (TagField::Mix, "Dub"),
    ]
    .into_iter()
    .map(|(field, value)| Proposal {
        field,
        value: value.to_owned(),
        provider: "filename".to_owned(),
        confidence: 0.95,
        audio_matched: false,
    })
    .collect();
    for id in [first, second] {
        library.store_proposals(id, &props).expect("proposals");
    }
    let edits = [
        (TagField::Title, Some("Prior title".to_owned())),
        (TagField::Comment, Some("Unwritten".to_owned())),
    ];
    library.set_user_tags(&[first], &edits).expect("edits");
    let before_layers = library.tag_layers(first).expect("layers").expect("track");
    let before = read_tags(&path);
    let untouched = fs::read(&other).expect("second file");
    let database = Mutex::new(library);
    let save = accept_and_write(&database, &[first, first], Some(&[TagField::Title]), false)
        .expect("accept");
    assert_eq!(save.written, 1, "{:?}", save.failed);
    let saved = read_tags(&path);
    assert_eq!(tag(&saved, TagField::Title), Some("Suggested"));
    assert_eq!(tag(&saved, TagField::Artist), None);
    assert_eq!(tag(&saved, TagField::Comment), None);
    let retry =
        accept_and_write(&database, &[first], Some(&[TagField::Title]), false).expect("retry");
    assert_eq!(retry.operation_id, None);
    undo_operation(&database, save.operation_id.expect("id")).expect("undo");
    assert_eq!(read_tags(&path), before);
    assert_eq!(
        database
            .lock()
            .expect("lock")
            .tag_layers(first)
            .expect("layers")
            .expect("track"),
        before_layers
    );
    assert_eq!(fs::read(&other).expect("second file"), untouched);
    let snapshot = fs::read(&path).expect("first file");
    let library_only =
        accept_and_write(&database, &[first], Some(&[TagField::Mix]), false).expect("accept mix");
    assert_eq!((library_only.written, library_only.library_fields), (0, 1));
    assert_eq!(fs::read(&path).expect("first file"), snapshot);
    assert_eq!(
        tag(
            &database
                .lock()
                .expect("lock")
                .tag_layers(first)
                .expect("layers")
                .expect("track")
                .user,
            TagField::Mix
        ),
        Some("Dub")
    );
    undo_operation(&database, library_only.operation_id.expect("id")).expect("undo mix");
    assert_eq!(
        database
            .lock()
            .expect("lock")
            .tag_layers(first)
            .expect("layers")
            .expect("track"),
        before_layers
    );

    fs::remove_file(&other).expect("remove test copy");
    let batch = accept_and_write(&database, &[first, second], None, false).expect("batch");
    assert_eq!(
        (batch.written, batch.library_fields, batch.failed.len()),
        (1, 1, 1)
    );
    assert_eq!(
        tag(&read_tags(&path), TagField::Title),
        None,
        "bulk leaves the locked title alone"
    );
    let db = database.lock().expect("lock");
    let failed = db.tag_layers(second).expect("layers").expect("track");
    assert!(failed.user.is_empty());
    assert!(
        failed
            .proposals
            .iter()
            .all(|p| p.status == ProposalStatus::Pending)
    );
    drop(db);
    let batch_id = batch.operation_id.expect("batch operation");
    database
        .lock()
        .expect("lock")
        .set_user_tags(
            &[first],
            &[(TagField::Artist, Some("Later edit".to_owned()))],
        )
        .expect("later edit");
    assert!(matches!(
        undo_operation(&database, batch_id),
        Err(TagWriteError::Database(
            resonance_db::DbError::DecisionChanged
        ))
    ));
    assert_eq!(
        tag(&read_tags(&path), TagField::Artist),
        Some("Proposed artist")
    );
    database
        .lock()
        .expect("lock")
        .set_user_tags(
            &[first],
            &[(TagField::Artist, Some("Proposed artist".to_owned()))],
        )
        .expect("restore user value");
    undo_operation(&database, batch_id).expect("undo mixed operation");
    assert_eq!(
        database
            .lock()
            .expect("lock")
            .tag_layers(first)
            .expect("layers")
            .expect("track"),
        before_layers
    );
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

const ALT_COVER: [u8; 67] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn front_cover(path: &Path) -> Option<Vec<u8>> {
    resonance_library::file_tags::read(path)
        .expect("tags read")
        .front_cover
}

fn proposal(field: TagField, value: &str) -> Proposal {
    Proposal {
        field,
        value: value.to_owned(),
        provider: "discogs".to_owned(),
        confidence: 0.96,
        audio_matched: false,
    }
}

#[test]
fn accept_reviewed_protects_embedded_cover_in_bulk_and_replaces_when_explicitly_selected() {
    let folder = test_folder("accept_reviewed_artwork_guard");
    let with_path = folder.join("01_with_cover.mp3");
    let without_path = folder.join("02_no_cover.mp3");
    write_silent_mp3(&with_path, 2.0).expect("mp3 with cover");
    seed(&with_path);
    write_silent_mp3(&without_path, 2.0).expect("mp3 without cover");

    let mut database = Database::open_in_memory().expect("database");
    database.set_artwork_folder(folder.parent().expect("parent").join("artwork"));
    scan(&mut database, &folder, &[]).expect("scan");
    let tracks = database.tracks().expect("tracks");
    let id_with = tracks
        .iter()
        .find(|t| t.file.path.ends_with("01_with_cover.mp3"))
        .expect("track with cover")
        .track_id;
    let id_without = tracks
        .iter()
        .find(|t| t.file.path.ends_with("02_no_cover.mp3"))
        .expect("track without cover")
        .track_id;

    let alt_art = database
        .artwork_store()
        .expect("store")
        .put(&ALT_COVER)
        .expect("stored alt cover");
    database
        .store_proposals(id_with, &[proposal(TagField::Artist, "Artist A")])
        .expect("proposals 1");
    database
        .store_proposals(id_without, &[proposal(TagField::Artist, "Artist B")])
        .expect("proposals 2");
    database
        .store_proposed_artwork(id_with, &alt_art, "discogs", "https://discogs.example/1")
        .expect("proposed art 1");
    database
        .store_proposed_artwork(id_without, &alt_art, "discogs", "https://discogs.example/2")
        .expect("proposed art 2");

    let database = Mutex::new(database);
    let bulk = accept_reviewed_and_write(&database, &[id_with, id_without], None, true, None)
        .expect("bulk accept");
    assert_eq!(bulk.written, 2, "{:?}", bulk.failed);
    assert!(bulk.failed.is_empty());
    assert_eq!(front_cover(&with_path).as_deref(), Some(COVER.as_slice()));
    assert_eq!(
        front_cover(&without_path).as_deref(),
        Some(ALT_COVER.as_slice())
    );

    let replace = accept_reviewed_and_write(&database, &[id_with], Some(&[]), true, None)
        .expect("explicit artwork accept");
    assert_eq!(replace.written, 1, "{:?}", replace.failed);
    assert_eq!(
        front_cover(&with_path).as_deref(),
        Some(ALT_COVER.as_slice())
    );

    undo_operation(&database, replace.operation_id.expect("replace op")).expect("undo replace");
    assert_eq!(front_cover(&with_path).as_deref(), Some(COVER.as_slice()));
}

#[test]
fn accept_reviewed_m4a_writes_supported_and_rounded_bpm_and_keeps_unsupported_in_library() {
    let folder = test_folder("accept_reviewed_m4a_formats");
    let path = folder.join("track.m4a");
    write_silent_m4a(&path, 2.0).expect("m4a written");
    let (mut database, track_id) = library_of(&folder);
    database
        .store_proposals(
            track_id,
            &[
                proposal(TagField::Title, "Night"),
                proposal(TagField::Bpm, "127.6"),
                proposal(TagField::CatNo, "VEC001"),
                proposal(TagField::TrackNumber, "A1"),
                proposal(TagField::Mix, "Extended"),
            ],
        )
        .expect("proposals stored");

    let database = Mutex::new(database);
    let report = accept_reviewed_and_write(
        &database,
        &[track_id],
        Some(&[
            TagField::Title,
            TagField::Bpm,
            TagField::CatNo,
            TagField::TrackNumber,
            TagField::Mix,
        ]),
        false,
        None,
    )
    .expect("m4a accept");
    assert_eq!(report.written, 1, "{:?}", report.failed);
    assert_eq!(report.library_fields, 2);
    assert_eq!(report.reverted_fields, 0);
    assert!(report.failed.is_empty());
    let tags = read_tags(&path);
    assert_eq!(tag(&tags, TagField::Title), Some("Night"));
    assert_eq!(tag(&tags, TagField::Bpm), Some("127.6"));
    assert_eq!(tag(&tags, TagField::CatNo), Some("VEC001"));

    {
        let mut library = database.lock().expect("lock");
        assert!(
            library
                .tracks_with_unwritten_tags(&[])
                .expect("unwritten")
                .is_empty()
        );
        library
            .store_proposals(
                track_id,
                &[
                    proposal(TagField::Title, "Morning"),
                    proposal(TagField::TrackNumber, "3/12"),
                ],
            )
            .expect("second proposals");
    }

    let follow_up = accept_reviewed_and_write(
        &database,
        &[track_id],
        Some(&[TagField::Title, TagField::TrackNumber]),
        false,
        None,
    )
    .expect("second accept");
    assert_eq!(follow_up.written, 1, "{:?}", follow_up.failed);
    assert_eq!(follow_up.library_fields, 1);
    assert!(follow_up.failed.is_empty());
    assert!(
        database
            .lock()
            .expect("lock")
            .tracks_with_unwritten_tags(&[])
            .expect("still clean")
            .is_empty()
    );

    undo_operation(&database, follow_up.operation_id.expect("second op")).expect("undo second");
    let undone_tags = read_tags(&path);
    assert_eq!(tag(&undone_tags, TagField::Title), Some("Night"));
    {
        let library = database.lock().expect("lock");
        assert!(
            library
                .tracks_with_unwritten_tags(&[])
                .expect("still clean after undo")
                .is_empty()
        );
        let layers = library
            .tag_layers(track_id)
            .expect("layers")
            .expect("present");
        assert!(
            layers
                .user
                .iter()
                .any(|(f, v)| *f == TagField::TrackNumber && v == "A1")
        );
    }

    database
        .lock()
        .expect("lock")
        .store_proposals(track_id, &[proposal(TagField::TrackNumber, "0")])
        .expect("zero track number proposal");
    let lib_only = accept_reviewed_and_write(
        &database,
        &[track_id],
        Some(&[TagField::TrackNumber]),
        false,
        None,
    )
    .expect("zero accept");
    assert_eq!(lib_only.written, 0, "{:?}", lib_only.failed);
    assert_eq!(lib_only.library_fields, 1);
    assert!(lib_only.failed.is_empty());
    assert!(
        database
            .lock()
            .expect("lock")
            .tracks_with_unwritten_tags(&[])
            .expect("clean after zero accept")
            .is_empty()
    );

    undo_operation(&database, lib_only.operation_id.expect("zero op")).expect("undo zero");
    let library = database.lock().expect("lock");
    assert!(
        library
            .tracks_with_unwritten_tags(&[])
            .expect("clean after library undo")
            .is_empty()
    );
    let layers = library
        .tag_layers(track_id)
        .expect("layers")
        .expect("present");
    assert!(
        layers
            .user
            .iter()
            .any(|(f, v)| *f == TagField::TrackNumber && v == "A1")
    );
}

#[test]
fn accept_reviewed_reverting_dirty_user_tag_clears_dirty_and_undoes_cleanly() {
    let folder = test_folder("accept_reviewed_revert_dirty");
    let path = folder.join("track.flac");
    tone().write_flac(&path).expect("flac written");
    seed(&path);
    let (mut database, track_id) = library_of(&folder);
    database
        .set_user_tags(
            &[track_id],
            &[(TagField::Artist, Some("Manual Edit".to_owned()))],
        )
        .expect("manual edit set");
    assert_eq!(
        database.tracks_with_unwritten_tags(&[]).expect("unwritten"),
        vec![track_id]
    );
    database
        .store_proposals(track_id, &[proposal(TagField::Artist, "Old Artist")])
        .expect("proposal stored");

    let database = Mutex::new(database);
    let report = accept_reviewed_and_write(
        &database,
        &[track_id],
        Some(&[TagField::Artist]),
        false,
        None,
    )
    .expect("revert accept");
    assert_eq!(report.written, 0);
    assert_eq!(report.library_fields, 0);
    assert_eq!(report.reverted_fields, 1);
    assert!(report.failed.is_empty());
    let op_id = report.operation_id.expect("journaled revert");
    {
        let library = database.lock().expect("lock");
        assert_eq!(
            library.operation(op_id).expect("op").map(|o| o.state),
            Some(OperationState::Applied)
        );
        assert!(
            library
                .tracks_with_unwritten_tags(&[])
                .expect("dirty cleared")
                .is_empty()
        );
    }

    undo_operation(&database, op_id).expect("undo revert");
    let library = database.lock().expect("lock");
    assert_eq!(
        library
            .tracks_with_unwritten_tags(&[])
            .expect("dirty restored"),
        vec![track_id]
    );
    let layers = library
        .tag_layers(track_id)
        .expect("layers")
        .expect("present");
    assert!(
        layers
            .user
            .iter()
            .any(|(f, v)| *f == TagField::Artist && v == "Manual Edit")
    );
}
