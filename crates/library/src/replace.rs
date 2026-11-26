//! Replacing a track's file with a better copy of the same recording. The compare measures
//! both files and lines them up; Apply is one journaled operation that places the new file
//! beside the old one, carries tags and cues across, keeps the track and its collections, and
//! moves the old file to the bin. Undo puts all of it back.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use resonance_core::duplicates::{CopyFacts, Encoding, PairKind, classify_asset_pair};
use resonance_core::identity::{
    AssetAlignment, AssetIdentity, MeasuredAlignment, RELINK_MAX_FINGERPRINT_SHIFT,
    fingerprint_similarity,
};
use resonance_core::journal::OperationKind;
use resonance_core::library::{AudioFormat, TagField, title_from_tags};
use resonance_core::quality::{QualityFlag, QualityMeasurements};
use resonance_core::replace::{ReplaceChoice, ReplaceVerdict, merge_tags, replace_verdict};
use resonance_core::template::{Rendered, Template, file_name, name_key, sanitise};
use resonance_db::{Database, DbError, StoredOperation};
use serde::Serialize;

use crate::align::{AlignError, measure_alignment};
use crate::file_tags;
use crate::identity::{compute_identity, compute_identity_for_compare, full_asset_path};
use crate::journal::{self, JournalError, Step, byte_hash_hex};
use crate::scan::{scan_single_file, stored_path};
use crate::write_tags;

#[derive(Debug)]
pub enum ReplaceError {
    Database(DbError),
    TrackNotFound(i64),
    /// The track's file is not where the library last saw it.
    OldFileMissing(PathBuf),
    NotAudio(PathBuf),
    /// The new file is the track's own file.
    SameFile,
    File {
        path: PathBuf,
        source: io::Error,
    },
    Align(AlignError),
    Journal(JournalError),
}

impl fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(source) => source.fmt(f),
            Self::TrackNotFound(id) => write!(f, "There is no track {id} in the library."),
            Self::OldFileMissing(path) => write!(
                f,
                "{} is missing. Relink the track before replacing its file.",
                path.display()
            ),
            Self::NotAudio(path) => write!(
                f,
                "{} is not an MP3, FLAC, WAV, AIFF or M4A file.",
                path.display()
            ),
            Self::SameFile => f.write_str("That is the track's own file."),
            Self::File { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Align(source) => source.fmt(f),
            Self::Journal(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for ReplaceError {}

impl From<DbError> for ReplaceError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

impl From<JournalError> for ReplaceError {
    fn from(source: JournalError) -> Self {
        Self::Journal(source)
    }
}

/// What the compare sheet shows about one file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSummary {
    pub path: PathBuf,
    pub format: AudioFormat,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub duration_ms: Option<u64>,
    pub measurements: QualityMeasurements,
    pub flags: Vec<QualityFlag>,
}

impl FileSummary {
    fn encoding(&self) -> Encoding {
        Encoding::of(
            self.format,
            self.bitrate_kbps,
            self.sample_rate,
            self.bit_depth,
            !self.flags.is_empty(),
        )
    }
}

/// The track's file as the library holds it.
#[derive(Debug, Clone)]
pub struct CurrentFile {
    pub track_id: i64,
    pub asset_id: i64,
    pub source_folder: String,
    /// Relative to the source folder.
    pub path: String,
    pub size_bytes: u64,
    pub alignment: AssetAlignment,
    /// What the library shows: the user's values over the file's.
    pub shown_tags: Vec<(TagField, String)>,
    pub user_tags: Vec<(TagField, String)>,
    pub file_tags: Vec<(TagField, String)>,
    pub unchecked_cue_ids: Vec<i64>,
    pub unwritten_fields: Vec<String>,
}

impl CurrentFile {
    pub fn full_path(&self) -> PathBuf {
        full_asset_path(&self.source_folder, &self.path)
    }

    fn folder(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(folder, _)| folder)
    }
}

/// Reads what replacing `track_id`'s file needs from the library. Quick, so it can run under
/// the library's lock; the decoding happens in [`compare`] without it.
pub fn current_file(database: &Database, track_id: i64) -> Result<CurrentFile, ReplaceError> {
    let asset_id = *database
        .included_assets_of_tracks(&[track_id])?
        .first()
        .ok_or(ReplaceError::TrackNotFound(track_id))?;
    let file = database
        .asset_file(asset_id)?
        .ok_or(ReplaceError::TrackNotFound(track_id))?;
    let alignment = database
        .track_identity(track_id)?
        .map_or(AssetAlignment::IDENTITY, |stored| stored.alignment());
    let layers = database
        .tag_layers(track_id)?
        .ok_or(ReplaceError::TrackNotFound(track_id))?;
    let shown_tags = merge_tags(&layers.user, &layers.file, &[]);
    let unchecked_cue_ids = database
        .cues(track_id)?
        .into_iter()
        .filter(|cue| !cue.carried_over_check)
        .map(|cue| cue.id)
        .collect();
    Ok(CurrentFile {
        track_id,
        asset_id,
        source_folder: file.source_folder,
        path: file.path,
        size_bytes: file.size_bytes,
        alignment,
        shown_tags,
        user_tags: layers.user,
        file_tags: layers.file,
        unchecked_cue_ids,
        unwritten_fields: database.unwritten_user_fields(track_id)?,
    })
}

/// Everything the compare sheet shows, and what Apply needs to act on it.
#[derive(Debug, Clone)]
pub struct ReplaceComparison {
    pub current: CurrentFile,
    pub old: FileSummary,
    pub new: FileSummary,
    pub new_tags: Vec<(TagField, String)>,
    pub kind: PairKind,
    pub fingerprint_similarity: Option<f32>,
    pub alignment: Option<MeasuredAlignment>,
    pub verdict: ReplaceVerdict,
    /// The name the new file gets beside the old one when it replaces it.
    pub replace_name: String,
    /// The name it gets when it is added as a track of its own.
    pub separate_name: String,
    pub cues_to_carry: usize,
}

impl ReplaceComparison {
    /// Milliseconds from the old file's start to where its audio starts in the new file.
    pub fn offset_ms(&self) -> Option<f64> {
        self.alignment.map(|a| a.new_at_old_zero_s * 1000.0)
    }
}

fn summary(path: &Path, identity: &AssetIdentity) -> Result<FileSummary, ReplaceError> {
    let format = AudioFormat::of(path).ok_or_else(|| ReplaceError::NotAudio(path.to_owned()))?;
    let audio = file_tags::read(path).ok().map(|contents| contents.audio);
    let facts = identity.decoded_facts;
    let size_bytes = fs::metadata(path).map(|metadata| metadata.len()).ok();
    let quality = identity.quality.clone();
    Ok(FileSummary {
        path: path.to_owned(),
        format,
        bitrate_kbps: audio.and_then(|audio| audio.bitrate_kbps).or_else(|| {
            facts
                .zip(size_bytes)
                .and_then(|(facts, size)| facts.effective_bitrate_kbps(size))
        }),
        sample_rate: facts
            .map(|facts| facts.sample_rate)
            .or_else(|| audio.and_then(|audio| audio.sample_rate)),
        bit_depth: audio.and_then(|audio| audio.bit_depth),
        duration_ms: facts
            .map(|facts| facts.duration_ms())
            .or_else(|| audio.map(|audio| audio.duration_ms)),
        measurements: quality
            .as_ref()
            .map(|quality| quality.measurements)
            .unwrap_or_default(),
        flags: quality.map(|quality| quality.flags).unwrap_or_default(),
    })
}

fn copy_facts<'a>(
    identity: &'a AssetIdentity,
    summary: &FileSummary,
    artist: Option<&'a str>,
    title: &'a str,
) -> CopyFacts<'a> {
    CopyFacts {
        byte_hash: Some(identity.byte_hash),
        audio_hash: identity.audio.as_ref().map(|audio| audio.hash),
        fingerprint: identity.chromaprint.as_ref().map(|fp| fp.raw.as_slice()),
        duration_ms: summary.duration_ms,
        artist,
        title: Some(title),
        encoding: summary.encoding(),
    }
}

fn value_of(tags: &[(TagField, String)], field: TagField) -> Option<&str> {
    tags.iter()
        .find(|(taken, _)| *taken == field)
        .map(|(_, value)| value.as_str())
}

fn extension_of(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::Aiff => "aiff",
        other => other.name(),
    }
}

/// A name for `stem` with `extension` in `folder` that no other file has. The file being
/// replaced, `leaving`, gives its name up.
fn free_name(folder: &Path, stem: &str, extension: &str, leaving: Option<&str>) -> String {
    let taken: Vec<String> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| name_key(&entry.file_name().to_string_lossy()))
                .collect()
        })
        .unwrap_or_default();
    let leaving = leaving.map(name_key);
    let free = |name: &str| {
        let key = name_key(name);
        leaving.as_deref() == Some(key.as_str()) || !taken.contains(&key)
    };
    let first = file_name(stem, extension, 240);
    if free(&first) {
        return first;
    }
    (2..)
        .map(|count| file_name(&format!("{stem} ({count})"), extension, 240))
        .find(|name| free(name))
        .unwrap_or(first)
}

fn stem_of(name: &str) -> &str {
    name.rsplit_once('.')
        .map_or(name, |(stem, _)| if stem.is_empty() { name } else { stem })
}

/// The stem `template` gives `tags`, or `fallback` when the template is missing or a tag it
/// needs is empty.
fn templated_stem(
    template: Option<&Template>,
    tags: &[(TagField, String)],
    fallback: &str,
) -> String {
    match template.map(|template| template.render(|field| value_of(tags, field))) {
        Some(Rendered::Stem(stem)) if !stem.trim().is_empty() => stem,
        _ => sanitise(fallback),
    }
}

/// Measures `new_path` against the track's file and decides what Apply would do. Decodes both
/// files, so it runs without the library's lock.
pub fn compare(
    current: CurrentFile,
    new_path: &Path,
    template: Option<&Template>,
) -> Result<ReplaceComparison, ReplaceError> {
    let old_path = current.full_path();
    if !old_path.is_file() {
        return Err(ReplaceError::OldFileMissing(old_path));
    }
    let new_format =
        AudioFormat::of(new_path).ok_or_else(|| ReplaceError::NotAudio(new_path.to_owned()))?;
    let file_error = |path: &Path| {
        let path = path.to_owned();
        move |source| ReplaceError::File { path, source }
    };
    if dunce::canonicalize(&old_path).ok() == dunce::canonicalize(new_path).ok() {
        return Err(ReplaceError::SameFile);
    }
    let old_identity = compute_identity_for_compare(&old_path).map_err(file_error(&old_path))?;
    let new_identity = compute_identity_for_compare(new_path).map_err(file_error(new_path))?;
    let old = summary(&old_path, &old_identity)?;
    let new = summary(new_path, &new_identity)?;
    let new_tags = file_tags::read(new_path)
        .map(|contents| contents.tags)
        .unwrap_or_default();

    let old_fingerprint = old_identity
        .chromaprint
        .as_ref()
        .map(|fp| fp.raw.as_slice());
    let new_fingerprint = new_identity
        .chromaprint
        .as_ref()
        .map(|fp| fp.raw.as_slice());
    let old_title = title_from_tags(&current.path, &current.shown_tags).to_owned();
    let new_name = new_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let new_title = title_from_tags(&new_name, &new_tags).to_owned();
    let kind = classify_asset_pair(
        &copy_facts(
            &old_identity,
            &old,
            value_of(&current.shown_tags, TagField::Artist),
            &old_title,
        ),
        &copy_facts(
            &new_identity,
            &new,
            value_of(&new_tags, TagField::Artist),
            &new_title,
        ),
    );
    let similarity = old_fingerprint
        .zip(new_fingerprint)
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .map(|(a, b)| fingerprint_similarity(a, b, RELINK_MAX_FINGERPRINT_SHIFT));

    let alignment = if kind.is_copy() || kind.is_version() {
        match measure_alignment(&old_path, new_path) {
            Ok(measured) => Some(measured),
            Err(AlignError::NothingToMeasure) => None,
            Err(error) => return Err(ReplaceError::Align(error)),
        }
    } else {
        None
    };
    let verdict = replace_verdict(kind, alignment.as_ref(), &old.encoding(), &new.encoding());

    let folder = old_path.parent().unwrap_or(Path::new("")).to_owned();
    let old_name = old_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let merged = merge_tags(&current.user_tags, &current.file_tags, &new_tags);
    let extension = extension_of(new_format);
    let replace_stem = templated_stem(template, &merged, stem_of(&old_name));
    let replace_name = free_name(&folder, &replace_stem, extension, Some(&old_name));
    let separate_stem = templated_stem(template, &new_tags, stem_of(&new_name));
    let separate_name = free_name(&folder, &separate_stem, extension, None);
    let cues_to_carry = current.unchecked_cue_ids.len();
    Ok(ReplaceComparison {
        current,
        old,
        new,
        new_tags,
        kind,
        fingerprint_similarity: similarity,
        alignment,
        verdict,
        replace_name,
        separate_name,
        cues_to_carry,
    })
}

#[derive(Debug, Serialize)]
struct ReplaceForward<'a> {
    track_id: i64,
    choice: &'a str,
    old_path: &'a str,
    new_path: String,
    placed_path: &'a str,
}

/// What Apply did.
#[derive(Debug)]
pub struct ReplaceReport {
    pub operation: StoredOperation,
    /// The track the file now belongs to: the same track for a replace, a new one when added
    /// beside.
    pub track_id: i64,
    pub placed_path: PathBuf,
}

fn staged_path(final_path: &Path) -> PathBuf {
    let mut name = final_path.file_name().unwrap_or_default().to_os_string();
    name.push(".resonance-incoming");
    final_path.with_file_name(name)
}

/// Runs the choice the user made on the compare sheet as one journaled operation.
pub fn apply(
    database: &mut Database,
    comparison: &ReplaceComparison,
    choice: ReplaceChoice,
) -> Result<ReplaceReport, ReplaceError> {
    let current = &comparison.current;
    let old_path = current.full_path();
    let new_path = &comparison.new.path;
    let folder = old_path.parent().unwrap_or(Path::new("")).to_owned();
    let name = match choice {
        ReplaceChoice::Replace => &comparison.replace_name,
        ReplaceChoice::AddSeparateTrack => &comparison.separate_name,
    };
    let final_path = folder.join(name);
    let staged = staged_path(&final_path);
    let relative = match current.folder() {
        "" => name.clone(),
        folder => format!("{folder}/{name}"),
    };
    let byte_hash = byte_hash_hex(new_path).map_err(|source| ReplaceError::File {
        path: new_path.clone(),
        source,
    })?;
    let mut steps = vec![Step::CopyFile {
        from: new_path.clone(),
        to: staged.clone(),
        byte_hash,
    }];
    let (kind, summary) = match choice {
        ReplaceChoice::Replace => {
            let merged = merge_tags(&current.user_tags, &current.file_tags, &comparison.new_tags);
            let written_fields: Vec<String> = current
                .unwritten_fields
                .iter()
                .filter(|field| {
                    TagField::from_name(field)
                        .is_some_and(|field| merged.iter().any(|(taken, _)| *taken == field))
                })
                .cloned()
                .collect();
            let alignment = comparison
                .alignment
                .zip(comparison.old.sample_rate.zip(comparison.new.sample_rate))
                .and_then(|(measured, (old_rate, new_rate))| {
                    current.alignment.followed_by(&measured, old_rate, new_rate)
                })
                .unwrap_or(current.alignment);
            steps.extend([
                Step::TagFile {
                    path: staged.clone(),
                    tags: merged
                        .iter()
                        .map(|(field, value)| (field.name().to_owned(), value.clone()))
                        .collect(),
                },
                Step::HideAsset {
                    asset_id: current.asset_id,
                },
                Step::TrashFile {
                    path: old_path.clone(),
                    size_bytes: current.size_bytes,
                },
                Step::MoveFile {
                    from: staged.clone(),
                    to: final_path.clone(),
                },
                Step::AttachReplacement {
                    track_id: current.track_id,
                    old_asset_id: current.asset_id,
                    path: relative.clone(),
                    offset_samples: alignment.offset_samples(),
                    tempo_ratio: alignment.tempo_ratio(),
                    cue_ids: current.unchecked_cue_ids.clone(),
                    written_fields,
                },
            ]);
            (OperationKind::ReplaceFile, "Replaced 1 file".to_owned())
        }
        ReplaceChoice::AddSeparateTrack => {
            steps.extend([
                Step::MoveFile {
                    from: staged.clone(),
                    to: final_path.clone(),
                },
                Step::AddTrack {
                    beside_asset_id: current.asset_id,
                    path: relative.clone(),
                },
            ]);
            (OperationKind::AddFile, "Added 1 file".to_owned())
        }
    };
    let forward = serde_json::to_string(&ReplaceForward {
        track_id: current.track_id,
        choice: match choice {
            ReplaceChoice::Replace => "replace",
            ReplaceChoice::AddSeparateTrack => "add_separate_track",
        },
        old_path: &current.path,
        new_path: new_path.to_string_lossy().into_owned(),
        placed_path: &relative,
    })
    .map_err(JournalError::Plan)?;
    let operation = journal::apply(database, kind, &summary, &forward, &steps)?;
    let track_id = match choice {
        ReplaceChoice::Replace => current.track_id,
        ReplaceChoice::AddSeparateTrack => database
            .track_id_of_asset_path(current.asset_id, &relative)?
            .unwrap_or(current.track_id),
    };
    Ok(ReplaceReport {
        operation,
        track_id,
        placed_path: final_path,
    })
}

fn file_error(path: &Path) -> impl FnOnce(io::Error) -> JournalError + '_ {
    move |source| JournalError::File {
        path: path.to_owned(),
        source,
    }
}

/// Writes the merged tags into the staged copy of the new file.
pub(crate) fn tag_staged_copy(path: &Path, tags: &[(String, String)]) -> Result<(), JournalError> {
    let format = AudioFormat::of(staged_original(path)).ok_or_else(|| JournalError::File {
        path: path.to_owned(),
        source: io::Error::other("not an audio file"),
    })?;
    let tags: Vec<(TagField, String)> = tags
        .iter()
        .filter_map(|(field, value)| Some((TagField::from_name(field)?, value.clone())))
        .collect();
    write_tags::write_into_copy(path, format, &tags).map_err(|problem| JournalError::File {
        path: path.to_owned(),
        source: io::Error::other(problem.to_string()),
    })
}

/// The name a staged copy will take, which tells its format.
fn staged_original(path: &Path) -> &Path {
    let text = path.as_os_str().to_str().unwrap_or_default();
    Path::new(text.strip_suffix(".resonance-incoming").unwrap_or(text))
}

fn placed_file(
    database: &Database,
    beside_asset_id: i64,
    relative: &str,
) -> Result<(PathBuf, resonance_core::library::ScannedFile, AssetIdentity), JournalError> {
    let beside = database
        .asset_file(beside_asset_id)?
        .ok_or(JournalError::NoOperation(beside_asset_id))?;
    let full = full_asset_path(&beside.source_folder, relative);
    let root = full_asset_path(&beside.source_folder, "");
    let stored = stored_path(&root, &full).unwrap_or_else(|| relative.to_owned());
    let scanned = scan_single_file(&full, stored).map_err(file_error(&full))?;
    let identity = compute_identity(&full).map_err(file_error(&full))?;
    Ok((full, scanned, identity))
}

/// Puts the placed file in the track's place in the library.
#[allow(clippy::too_many_arguments)]
pub(crate) fn attach(
    database: &mut Database,
    operation_id: i64,
    position: u32,
    track_id: i64,
    old_asset_id: i64,
    relative: &str,
    alignment: AssetAlignment,
    cue_ids: &[i64],
    written_fields: &[String],
) -> Result<(), JournalError> {
    let (_, scanned, identity) = placed_file(database, old_asset_id, relative)?;
    database.attach_replacement(
        operation_id,
        position,
        &resonance_db::Replacement {
            track_id,
            old_asset_id,
            file: &scanned,
            identity: &identity,
            alignment,
            cue_ids,
            written_fields,
        },
    )?;
    Ok(())
}

/// Adds the placed file as a track of its own.
pub(crate) fn add_beside(
    database: &mut Database,
    operation_id: i64,
    position: u32,
    beside_asset_id: i64,
    relative: &str,
) -> Result<(), JournalError> {
    let (_, scanned, identity) = placed_file(database, beside_asset_id, relative)?;
    database.add_track_beside(operation_id, position, beside_asset_id, &scanned, &identity)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_staged_copy_keeps_the_extension_it_will_have() {
        let staged = staged_path(Path::new("music/Night drive.flac"));
        assert_eq!(
            staged,
            Path::new("music/Night drive.flac.resonance-incoming")
        );
        assert_eq!(
            staged_original(&staged),
            Path::new("music/Night drive.flac")
        );
    }

    #[test]
    fn a_stem_without_a_template_is_the_old_name() {
        assert_eq!(templated_stem(None, &[], "Night drive"), "Night drive");
        let template = Template::parse("{artist} - {title}").expect("template");
        let tags = vec![
            (TagField::Artist, "Somebody".to_owned()),
            (TagField::Title, "Night drive".to_owned()),
        ];
        assert_eq!(
            templated_stem(Some(&template), &tags, "x"),
            "Somebody - Night drive"
        );
        assert_eq!(templated_stem(Some(&template), &[], "fallback"), "fallback");
    }
}
