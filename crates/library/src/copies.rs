//! A track's other copies and versions, and keeping one copy of a recording.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use resonance_core::duplicates::{
    CopyFacts, Encoding, PairKind, classify_asset_pair, compare_copies,
};
use resonance_db::{Database, DbError, StoredCopy};

use crate::file_operation;
use crate::identity::full_asset_path;

#[derive(Debug)]
pub enum CopiesError {
    TrackNotFound {
        track_id: i64,
    },
    /// The file asked to be kept is not a copy of the track's recording.
    NotACopy {
        asset_id: i64,
    },
    /// The file asked to be kept is missing, so it cannot replace the others.
    KeptFileMissing {
        asset_id: i64,
    },
    Remove {
        path: PathBuf,
        source: io::Error,
    },
    Database(DbError),
}

impl fmt::Display for CopiesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TrackNotFound { track_id } => {
                write!(f, "Track #{track_id} is no longer in the library.")
            }
            Self::NotACopy { asset_id } => {
                write!(f, "File #{asset_id} is not a copy of this recording.")
            }
            Self::KeptFileMissing { asset_id } => {
                write!(f, "File #{asset_id} is missing, so it cannot be kept.")
            }
            Self::Remove { path, source } => {
                write!(
                    f,
                    "{} could not be moved to the bin: {source}",
                    path.display()
                )
            }
            Self::Database(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for CopiesError {}

impl From<DbError> for CopiesError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

/// One file next to the track's own, and how it relates to it.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatedFile {
    pub file: StoredCopy,
    pub kind: PairKind,
}

/// A track's own files, its copies on other tracks and its versions.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackCopies {
    pub track_id: i64,
    /// The track's own files, the one it plays first. Later ones are related to the first.
    pub files: Vec<RelatedFile>,
    /// Files on other tracks with the same recording, best first.
    pub copies: Vec<RelatedFile>,
    /// One file per other track that holds a different version of the song.
    pub versions: Vec<RelatedFile>,
    /// The copy worth keeping when there is more than one, by encoding.
    pub keep_asset_id: Option<i64>,
}

impl TrackCopies {
    /// Every copy of the recording: the track's own files and the copies on other tracks.
    pub fn group(&self) -> impl Iterator<Item = &RelatedFile> {
        self.files.iter().chain(&self.copies)
    }
}

fn encoding(file: &StoredCopy) -> Encoding {
    Encoding::of(
        file.format,
        file.bitrate_kbps,
        file.sample_rate,
        file.bit_depth,
        file.flagged,
    )
}

fn facts(file: &StoredCopy) -> CopyFacts<'_> {
    CopyFacts {
        byte_hash: file.byte_hash,
        audio_hash: file.audio_hash,
        fingerprint: file.fingerprint.as_deref(),
        duration_ms: file.duration_ms,
        artist: file.artist.as_deref(),
        title: Some(file.title.as_str()),
        encoding: encoding(file),
    }
}

/// Files that are present come first, then the better encoding, then the older file.
fn keep_order(left: &StoredCopy, right: &StoredCopy) -> std::cmp::Ordering {
    left.missing
        .cmp(&right.missing)
        .then_with(|| compare_copies(&encoding(left), &encoding(right)))
        .then_with(|| left.asset_id.cmp(&right.asset_id))
}

/// Sorts every file that could relate to `track_id`'s first file into copies and versions.
pub fn find_copies(database: &Database, track_id: i64) -> Result<TrackCopies, CopiesError> {
    let own = database.track_files(track_id)?;
    let Some(first) = own.first().cloned() else {
        return Err(CopiesError::TrackNotFound { track_id });
    };
    let first_facts = facts(&first);
    let files: Vec<RelatedFile> = own
        .into_iter()
        .enumerate()
        .map(|(index, file)| {
            let kind = if index == 0 {
                PairKind::ByteIdentical
            } else {
                classify_asset_pair(&first_facts, &facts(&file))
            };
            RelatedFile { file, kind }
        })
        .collect();

    let mut copies = Vec::new();
    let mut versions: Vec<RelatedFile> = Vec::new();
    for file in database.copy_candidates(&first)? {
        let kind = classify_asset_pair(&first_facts, &facts(&file));
        if kind.is_copy() {
            copies.push(RelatedFile { file, kind });
        } else if kind.is_version() {
            let primary = file.is_primary;
            match versions
                .iter_mut()
                .find(|seen| seen.file.track_id == file.track_id)
            {
                Some(seen) if primary && !seen.file.is_primary => {
                    *seen = RelatedFile { file, kind };
                }
                Some(_) => {}
                None => versions.push(RelatedFile { file, kind }),
            }
        }
    }
    copies.sort_by(|left, right| keep_order(&left.file, &right.file));

    let group_size = files.iter().filter(|file| file.kind.is_copy()).count() + copies.len();
    let keep_asset_id = (group_size > 1)
        .then(|| {
            files
                .iter()
                .filter(|file| file.kind.is_copy())
                .chain(&copies)
                .min_by(|left, right| keep_order(&left.file, &right.file))
                .map(|best| best.file.asset_id)
        })
        .flatten();

    Ok(TrackCopies {
        track_id,
        files,
        copies,
        versions,
        keep_asset_id,
    })
}

/// What keeping one copy did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptCopy {
    pub track_id: i64,
    pub asset_id: i64,
    /// Files moved to the bin, or already gone from disk, and taken out of the library.
    pub removed: Vec<PathBuf>,
}

/// Keeps `keep_asset_id` as the one file of `track_id`'s recording. Every other copy is passed
/// to `remove` (the Recycle Bin in the app) and folded into the kept file's track, so its
/// collections, favourite and edited tags stay. Versions are never touched. Stops at the first
/// file `remove` refuses, leaving that one and the rest in place.
pub fn keep_copy(
    database: &mut Database,
    track_id: i64,
    keep_asset_id: i64,
    mut remove: impl FnMut(&Path) -> io::Result<()>,
) -> Result<KeptCopy, CopiesError> {
    let found = find_copies(database, track_id)?;
    let Some(kept) = found
        .group()
        .find(|related| related.kind.is_copy() && related.file.asset_id == keep_asset_id)
        .map(|related| related.file.clone())
    else {
        return Err(CopiesError::NotACopy {
            asset_id: keep_asset_id,
        });
    };
    if kept.missing {
        return Err(CopiesError::KeptFileMissing {
            asset_id: keep_asset_id,
        });
    }
    let others: Vec<StoredCopy> = found
        .group()
        .filter(|related| related.kind.is_copy() && related.file.asset_id != keep_asset_id)
        .map(|related| related.file.clone())
        .collect();

    let mut removed = Vec::with_capacity(others.len());
    for other in others {
        let path = full_asset_path(&other.source_folder, &other.path);
        {
            let _guard = file_operation();
            if path.exists() {
                remove(&path).map_err(|source| CopiesError::Remove {
                    path: path.clone(),
                    source,
                })?;
            }
        }
        database.remove_copy_into(kept.track_id, other.asset_id)?;
        removed.push(path);
    }
    database.make_primary_asset(kept.asset_id)?;

    Ok(KeptCopy {
        track_id: kept.track_id,
        asset_id: kept.asset_id,
        removed,
    })
}

/// Moves a file to the Recycle Bin, or the desktop's trash, where the user can still restore it.
pub fn move_to_bin(path: &Path) -> io::Result<()> {
    trash::delete(path).map_err(io::Error::other)
}
