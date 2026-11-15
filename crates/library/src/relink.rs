use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use resonance_core::identity::{
    AssetIdentity, RelinkIdentityRef, RelinkMatchKind, compare_relink_matches,
    evaluate_relink_match,
};
use resonance_core::library::{
    AudioFormat, ScannedFile, is_asset_path_included, is_ignored_folder_with,
};
use resonance_db::{Database, DbError, SourceConfig, SourceId, StoredTrackRelinkTarget};

use crate::identity::{compute_identity, full_asset_path};
use crate::scan::{has_scan_ignore_marker, is_hidden, scan_single_file, stored_path};

const MAX_ON_THE_FLY_DB_CANDIDATES: usize = 16;
const MAX_FOLDER_SEARCH_FILES: usize = 512;

#[derive(Debug)]
pub enum RelinkError {
    TrackNotFound { track_id: i64 },
    CandidateFile { path: PathBuf, source: io::Error },
    SearchFolder { path: PathBuf, source: io::Error },
    UnsupportedFormat { path: PathBuf },
    ConfirmationRequired { similarity: Option<f32> },
    Database(DbError),
}

impl fmt::Display for RelinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackNotFound { track_id } => {
                write!(f, "Track #{track_id} is no longer in the library.")
            }
            Self::CandidateFile { path, source } => {
                write!(
                    f,
                    "The candidate file {} could not be read: {source}",
                    path.display()
                )
            }
            Self::SearchFolder { path, source } => {
                write!(
                    f,
                    "The folder {} could not be searched: {source}",
                    path.display()
                )
            }
            Self::UnsupportedFormat { path } => {
                write!(
                    f,
                    "{} is not a supported audio format (WAV, AIFF, FLAC, MP3 or M4A).",
                    path.display()
                )
            }
            Self::ConfirmationRequired {
                similarity: Some(score),
            } => {
                write!(
                    f,
                    "This file appears to be a different recording ({:.0}% Chromaprint agreement). Confirm before relinking.",
                    score * 100.0
                )
            }
            Self::ConfirmationRequired { similarity: None } => {
                write!(
                    f,
                    "Audio identity could not be matched automatically. Confirm before relinking."
                )
            }
            Self::Database(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for RelinkError {}

impl From<DbError> for RelinkError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelinkFileFacts {
    pub path: String,
    pub format: AudioFormat,
    pub size_bytes: u64,
    pub duration_ms: Option<u64>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelinkCandidate {
    pub candidate_path: String,
    pub asset_id: Option<i64>,
    pub existing_track_id: Option<i64>,
    pub verdict: RelinkMatchKind,
    pub file: RelinkFileFacts,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelinkPreview {
    pub track_id: i64,
    pub verdict: RelinkMatchKind,
    pub requires_confirmation: bool,
    pub current: RelinkFileFacts,
    pub candidate: RelinkFileFacts,
    pub merges_existing_track_id: Option<i64>,
}

/// Discovers ranked relink candidates across library sources (and optionally inside `search_folder`)
/// matching `track_id`'s stored identity by `byte_hash`, `audio_hash`, or `fingerprint_similarity >= 0.85`
/// within `±3.0 s` duration.
pub fn find_relink_candidates(
    database: &Database,
    track_id: i64,
    search_folder: Option<&Path>,
) -> Result<Vec<RelinkCandidate>, RelinkError> {
    let target = database
        .track_relink_target(track_id)?
        .ok_or(RelinkError::TrackNotFound { track_id })?;

    let missing_ref = target_identity_ref(&target);
    if missing_ref.byte_hash.is_none()
        && missing_ref.audio_hash.is_none()
        && missing_ref.chromaprint_raw.is_none()
    {
        return Ok(Vec::new());
    }

    let mut seen_paths: HashSet<PathBuf> = HashSet::new();
    let mut candidates: Vec<RelinkCandidate> = Vec::new();
    let mut computed_unidentified = 0usize;

    for row in database.candidate_assets_for_relink(&target)? {
        let full = full_asset_path(&row.source_folder, &row.path);
        let Ok(canonical) = fs::canonicalize(&full) else {
            continue;
        };
        if !canonical.is_file() || !seen_paths.insert(canonical.clone()) {
            continue;
        }
        let display_path = simplified_path_string(&canonical);

        let verdict =
            if row.byte_hash.is_some() || row.audio_hash.is_some() || row.chromaprint_raw.is_some()
            {
                evaluate_relink_match(
                    missing_ref,
                    RelinkIdentityRef {
                        byte_hash: row.byte_hash,
                        audio_hash: row.audio_hash,
                        chromaprint_raw: row.chromaprint_raw.as_deref(),
                        duration_ms: row.duration_ms,
                    },
                )
            } else if computed_unidentified < MAX_ON_THE_FLY_DB_CANDIDATES {
                computed_unidentified += 1;
                let Ok(computed) = compute_identity(&canonical) else {
                    continue;
                };
                evaluate_against_computed(missing_ref, &computed, row.duration_ms)
            } else {
                continue;
            };

        if verdict.is_auto_candidate() {
            candidates.push(RelinkCandidate {
                candidate_path: display_path.clone(),
                asset_id: Some(row.asset_id),
                existing_track_id: (row.track_id != track_id).then_some(row.track_id),
                verdict,
                file: RelinkFileFacts {
                    path: display_path,
                    format: row.format,
                    size_bytes: row.size_bytes,
                    duration_ms: row.duration_ms,
                    sample_rate: row.sample_rate,
                    bit_depth: row.bit_depth,
                    bitrate_kbps: row.bitrate_kbps,
                },
            });
        }
    }

    if let Some(folder) = search_folder {
        let root = fs::canonicalize(folder).map_err(|source| RelinkError::SearchFolder {
            path: folder.to_owned(),
            source,
        })?;
        let ignored = database.ignored_folders()?;
        let mut files = Vec::new();
        collect_folder_audio_files(&root, &ignored, &mut files);
        files.sort();

        for path in files {
            let Ok(canonical) = fs::canonicalize(&path) else {
                continue;
            };
            if !canonical.is_file() || !seen_paths.insert(canonical.clone()) {
                continue;
            }
            let display_path = simplified_path_string(&canonical);
            let Ok(scanned) = scan_single_file(&canonical, display_path.clone()) else {
                continue;
            };
            let Ok(computed) = compute_identity(&canonical) else {
                continue;
            };
            let file_facts = facts_from_scanned(&display_path, &scanned, &computed);
            let verdict = evaluate_against_computed(missing_ref, &computed, file_facts.duration_ms);
            if verdict.is_auto_candidate() {
                candidates.push(RelinkCandidate {
                    candidate_path: display_path,
                    asset_id: None,
                    existing_track_id: None,
                    verdict,
                    file: file_facts,
                });
            }
        }
    }

    candidates.sort_by(|left, right| {
        compare_relink_matches(left.verdict, right.verdict)
            .then_with(|| left.candidate_path.cmp(&right.candidate_path))
    });
    Ok(candidates)
}

/// Computes identity on `candidate_path` and returns a before/after comparison against `track_id`
/// without modifying the database.
pub fn preview_relink(
    database: &Database,
    track_id: i64,
    candidate_path: &Path,
) -> Result<RelinkPreview, RelinkError> {
    let target = database
        .track_relink_target(track_id)?
        .ok_or(RelinkError::TrackNotFound { track_id })?;
    let (canonical, display_path) = resolve_candidate_file(candidate_path)?;
    let scanned = scan_single_file(&canonical, display_path.clone()).map_err(|source| {
        RelinkError::CandidateFile {
            path: candidate_path.to_owned(),
            source,
        }
    })?;
    let computed = compute_identity(&canonical).map_err(|source| RelinkError::CandidateFile {
        path: candidate_path.to_owned(),
        source,
    })?;
    let candidate_facts = facts_from_scanned(&display_path, &scanned, &computed);
    let verdict = evaluate_against_computed(
        target_identity_ref(&target),
        &computed,
        candidate_facts.duration_ms,
    );

    let sources = database.sources()?;
    let ignored = database.ignored_folders()?;
    let merges_existing_track_id = find_matching_included_source(&canonical, &sources, &ignored)
        .and_then(|(source_id, rel_path)| {
            existing_other_track_at_path(database, source_id, &rel_path, track_id)
        });

    Ok(RelinkPreview {
        track_id,
        verdict,
        requires_confirmation: verdict.requires_confirmation(),
        current: target_file_facts(&target),
        candidate: candidate_facts,
        merges_existing_track_id,
    })
}

/// Verifies `candidate_path` and repoints `track_id`'s asset to it, merging any duplicate scanned
/// track row and clearing `missing_since`. Refuses `DifferentRecording` unless `force` is `true`.
pub fn apply_relink(
    database: &mut Database,
    track_id: i64,
    candidate_path: &Path,
    force: bool,
) -> Result<RelinkPreview, RelinkError> {
    let target = database
        .track_relink_target(track_id)?
        .ok_or(RelinkError::TrackNotFound { track_id })?;
    let (canonical, display_path) = resolve_candidate_file(candidate_path)?;
    let computed = compute_identity(&canonical).map_err(|source| RelinkError::CandidateFile {
        path: candidate_path.to_owned(),
        source,
    })?;

    let sources = database.sources()?;
    let ignored = database.ignored_folders()?;
    let (target_source, rel_path, merges_existing_track_id) = if let Some((source_id, rel)) =
        find_matching_included_source(&canonical, &sources, &ignored)
    {
        let merges = existing_other_track_at_path(database, source_id, &rel, track_id);
        (source_id, rel, merges)
    } else {
        let parent = canonical
            .parent()
            .ok_or_else(|| RelinkError::CandidateFile {
                path: candidate_path.to_owned(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "file has no parent directory"),
            })?;
        let parent_str = simplified_path_string(parent);
        let file_name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| RelinkError::CandidateFile {
                path: candidate_path.to_owned(),
                source: io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "file name is not valid Unicode",
                ),
            })?
            .to_owned();
        let source_id = database.source_for_unwatched_folder(&parent_str)?;
        let merges = existing_other_track_at_path(database, source_id, &file_name, track_id);
        (source_id, file_name, merges)
    };

    let scanned =
        scan_single_file(&canonical, rel_path).map_err(|source| RelinkError::CandidateFile {
            path: candidate_path.to_owned(),
            source,
        })?;
    let candidate_facts = facts_from_scanned(&display_path, &scanned, &computed);
    let verdict = evaluate_against_computed(
        target_identity_ref(&target),
        &computed,
        candidate_facts.duration_ms,
    );

    if verdict.requires_confirmation() && !force {
        let similarity = match verdict {
            RelinkMatchKind::DifferentRecording { similarity } => similarity,
            _ => None,
        };
        return Err(RelinkError::ConfirmationRequired { similarity });
    }

    if !database.apply_track_relink(track_id, target_source, &scanned, &computed)? {
        return Err(RelinkError::TrackNotFound { track_id });
    }

    Ok(RelinkPreview {
        track_id,
        verdict,
        requires_confirmation: verdict.requires_confirmation(),
        current: target_file_facts(&target),
        candidate: candidate_facts,
        merges_existing_track_id,
    })
}

fn target_identity_ref(target: &StoredTrackRelinkTarget) -> RelinkIdentityRef<'_> {
    RelinkIdentityRef {
        byte_hash: target.byte_hash,
        audio_hash: target.audio_hash,
        chromaprint_raw: target.chromaprint_raw.as_deref(),
        duration_ms: target.duration_ms,
    }
}

fn target_file_facts(target: &StoredTrackRelinkTarget) -> RelinkFileFacts {
    let full = full_asset_path(&target.source_folder, &target.path);
    RelinkFileFacts {
        path: simplified_path_string(&full),
        format: target.format,
        size_bytes: target.size_bytes,
        duration_ms: target.duration_ms,
        sample_rate: target.sample_rate,
        bit_depth: target.bit_depth,
        bitrate_kbps: target.bitrate_kbps,
    }
}

fn evaluate_against_computed(
    missing_ref: RelinkIdentityRef<'_>,
    computed: &AssetIdentity,
    fallback_duration_ms: Option<u64>,
) -> RelinkMatchKind {
    let duration_ms = computed
        .decoded_facts
        .map(|facts| facts.duration_ms())
        .or(fallback_duration_ms);
    evaluate_relink_match(
        missing_ref,
        RelinkIdentityRef {
            byte_hash: Some(computed.byte_hash),
            audio_hash: computed.audio.as_ref().map(|a| a.hash),
            chromaprint_raw: computed.chromaprint.as_ref().map(|fp| fp.raw.as_slice()),
            duration_ms,
        },
    )
}

fn facts_from_scanned(
    display_path: &str,
    scanned: &ScannedFile,
    computed: &AssetIdentity,
) -> RelinkFileFacts {
    let duration_ms = scanned
        .audio
        .map(|a| a.duration_ms)
        .or_else(|| computed.decoded_facts.map(|f| f.duration_ms()));
    let sample_rate = scanned
        .audio
        .and_then(|a| a.sample_rate)
        .or_else(|| computed.decoded_facts.map(|f| f.sample_rate));
    let bit_depth = scanned.audio.and_then(|a| a.bit_depth);
    let bitrate_kbps = scanned.audio.and_then(|a| a.bitrate_kbps).or_else(|| {
        computed
            .decoded_facts
            .and_then(|f| f.effective_bitrate_kbps(scanned.size_bytes))
    });
    RelinkFileFacts {
        path: display_path.to_owned(),
        format: scanned.format,
        size_bytes: scanned.size_bytes,
        duration_ms,
        sample_rate,
        bit_depth,
        bitrate_kbps,
    }
}

fn resolve_candidate_file(candidate_path: &Path) -> Result<(PathBuf, String), RelinkError> {
    if AudioFormat::of(candidate_path).is_none() {
        return Err(RelinkError::UnsupportedFormat {
            path: candidate_path.to_owned(),
        });
    }
    let canonical =
        fs::canonicalize(candidate_path).map_err(|source| RelinkError::CandidateFile {
            path: candidate_path.to_owned(),
            source,
        })?;
    if !canonical.is_file() {
        return Err(RelinkError::CandidateFile {
            path: candidate_path.to_owned(),
            source: io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"),
        });
    }
    let display = simplified_path_string(&canonical);
    Ok((canonical, display))
}

fn simplified_path_string(path: &Path) -> String {
    dunce::simplified(path).to_string_lossy().into_owned()
}

fn find_matching_included_source(
    canonical_file: &Path,
    sources: &[SourceConfig],
    ignored_folders: &[String],
) -> Option<(SourceId, String)> {
    let mut best: Option<(usize, SourceId, String)> = None;
    for source in sources {
        let Ok(source_canon) = fs::canonicalize(&source.folder) else {
            continue;
        };
        let Some(rel) = stored_path(&source_canon, canonical_file) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        if !is_asset_path_included(
            &rel,
            source.include_subfolders,
            &source.excluded_subfolders,
            ignored_folders,
        ) {
            continue;
        }
        let depth = source_canon.components().count();
        if best
            .as_ref()
            .is_none_or(|(best_depth, _, _)| depth > *best_depth)
        {
            best = Some((depth, source.id, rel));
        }
    }
    best.map(|(_, id, rel)| (id, rel))
}

fn existing_other_track_at_path(
    database: &Database,
    source_id: SourceId,
    rel_path: &str,
    track_id: i64,
) -> Option<i64> {
    database
        .track_id_at_source_path(source_id, rel_path)
        .ok()
        .flatten()
        .filter(|&existing_id| existing_id != track_id)
}

fn collect_folder_audio_files(root: &Path, ignored_folders: &[String], out: &mut Vec<PathBuf>) {
    if out.len() >= MAX_FOLDER_SEARCH_FILES {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= MAX_FOLDER_SEARCH_FILES {
            return;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let ignored_name = entry
                .file_name()
                .to_str()
                .is_some_and(|name| is_ignored_folder_with(name, ignored_folders));
            let hidden = entry.metadata().is_ok_and(|meta| is_hidden(&meta));
            let marked = has_scan_ignore_marker(&path).unwrap_or(false);
            if !ignored_name && !hidden && !marked {
                collect_folder_audio_files(&path, ignored_folders, out);
            }
        } else if file_type.is_file() && AudioFormat::of(&path).is_some() {
            out.push(path);
        }
    }
}
