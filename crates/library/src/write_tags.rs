//! Writes the user's tag values into files, one journaled operation per apply, with undo.
//!
//! A file is never edited in place. It is copied to a sibling on the same volume, the tags are
//! changed in the copy, the copy's decoded audio is checked against the original's, and only then
//! does the copy replace the original. Only the items for the fields being written change: frames
//! the library does not understand (private frames, DJ software data, other pictures, custom
//! text) are carried through untouched. A picture the user chose replaces the front cover alone,
//! and the cover it replaced is kept in the picture store so undo can put it back.

use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use lofty::config::{ParseOptions, ParsingMode, WriteOptions};
use lofty::error::{FileEncodingError, FileParseError};
use lofty::flac::FlacFile;
use lofty::id3::v2::{Id3v2Tag, Id3v2Version};
use lofty::iff::aiff::AiffFile;
use lofty::iff::wav::WavFile;
use lofty::mp4::{Ilst, Mp4File};
use lofty::mpeg::MpegFile;
use lofty::ogg::OggPictureStorage;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::{MimeType, Picture, PictureInformation, PictureType};
use lofty::prelude::{AudioFile, ItemKey, MergeTag, SplitTag};
use lofty::tag::{ItemValue, Tag, TagItem, TagType};
use resonance_core::artwork::ImageKind;
use resonance_core::identity::ContentHash;
use resonance_core::journal::{OperationKind, OperationState};
use resonance_core::library::{AudioFormat, TagField};
use resonance_core::staging::{FieldWrite, staging_diff, tag_writes};
use resonance_db::{
    ArtworkStore, Database, DbError, FileDecision, FileRevision, FileStamp, StoredArtwork,
    StoredOperation, TagWriteTarget,
};
use serde::{Deserialize, Serialize};

use crate::file_tags::{self, has_leading_id3v2};
use crate::identity::{compute_identity, full_asset_path};
use crate::journal::JournalError;
use crate::scan::stamp;

#[derive(Debug)]
pub enum TagWriteError {
    Database(DbError),
    Journal(serde_json::Error),
    NoOperation(i64),
    CannotUndo {
        id: i64,
        state: OperationState,
    },
    /// The file changed after the operation wrote it, so undoing would lose that change.
    ChangedSinceWrite(PathBuf),
    File {
        path: PathBuf,
        problem: FileProblem,
    },
}

/// Why one file was not written.
#[derive(Debug)]
pub enum FileProblem {
    Missing,
    /// The file is not what the last scan saw. A rescan reads it again first.
    ChangedOnDisk,
    DecisionChanged,
    UnsupportedTag,
    /// An ID3v2 block sits in front of the MP4 container. Writing atoms after it could break the
    /// file, so it needs a container repair first.
    LeadingId3v2,
    Io(io::Error),
    Parse(FileParseError),
    Encode(FileEncodingError),
    AudioUnreadable,
    /// The written copy decoded differently from the original, so it was thrown away.
    AudioChanged,
}

impl fmt::Display for FileProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("the file is missing"),
            Self::ChangedOnDisk => {
                f.write_str("the file changed since the last scan. Scan its folder again first")
            }
            Self::DecisionChanged => f.write_str("the proposals changed. Review them again"),
            Self::UnsupportedTag => {
                f.write_str("the file format could not store the selected value")
            }
            Self::LeadingId3v2 => f.write_str(
                "an ID3 tag sits in front of the MP4 data. The file needs a container repair first",
            ),
            Self::Io(source) => write!(f, "the file could not be read or written: {source}"),
            Self::Parse(source) => write!(f, "the tags could not be read: {source}"),
            Self::Encode(source) => write!(f, "the tags could not be written: {source}"),
            Self::AudioUnreadable => f.write_str("the audio could not be decoded to check it"),
            Self::AudioChanged => f.write_str(
                "the written copy decoded differently from the original, so it was not used",
            ),
        }
    }
}

impl fmt::Display for TagWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(source) => source.fmt(f),
            Self::Journal(source) => write!(f, "The journal entry could not be read: {source}"),
            Self::NoOperation(id) => write!(f, "There is no operation {id} in the journal."),
            Self::CannotUndo { id, state } => write!(
                f,
                "Operation {id} is {}, and only an applied operation can be undone.",
                state.name()
            ),
            Self::ChangedSinceWrite(path) => write!(
                f,
                "{} changed after its tags were written, so undoing would lose that change.",
                path.display()
            ),
            Self::File { path, problem } => write!(f, "{}: {problem}.", path.display()),
        }
    }
}

impl std::error::Error for TagWriteError {}

impl From<DbError> for TagWriteError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

impl From<serde_json::Error> for TagWriteError {
    fn from(source: serde_json::Error) -> Self {
        Self::Journal(source)
    }
}

/// One file's part of a preview.
#[derive(Debug)]
pub struct PlannedFile {
    pub target: TagWriteTarget,
    pub full_path: PathBuf,
    pub writes: Vec<FieldWrite>,
    decision: Option<AcceptedFields>,
    /// Set when the file cannot be written as things stand.
    pub blocked: Option<FileProblem>,
}

#[derive(Debug)]
struct AcceptedFields {
    fields: Vec<(TagField, String, Option<String>, bool)>,
    previous_cover: Option<String>,
    library_fields: u32,
    reverted_fields: u32,
}

#[derive(Debug)]
pub struct FailedFile {
    pub track_id: i64,
    pub path: PathBuf,
    pub problem: FileProblem,
}

#[derive(Debug, Default)]
pub struct TagWriteReport {
    /// `None` when no file was written.
    pub operation_id: Option<i64>,
    pub written: u32,
    pub library_fields: u32,
    pub reverted_fields: u32,
    pub failed: Vec<FailedFile>,
}

#[derive(Debug)]
pub struct UndoReport {
    pub operation: StoredOperation,
    pub restored: u32,
}

/// The items one field takes in one tag. `key` indexes `write_keys(field)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FieldItems {
    field: String,
    key: usize,
    values: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ForwardFile {
    track_id: i64,
    asset_id: i64,
    path: String,
    fields: Vec<(String, Option<String>)>,
    /// The hash of the picture written as the front cover.
    #[serde(default)]
    cover: Option<String>,
}

/// The front cover a file had before a write changed it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoverBefore {
    /// `None` when the file had no front cover.
    hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InverseFile {
    asset_id: i64,
    /// The ID3v2 version the file had before, so undo writes the tag back the way it was.
    id3v23: bool,
    items: Vec<FieldItems>,
    /// Set only when the write changed the front cover.
    #[serde(default)]
    cover: Option<CoverBefore>,
    size_bytes: u64,
    modified_ns: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Inverse {
    files: Vec<InverseFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    library_tracks: Vec<i64>,
    #[serde(default)]
    restored_files: usize,
    #[serde(default)]
    restored_library_tracks: usize,
}

/// The item keys a field is written under, in the order the reader looks for them. A file gets
/// the value under every one its tag format has a frame for, so any reader finds the same value.
/// A year goes in the date frame most software reads first.
fn write_keys(field: TagField) -> &'static [ItemKey] {
    match field {
        TagField::Year => &[ItemKey::RecordingDate, ItemKey::Year],
        _ => file_tags::keys(field),
    }
}

/// The keys of `field` that `tag_type` has a frame for, one per frame.
fn keys_in(field: TagField, tag_type: TagType) -> Vec<usize> {
    let mut frames: Vec<&str> = Vec::new();
    let mut keys = Vec::new();
    for (index, key) in write_keys(field).iter().enumerate() {
        if let Some(frame) = key.map_key(tag_type)
            && !frames.contains(&frame)
        {
            frames.push(frame);
            keys.push(index);
        }
    }
    keys
}

/// An integer BPM frame holds whole beats per minute.
fn value_for(key: ItemKey, value: &str) -> String {
    match (key, value.parse::<f64>()) {
        (ItemKey::IntegerBpm, Ok(bpm)) => format!("{}", bpm.round()),
        _ => value.to_owned(),
    }
}

fn tag_type_of(format: AudioFormat) -> TagType {
    match format {
        AudioFormat::Mp3 | AudioFormat::Wav | AudioFormat::Aiff => TagType::Id3v2,
        AudioFormat::Flac => TagType::VorbisComments,
        AudioFormat::M4a => TagType::Mp4Ilst,
    }
}

fn mp4_u16_positive(part: &str) -> bool {
    let s = part.trim();
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u16>().is_ok_and(|n| n > 0)
}

fn mp4_track_number_supported(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.is_empty() || mp4_u16_positive(trimmed)
}

fn writable_in_format(format: AudioFormat, field: TagField, value: Option<&str>) -> bool {
    if !field.is_written_to_files() {
        return false;
    }
    let tag_type = tag_type_of(format);
    if keys_in(field, tag_type).is_empty() {
        return false;
    }
    if tag_type == TagType::Mp4Ilst && field == TagField::TrackNumber {
        return value.is_none_or(mp4_track_number_supported);
    }
    true
}

fn written_value_matches(
    format: AudioFormat,
    field: TagField,
    intended: Option<&str>,
    actual: Option<&str>,
) -> bool {
    if actual == intended {
        return true;
    }
    let tag_type = tag_type_of(format);
    let uses_integer_bpm = field == TagField::Bpm
        && keys_in(TagField::Bpm, tag_type)
            .iter()
            .any(|&idx| write_keys(TagField::Bpm)[idx] == ItemKey::IntegerBpm);
    if uses_integer_bpm
        && let (Some(intended), Some(actual)) = (intended, actual)
        && let (Ok(want), Ok(got)) = (intended.trim().parse::<f64>(), actual.trim().parse::<f64>())
        && want.is_finite()
        && got.is_finite()
    {
        return want.round() == got.round();
    }
    false
}

enum TagEdit<'a> {
    Write(&'a [FieldWrite]),
    Restore(&'a [FieldItems]),
}

/// What a write does to the file's front cover.
#[derive(Debug, Clone, Copy)]
enum CoverEdit<'a> {
    Keep,
    Set(&'a [u8], ImageKind),
    Remove,
}

fn front_cover_picture(bytes: &[u8], kind: ImageKind) -> Picture {
    let mime = match kind {
        ImageKind::Jpeg => MimeType::Jpeg,
        ImageKind::Png => MimeType::Png,
    };
    Picture::unchecked(bytes.to_vec())
        .pic_type(PictureType::CoverFront)
        .mime_type(mime)
        .build()
}

/// Replaces the generic tag's front cover and returns the one it had, or `None` when the cover
/// is kept.
fn edit_cover(tag: &mut Tag, cover: CoverEdit<'_>) -> Option<Option<Vec<u8>>> {
    if matches!(cover, CoverEdit::Keep) {
        return None;
    }
    // MP4 pictures carry no type, and players show the first as the cover.
    let index = tag
        .pictures()
        .iter()
        .position(|picture| picture.pic_type() == PictureType::CoverFront)
        .or_else(|| (tag.tag_type() == TagType::Mp4Ilst && tag.picture_count() > 0).then_some(0));
    let prior = index.map(|index| tag.pictures()[index].data().to_vec());
    match (cover, index) {
        (CoverEdit::Set(bytes, kind), Some(index)) => {
            tag.set_picture(index, front_cover_picture(bytes, kind));
        }
        (CoverEdit::Set(bytes, kind), None) => tag.push_picture(front_cover_picture(bytes, kind)),
        (CoverEdit::Remove, Some(index)) => {
            tag.remove_picture(index);
        }
        (CoverEdit::Remove | CoverEdit::Keep, _) => {}
    }
    Some(prior)
}

fn picture_problem(error: impl fmt::Display) -> FileProblem {
    FileProblem::Io(io::Error::new(
        io::ErrorKind::InvalidData,
        error.to_string(),
    ))
}

/// The same for a FLAC file, which keeps its pictures in their own blocks beside the comments.
fn edit_flac_cover(
    audio: &mut FlacFile,
    cover: CoverEdit<'_>,
) -> Result<Option<Option<Vec<u8>>>, FileProblem> {
    if matches!(cover, CoverEdit::Keep) {
        return Ok(None);
    }
    let prior = audio
        .pictures()
        .iter()
        .find(|(picture, _)| picture.pic_type() == PictureType::CoverFront)
        .map(|(picture, _)| picture.data().to_vec());
    audio.remove_picture_type(PictureType::CoverFront);
    if let CoverEdit::Set(bytes, kind) = cover {
        let picture = front_cover_picture(bytes, kind);
        // The block's size fields only describe the picture, so one the parser cannot measure
        // is still written.
        let information = PictureInformation::from_picture(&picture).unwrap_or_default();
        audio
            .insert_picture(picture, Some(information))
            .map_err(picture_problem)?;
    }
    Ok(Some(prior))
}

/// What an edit replaced: the items the fields held, and the front cover when it changed.
struct Prior {
    items: Vec<FieldItems>,
    cover: Option<Option<Vec<u8>>>,
}

/// Sets the items `edit` names on the generic tag and returns what they held before.
fn edit_items(tag: &mut Tag, edit: &TagEdit<'_>) -> Vec<FieldItems> {
    let sets: Vec<FieldItems> = match edit {
        TagEdit::Write(writes) => writes
            .iter()
            .flat_map(|write| {
                keys_in(write.field, tag.tag_type())
                    .into_iter()
                    .map(|key| FieldItems {
                        field: write.field.name().to_owned(),
                        key,
                        values: write
                            .to
                            .iter()
                            .map(|value| value_for(write_keys(write.field)[key], value))
                            .collect(),
                    })
            })
            .collect(),
        TagEdit::Restore(items) => items.to_vec(),
    };
    let mut prior = Vec::with_capacity(sets.len());
    for set in sets {
        let Some(key) = TagField::from_name(&set.field)
            .and_then(|field| write_keys(field).get(set.key).copied())
        else {
            continue;
        };
        prior.push(FieldItems {
            values: tag.get_strings(key).map(str::to_owned).collect(),
            ..set.clone()
        });
        tag.remove_key(key);
        for value in set.values {
            tag.push(TagItem::new(key, ItemValue::Text(value)));
        }
    }
    prior
}

/// Edits a format's own tag through its generic view. Splitting leaves everything the generic
/// view cannot hold in the remainder, and merging puts it back.
fn edit_tag<T>(tag: T, edit: &TagEdit<'_>, cover: CoverEdit<'_>) -> (T, Prior)
where
    T: SplitTag,
    T::Remainder: MergeTag<Merged = T>,
{
    let (remainder, mut generic) = tag.split_tag();
    let items = edit_items(&mut generic, edit);
    let cover = edit_cover(&mut generic, cover);
    (remainder.merge_tag(generic), Prior { items, cover })
}

fn edit_id3v2(
    tag: Option<Id3v2Tag>,
    edit: &TagEdit<'_>,
    cover: CoverEdit<'_>,
) -> (Id3v2Tag, Prior, bool) {
    let tag = tag.unwrap_or_default();
    let was_v23 = !matches!(tag.original_version(), Id3v2Version::V4);
    let (tag, prior) = edit_tag(tag, edit, cover);
    (tag, prior, was_v23)
}

/// Changes the tags of the open file and returns what the edited items held before and whether
/// its ID3v2 tag, if it has one, was version 2.3.
fn edit_file(
    file: &mut File,
    format: AudioFormat,
    edit: &TagEdit<'_>,
    cover: CoverEdit<'_>,
    use_id3v23: bool,
) -> Result<(Prior, bool), FileProblem> {
    let parse = ParseOptions::new();
    let relaxed = ParseOptions::new().parsing_mode(ParsingMode::Relaxed);
    let options = WriteOptions::new().use_id3v23(use_id3v23);
    // The same lenient second read the scan uses, so a file the library could read can be
    // written. Only a file the strict read refuses takes it, and a frame the lenient read skips
    // is not written back, so undo cannot bring that frame back either. The copy's audio is
    // compared with the original before it replaces it.
    macro_rules! read {
        ($file_type:ty) => {
            match <$file_type>::read_from(file, parse) {
                Ok(audio) => audio,
                Err(first) => {
                    file.rewind().map_err(FileProblem::Io)?;
                    <$file_type>::read_from(file, relaxed).map_err(|_| FileProblem::Parse(first))?
                }
            }
        };
    }
    macro_rules! with_id3v2 {
        ($file_type:ty) => {{
            let mut audio = read!($file_type);
            let (tag, prior, was_v23) = edit_id3v2(audio.remove_id3v2(), edit, cover);
            audio.set_id3v2(tag);
            file.rewind().map_err(FileProblem::Io)?;
            audio.save_to(file, options).map_err(FileProblem::Encode)?;
            Ok((prior, was_v23))
        }};
    }
    match format {
        AudioFormat::Mp3 => with_id3v2!(MpegFile),
        AudioFormat::Wav => with_id3v2!(WavFile),
        AudioFormat::Aiff => with_id3v2!(AiffFile),
        AudioFormat::Flac => {
            let mut audio = read!(FlacFile);
            let tag: VorbisComments = audio.remove_vorbis_comments().unwrap_or_default();
            let (tag, mut prior) = edit_tag(tag, edit, CoverEdit::Keep);
            audio.set_vorbis_comments(tag);
            prior.cover = edit_flac_cover(&mut audio, cover)?;
            file.rewind().map_err(FileProblem::Io)?;
            audio.save_to(file, options).map_err(FileProblem::Encode)?;
            Ok((prior, true))
        }
        AudioFormat::M4a => {
            let mut audio = read!(Mp4File);
            let tag: Ilst = audio.remove_ilst().unwrap_or_default();
            let (tag, prior) = edit_tag(tag, edit, cover);
            audio.set_ilst(tag);
            file.rewind().map_err(FileProblem::Io)?;
            audio.save_to(file, options).map_err(FileProblem::Encode)?;
            Ok((prior, true))
        }
    }
}

/// The working copy beside a file. It is removed unless it replaced the file.
struct WorkingCopy {
    path: PathBuf,
    kept: bool,
}

impl WorkingCopy {
    fn path_of(original: &Path) -> PathBuf {
        let mut name = original.file_name().unwrap_or_default().to_os_string();
        name.push(".resonance-tmp");
        original.with_file_name(name)
    }

    fn of(original: &Path) -> Result<Self, FileProblem> {
        let path = Self::path_of(original);
        let mut target = File::create_new(&path).map_err(FileProblem::Io)?;
        let copy = Self { path, kept: false };
        let mut source = File::open(original).map_err(FileProblem::Io)?;
        io::copy(&mut source, &mut target).map_err(FileProblem::Io)?;
        target
            .set_permissions(source.metadata().map_err(FileProblem::Io)?.permissions())
            .map_err(FileProblem::Io)?;
        Ok(copy)
    }

    fn replace(mut self, original: &Path, expected: FileStamp) -> io::Result<()> {
        if current_stamp(original).ok() != Some(expected) {
            return Err(io::Error::other(
                "the file changed while its tags were being written",
            ));
        }
        fs::rename(&self.path, original)?;
        self.kept = true;
        Ok(())
    }
}

impl Drop for WorkingCopy {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_file(&self.path);
        }
    }
}

struct Rewritten {
    copy: WorkingCopy,
    prior: Prior,
    was_v23: bool,
    stamp: FileStamp,
    byte_hash: ContentHash,
    tags: Vec<(TagField, String)>,
    front_cover: Option<Vec<u8>>,
}

fn current_stamp(path: &Path) -> Result<FileStamp, FileProblem> {
    match fs::metadata(path) {
        Ok(metadata) => stamp(&metadata).map_err(FileProblem::Io),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(FileProblem::Missing),
        Err(error) => Err(FileProblem::Io(error)),
    }
}

fn check_writable(
    path: &Path,
    format: AudioFormat,
    expected: FileStamp,
) -> Result<(), FileProblem> {
    if current_stamp(path)? != expected {
        return Err(FileProblem::ChangedOnDisk);
    }
    if format == AudioFormat::M4a && has_leading_id3v2(path).map_err(FileProblem::Io)? {
        return Err(FileProblem::LeadingId3v2);
    }
    Ok(())
}

fn rewrite(
    path: &Path,
    format: AudioFormat,
    edit: &TagEdit<'_>,
    cover: CoverEdit<'_>,
    use_id3v23: bool,
    expected: FileStamp,
) -> Result<Rewritten, FileProblem> {
    check_writable(path, format, expected)?;
    let before = compute_identity(path)
        .map_err(FileProblem::Io)?
        .audio
        .ok_or(FileProblem::AudioUnreadable)?;

    let copy = WorkingCopy::of(path)?;
    let (prior, was_v23) = {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&copy.path)
            .map_err(FileProblem::Io)?;
        edit_file(&mut file, format, edit, cover, use_id3v23)?
    };
    let written = compute_identity(&copy.path).map_err(FileProblem::Io)?;
    if written.audio.as_ref() != Some(&before) {
        return Err(FileProblem::AudioChanged);
    }
    let contents = file_tags::read(&copy.path)
        .map_err(|error| FileProblem::Io(io::Error::other(error.to_string())))?;
    Ok(Rewritten {
        prior,
        was_v23,
        stamp: current_stamp(&copy.path)?,
        copy,
        byte_hash: written.byte_hash,
        tags: contents.tags,
        front_cover: contents.front_cover,
    })
}

/// The front cover a rewritten file holds, as the library records it.
fn stored_cover(store: Option<&ArtworkStore>, cover: Option<&[u8]>) -> Option<StoredArtwork> {
    store?.put(cover?).ok()
}

fn locked(database: &Mutex<Database>) -> MutexGuard<'_, Database> {
    database.lock().unwrap_or_else(PoisonError::into_inner)
}

fn filter_writable_writes(target: &TagWriteTarget, writes: Vec<FieldWrite>) -> Vec<FieldWrite> {
    writes
        .into_iter()
        .filter(|w| {
            if !writable_in_format(target.format, w.field, w.to.as_deref()) {
                return false;
            }
            let in_file = target
                .file
                .iter()
                .find(|(f, _)| *f == w.field)
                .map(|(_, v)| v.as_str());
            !written_value_matches(target.format, w.field, w.to.as_deref(), in_file)
        })
        .collect()
}

fn plan_track(database: &Database, track_id: i64) -> Result<Option<PlannedFile>, DbError> {
    let Some(target) = database.tag_write_target(track_id)? else {
        return Ok(None);
    };
    let writes = filter_writable_writes(&target, tag_writes(&target.file, &target.unwritten));
    if writes.is_empty() && target.artwork.is_none() {
        return Ok(None);
    }
    let full_path = full_asset_path(&target.source_folder, &target.path);
    let blocked = if target.missing {
        Some(FileProblem::Missing)
    } else {
        check_writable(&full_path, target.format, target.stamp).err()
    };
    Ok(Some(PlannedFile {
        target,
        full_path,
        writes,
        decision: None,
        blocked,
    }))
}

/// What applying would write: each file with unwritten user values among `track_ids` (every
/// track when it is empty), the fields that would change, and why a file cannot be written.
/// Nothing on disk changes.
pub fn preview_tag_writes(
    database: &Database,
    track_ids: &[i64],
) -> Result<Vec<PlannedFile>, DbError> {
    let mut planned = Vec::new();
    for track_id in database.tracks_with_unwritten_tags(track_ids)? {
        planned.extend(plan_track(database, track_id)?);
    }
    Ok(planned)
}

fn file_count(count: usize) -> String {
    match count {
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    }
}

/// Writes the user's unwritten values into each file the preview lists, as one journaled
/// operation. The database lock is held only between files, never while a file is decoded.
pub fn apply_tag_writes(
    database: &Mutex<Database>,
    track_ids: &[i64],
) -> Result<TagWriteReport, TagWriteError> {
    let _operation = crate::file_operation();
    let (planned, store) = {
        let library = locked(database);
        (
            preview_tag_writes(&library, track_ids)?,
            library.artwork_store(),
        )
    };
    write_plans(database, planned, store)
}

/// Accepts the displayed proposals and writes just those fields to the selected files.
pub fn accept_and_write(
    database: &Mutex<Database>,
    track_ids: &[i64],
    selected: Option<&[TagField]>,
    include_artwork: bool,
) -> Result<TagWriteReport, TagWriteError> {
    accept_reviewed_and_write(database, track_ids, selected, include_artwork, None)
}

pub fn accept_reviewed_and_write(
    database: &Mutex<Database>,
    track_ids: &[i64],
    selected: Option<&[TagField]>,
    include_artwork: bool,
    expected_review: Option<&str>,
) -> Result<TagWriteReport, TagWriteError> {
    let _operation = crate::file_operation();
    let mut seen = HashSet::with_capacity(track_ids.len());
    let unique_ids: Vec<i64> = track_ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect();
    if unique_ids.is_empty() {
        return Ok(TagWriteReport::default());
    }

    let (planned, store) = {
        let library = locked(database);
        let mut targets = library.tag_write_targets_by_track_ids(&unique_ids)?;
        let mut layers_by_track = library.tag_layers_by_track_ids(&unique_ids)?;
        let mut artwork_by_track = library.artwork_states_by_track_ids(&unique_ids)?;
        let mut user_covers = library.user_cover_hashes_by_track_ids(&unique_ids)?;
        let identifications = if expected_review.is_some() {
            library.identifications_by_track_ids(&unique_ids)?
        } else {
            std::collections::HashMap::new()
        };

        let mut planned = Vec::with_capacity(unique_ids.len());
        for &id in &unique_ids {
            let Some(mut target) = targets.remove(&id) else {
                if expected_review.is_some() {
                    return Err(TagWriteError::Database(DbError::DecisionChanged));
                }
                continue;
            };
            let Some(layers) = layers_by_track.remove(&id) else {
                if expected_review.is_some() {
                    return Err(TagWriteError::Database(DbError::DecisionChanged));
                }
                continue;
            };
            let art_state = artwork_by_track.remove(&id).unwrap_or_default();
            if let Some(expected) = expected_review
                && layers.review_token(id, identifications.get(&id), &art_state) != expected
            {
                return Err(TagWriteError::Database(DbError::DecisionChanged));
            }
            let accepted: Vec<_> = staging_diff(&layers.file, &layers.user, &layers.proposals)
                .into_iter()
                .filter(|d| {
                    d.changes()
                        && selected.map_or(!d.locked_by_user, |names| names.contains(&d.field))
                })
                .filter_map(|d| {
                    let previous = d.locked_by_user.then_some(d.current_value).flatten();
                    Some((d.field, d.proposal?.proposal.value, previous))
                })
                .collect();
            let previous_cover = user_covers.remove(&id);
            let has_any_cover = art_state.shown.is_some();
            target.artwork = if include_artwork && (selected.is_some() || !has_any_cover) {
                art_state.proposed.filter(|p| p.pending).map(|p| p.artwork)
            } else {
                None
            };
            if accepted.is_empty() && target.artwork.is_none() {
                continue;
            }
            let writable_choices: Vec<_> = accepted
                .iter()
                .filter(|(field, value, _)| {
                    writable_in_format(target.format, *field, Some(value.as_str()))
                })
                .map(|(field, value, _)| (*field, value.clone()))
                .collect();
            let writes =
                filter_writable_writes(&target, tag_writes(&target.file, &writable_choices));

            let mut fields = Vec::with_capacity(accepted.len());
            let mut library_fields = 0u32;
            let mut reverted_fields = 0u32;
            for (field, value, previous) in accepted {
                let will_write = writes.iter().any(|w| w.field == field);
                let in_file = target
                    .file
                    .iter()
                    .find(|(f, _)| *f == field)
                    .map(|(_, v)| v.as_str());
                let matches_file =
                    written_value_matches(target.format, field, Some(value.as_str()), in_file);
                if !will_write {
                    if matches_file {
                        reverted_fields += 1;
                    } else {
                        library_fields += 1;
                    }
                }
                fields.push((field, value, previous, will_write));
            }
            let full_path = full_asset_path(&target.source_folder, &target.path);
            let changes_file = !writes.is_empty() || target.artwork.is_some();
            let blocked = if target.missing && changes_file {
                Some(FileProblem::Missing)
            } else {
                None
            };
            planned.push(PlannedFile {
                target,
                full_path,
                writes,
                blocked,
                decision: Some(AcceptedFields {
                    fields,
                    previous_cover,
                    library_fields,
                    reverted_fields,
                }),
            });
        }
        (planned, library.artwork_store())
    };
    write_plans(database, planned, store)
}

fn write_plans(
    database: &Mutex<Database>,
    planned: Vec<PlannedFile>,
    store: Option<ArtworkStore>,
) -> Result<TagWriteReport, TagWriteError> {
    let mut report = TagWriteReport::default();
    let mut ready = Vec::new();
    for file in planned {
        match file.blocked {
            Some(problem) => report.failed.push(FailedFile {
                track_id: file.target.track_id,
                path: file.full_path,
                problem,
            }),
            None => ready.push(file),
        }
    }
    if ready.is_empty() {
        return Ok(report);
    }

    let forward: Vec<ForwardFile> = ready
        .iter()
        .map(|file| ForwardFile {
            track_id: file.target.track_id,
            asset_id: file.target.asset_id,
            path: file.target.path.clone(),
            fields: file
                .writes
                .iter()
                .map(|write| (write.field.name().to_owned(), write.to.clone()))
                .collect(),
            cover: file
                .target
                .artwork
                .as_ref()
                .map(|artwork| artwork.hash.clone()),
        })
        .collect();
    let mut inverse = Inverse::default();
    let operation_id = locked(database).begin_operation(
        OperationKind::WriteTags,
        &format!("Writing tags to {}", file_count(ready.len())),
        &serde_json::to_string(&forward)?,
        &serde_json::to_string(&inverse)?,
    )?;

    let mut file_ready = Vec::new();
    let mut library_ready = Vec::new();
    for file in ready {
        if file.writes.is_empty() && file.target.artwork.is_none() {
            library_ready.push(file);
        } else {
            file_ready.push(file);
        }
    }

    for chunk_files in library_ready.chunks(16) {
        let mut chunk_decisions = Vec::with_capacity(chunk_files.len());
        let mut meta_by_track: std::collections::HashMap<i64, (PathBuf, u32, u32)> =
            std::collections::HashMap::with_capacity(chunk_files.len());
        for file in chunk_files {
            let Some(decision) = file.decision.as_ref() else {
                continue;
            };
            let track_id = file.target.track_id;
            meta_by_track.insert(
                track_id,
                (
                    file.full_path.clone(),
                    decision.library_fields,
                    decision.reverted_fields,
                ),
            );
            chunk_decisions.push((track_id, decision.fields.clone()));
        }
        if chunk_decisions.is_empty() {
            continue;
        }
        let conflicted = locked(database).accept_library_decisions_chunk(
            operation_id,
            &chunk_decisions,
            |applied| {
                inverse.library_tracks.extend_from_slice(applied);
                Ok::<String, TagWriteError>(serde_json::to_string(&inverse)?)
            },
        )?;
        let conflicted_set: HashSet<i64> = conflicted.iter().copied().collect();
        for track_id in conflicted {
            if let Some((path, ..)) = meta_by_track.remove(&track_id) {
                report.failed.push(FailedFile {
                    track_id,
                    path,
                    problem: FileProblem::DecisionChanged,
                });
            }
        }
        for (track_id, (_, lib_fields, rev_fields)) in meta_by_track {
            if !conflicted_set.contains(&track_id) {
                report.operation_id = Some(operation_id);
                report.library_fields += lib_fields;
                report.reverted_fields += rev_fields;
            }
        }
    }

    for file in file_ready {
        let decision =
            file.decision
                .as_ref()
                .map_or(FileDecision::Write, |d| FileDecision::Accept {
                    fields: &d.fields,
                    cover: file
                        .target
                        .artwork
                        .as_ref()
                        .map(|a| (a.hash.as_str(), d.previous_cover.as_deref())),
                });
        let (library_fields, reverted_fields) = file
            .decision
            .as_ref()
            .map_or((0, 0), |d| (d.library_fields, d.reverted_fields));
        let picture = match (&store, &file.target.artwork) {
            (Some(store), Some(artwork)) => match store.read(artwork) {
                Ok(bytes) => Some((bytes, artwork.kind)),
                Err(error) => {
                    report.failed.push(FailedFile {
                        track_id: file.target.track_id,
                        path: file.full_path,
                        problem: FileProblem::Io(error),
                    });
                    continue;
                }
            },
            (None, Some(_)) => {
                report.failed.push(FailedFile {
                    track_id: file.target.track_id,
                    path: file.full_path,
                    problem: FileProblem::Io(io::Error::other("the artwork store is unavailable")),
                });
                continue;
            }
            _ => None,
        };
        let cover = match &picture {
            Some((bytes, kind)) => CoverEdit::Set(bytes, *kind),
            None => CoverEdit::Keep,
        };
        let done = match rewrite(
            &file.full_path,
            file.target.format,
            &TagEdit::Write(&file.writes),
            cover,
            true,
            file.target.stamp,
        ) {
            Ok(done) => done,
            Err(problem) => {
                report.failed.push(FailedFile {
                    track_id: file.target.track_id,
                    path: file.full_path,
                    problem,
                });
                continue;
            }
        };
        if file.decision.is_some()
            && file.writes.iter().any(|w| {
                let actual = done
                    .tags
                    .iter()
                    .find(|(f, _)| *f == w.field)
                    .map(|(_, v)| v.as_str());
                !written_value_matches(file.target.format, w.field, w.to.as_deref(), actual)
            })
        {
            report.failed.push(FailedFile {
                track_id: file.target.track_id,
                path: file.full_path,
                problem: FileProblem::UnsupportedTag,
            });
            continue;
        }
        let cover_before = match done.prior.cover {
            Some(Some(bytes)) => match stored_cover(store.as_ref(), Some(&bytes)) {
                Some(art) => Some(CoverBefore {
                    hash: Some(art.hash),
                }),
                None => {
                    report.failed.push(FailedFile {
                        track_id: file.target.track_id,
                        path: file.full_path,
                        problem: picture_problem("the previous cover could not be kept for undo"),
                    });
                    continue;
                }
            },
            Some(None) => Some(CoverBefore { hash: None }),
            None => None,
        };
        inverse.files.push(InverseFile {
            asset_id: file.target.asset_id,
            id3v23: done.was_v23,
            items: done.prior.items,
            cover: cover_before,
            size_bytes: done.stamp.size_bytes,
            modified_ns: done.stamp.modified_ns,
        });
        let front_cover = stored_cover(store.as_ref(), done.front_cover.as_deref());
        let mut library = locked(database);
        let revision = FileRevision {
            asset_id: file.target.asset_id,
            stamp: done.stamp,
            byte_hash: &done.byte_hash,
            tags: &done.tags,
            artwork: front_cover.as_ref(),
        };
        if let Err(error) = library.finish_tag_decision(
            file.target.track_id,
            Some(revision),
            operation_id,
            &serde_json::to_string(&inverse)?,
            decision,
            || done.copy.replace(&file.full_path, file.target.stamp),
        ) {
            inverse.files.pop();
            let problem = match error {
                DbError::DecisionChanged => FileProblem::DecisionChanged,
                DbError::FileEdit(e) => FileProblem::Io(e),
                other => return Err(other.into()),
            };
            report.failed.push(FailedFile {
                track_id: file.target.track_id,
                path: file.full_path,
                problem,
            });
            continue;
        }
        report.operation_id = Some(operation_id);
        report.library_fields += library_fields;
        report.reverted_fields += reverted_fields;
        report.written += 1;
    }

    let (state, summary) = match report.written {
        0 if report.library_fields > 0 => (
            OperationState::Applied,
            if report.library_fields == 1 {
                "Saved 1 library field".to_owned()
            } else {
                format!("Saved {} library fields", report.library_fields)
            },
        ),
        0 if report.reverted_fields > 0 => (
            OperationState::Applied,
            if report.reverted_fields == 1 {
                "Reverted 1 field to file tag".to_owned()
            } else {
                format!("Reverted {} fields to file tags", report.reverted_fields)
            },
        ),
        0 => (
            OperationState::Failed,
            format!(
                "No tags were written ({} failed)",
                file_count(report.failed.len())
            ),
        ),
        written => (
            OperationState::Applied,
            format!("Wrote tags to {}", file_count(written as usize)),
        ),
    };
    locked(database).finish_operation(operation_id, state, &summary)?;
    Ok(report)
}

/// Puts back the tags an applied write replaced, in every file it wrote. Nothing is changed
/// unless every file is still as the write left it.
pub fn undo_operation(
    database: &Mutex<Database>,
    operation_id: i64,
) -> Result<UndoReport, TagWriteError> {
    let _operation = crate::file_operation();
    let (operation, files, mut inverse, store) = {
        let library = locked(database);
        let operation = library
            .operation(operation_id)?
            .ok_or(TagWriteError::NoOperation(operation_id))?;
        if !operation.state.can_undo() {
            return Err(TagWriteError::CannotUndo {
                id: operation_id,
                state: operation.state,
            });
        }
        let inverse: Inverse = serde_json::from_str(&operation.inverse_json)?;
        for &track in inverse
            .library_tracks
            .iter()
            .skip(inverse.restored_library_tracks)
        {
            library.check_decision_undo(operation_id, track)?;
        }
        let mut files = Vec::with_capacity(inverse.files.len());
        for file in inverse.files.iter().rev().skip(inverse.restored_files) {
            let target = library
                .tag_write_target_of_asset(file.asset_id)?
                .ok_or(TagWriteError::NoOperation(operation_id))?;
            let path = full_asset_path(&target.source_folder, &target.path);
            let after = FileStamp {
                size_bytes: file.size_bytes,
                modified_ns: file.modified_ns,
            };
            if current_stamp(&path).ok() != Some(after) {
                return Err(TagWriteError::ChangedSinceWrite(path));
            }
            library.check_decision_undo(operation_id, target.track_id)?;
            files.push((file.clone(), target.track_id, target.format, path, after));
        }
        (operation, files, inverse, library.artwork_store())
    };

    let mut restored = 0;
    for (file, track_id, format, path, after) in files {
        let failed = |problem| TagWriteError::File {
            path: path.clone(),
            problem,
        };
        let picture = match (&file.cover, &store) {
            (Some(CoverBefore { hash: Some(hash) }), Some(store)) => {
                let (stored, kind) = store
                    .find(hash)
                    .ok_or_else(|| failed(FileProblem::Io(io::ErrorKind::NotFound.into())))?;
                Some((
                    fs::read(stored).map_err(|error| failed(FileProblem::Io(error)))?,
                    kind,
                ))
            }
            _ => None,
        };
        let cover = match (&file.cover, &picture) {
            (None, _) => CoverEdit::Keep,
            (Some(_), Some((bytes, kind))) => CoverEdit::Set(bytes, *kind),
            (Some(_), None) => CoverEdit::Remove,
        };
        let done = rewrite(
            &path,
            format,
            &TagEdit::Restore(&file.items),
            cover,
            file.id3v23,
            after,
        )
        .map_err(failed)?;
        let front_cover = stored_cover(store.as_ref(), done.front_cover.as_deref());
        inverse.restored_files += 1;
        let revision = FileRevision {
            asset_id: file.asset_id,
            stamp: done.stamp,
            byte_hash: &done.byte_hash,
            tags: &done.tags,
            artwork: front_cover.as_ref(),
        };
        locked(database).finish_tag_decision(
            track_id,
            Some(revision),
            operation_id,
            &serde_json::to_string(&inverse)?,
            FileDecision::Undo,
            || done.copy.replace(&path, after),
        )?;
        restored += 1;
    }

    while inverse.restored_library_tracks < inverse.library_tracks.len() {
        let end = (inverse.restored_library_tracks + 16).min(inverse.library_tracks.len());
        let chunk: Vec<i64> = inverse.library_tracks[inverse.restored_library_tracks..end].to_vec();
        inverse.restored_library_tracks = end;
        let inverse_json = serde_json::to_string(&inverse)?;
        locked(database).undo_library_decisions_chunk(operation_id, &chunk, &inverse_json)?;
    }
    let mut library = locked(database);
    library.finish_operation(operation_id, OperationState::Undone, &operation.summary)?;
    let operation = library
        .operation(operation_id)?
        .ok_or(TagWriteError::NoOperation(operation_id))?;
    Ok(UndoReport {
        operation,
        restored,
    })
}

/// Brings a tag write the app stopped in the middle of to rest. Each file's swap is recorded in
/// the same transaction that records its new tags, so the journal knows which files changed:
/// the write stands for those, a working copy left beside any file is removed, and the rest are
/// as they were.
pub(crate) fn recover(
    database: &mut Database,
    operation: &StoredOperation,
) -> Result<(OperationState, String), JournalError> {
    let forward: Vec<ForwardFile> = serde_json::from_str(&operation.forward_json)?;
    let inverse: Inverse = serde_json::from_str(&operation.inverse_json)?;
    let mut removed = 0;
    for file in &forward {
        let Some(asset) = database.asset_file(file.asset_id)? else {
            continue;
        };
        let original = full_asset_path(&asset.source_folder, &file.path);
        let copy = WorkingCopy::path_of(&original);
        if fs::symlink_metadata(&copy).is_ok() {
            fs::remove_file(&copy).map_err(|source| JournalError::File {
                path: copy.clone(),
                source,
            })?;
            removed += 1;
        }
    }
    let written = inverse.files.len() + inverse.library_tracks.len();
    let state = if written == 0 {
        OperationState::Failed
    } else {
        OperationState::Applied
    };
    Ok((
        state,
        format!(
            "The app stopped part way. {} kept and {} working {} removed.",
            file_count(written),
            removed,
            if removed == 1 { "copy" } else { "copies" }
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_integer_bpm_frame_gets_whole_beats() {
        assert_eq!(value_for(ItemKey::IntegerBpm, "127.6"), "128");
        assert_eq!(value_for(ItemKey::Bpm, "127.6"), "127.6");
        assert_eq!(value_for(ItemKey::TrackTitle, "Night"), "Night");
    }

    #[test]
    fn a_field_is_written_once_per_frame() {
        // ID3v2 keeps both the label and the publisher in TPUB.
        assert_eq!(keys_in(TagField::Label, TagType::Id3v2).len(), 1);
        assert!(keys_in(TagField::Mix, TagType::Id3v2).is_empty());
        assert!(!keys_in(TagField::Year, TagType::VorbisComments).is_empty());
    }
}
