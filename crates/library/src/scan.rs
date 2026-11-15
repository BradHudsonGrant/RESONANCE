use std::collections::HashSet;
use std::fmt;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use resonance_core::library::{
    AudioFormat, SCAN_IGNORE_MARKER, ScannedFile, folder_under_source_holding,
    is_ignored_folder_with, is_subfolder_excluded,
};
use resonance_db::{Database, DbError, FileStamp, SourceId};

use crate::{file_tags, identity};

#[derive(Debug)]
pub enum ScanError {
    Folder { path: PathBuf, source: io::Error },
    ExcludedFolder { path: PathBuf },
    Database(DbError),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Folder { path, source } => {
                write!(
                    f,
                    "The folder {} could not be read: {source}",
                    path.display()
                )
            }
            Self::ExcludedFolder { path } => write!(
                f,
                "The folder {} is excluded by {SCAN_IGNORE_MARKER}. Choose a music folder or an unmarked subfolder instead",
                path.display()
            ),
            Self::Database(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for ScanError {}

impl From<DbError> for ScanError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

/// Something the scan could not read, and why. The scan carries on past it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanReport {
    pub source_folder: String,
    pub found: usize,
    pub added: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub missing_changed: usize,
    pub unreadable: Vec<Unreadable>,
}

#[cfg(windows)]
pub(crate) fn is_hidden(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

/// Elsewhere a hidden folder is one whose name starts with a dot, which the name rule covers.
#[cfg(not(windows))]
pub(crate) fn is_hidden(_metadata: &Metadata) -> bool {
    false
}

pub(crate) fn has_scan_ignore_marker(folder: &Path) -> io::Result<bool> {
    // A dangling marker symlink still marks the folder; only its presence matters.
    match fs::symlink_metadata(folder.join(SCAN_IGNORE_MARKER)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

struct Walk<'a> {
    root: &'a Path,
    own_folders: &'a [PathBuf],
    include_subfolders: bool,
    excluded_subfolders: &'a [String],
    ignored_folders: &'a [String],
    files: Vec<PathBuf>,
    unreadable: Vec<Unreadable>,
}

impl Walk<'_> {
    fn shown(&self, path: &Path) -> String {
        path.strip_prefix(self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn could_not_read(&mut self, path: &Path, source: &io::Error) {
        self.unreadable.push(Unreadable {
            path: self.shown(path),
            reason: source.to_string(),
        });
    }

    fn enters(&self, entry: &DirEntry, path: &Path) -> io::Result<bool> {
        if !self.include_subfolders {
            return Ok(false);
        }
        let ignored_by_name = entry
            .file_name()
            .to_str()
            .is_some_and(|name| is_ignored_folder_with(name, self.ignored_folders));
        if ignored_by_name
            || is_hidden(&entry.metadata()?)
            || self.own_folders.iter().any(|own| own == path)
            || has_scan_ignore_marker(path)?
        {
            return Ok(false);
        }
        let Some(relative) = stored_path(self.root, path) else {
            return Ok(false);
        };
        Ok(!is_subfolder_excluded(&relative, self.excluded_subfolders))
    }

    fn folder(&mut self, folder: &Path) {
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(source) => return self.could_not_read(folder, &source),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(source) => {
                    self.could_not_read(folder, &source);
                    continue;
                }
            };
            let path = entry.path();
            let entered = entry.file_type().and_then(|kind| {
                if kind.is_dir() {
                    return self.enters(&entry, &path);
                }
                if kind.is_file() && AudioFormat::of(&path).is_some() {
                    self.files.push(path.clone());
                }
                Ok(false)
            });
            match entered {
                Ok(true) => self.folder(&path),
                Ok(false) => {}
                Err(source) => self.could_not_read(&path, &source),
            }
        }
    }
}

/// The path as the library stores it: below the source, with forward slashes on every platform.
pub(crate) fn stored_path(root: &Path, path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|part| part.as_os_str().to_str())
        .collect();
    Some(parts?.join("/"))
}

fn stamp(metadata: &Metadata) -> io::Result<FileStamp> {
    let modified = metadata.modified()?;
    let nanoseconds = match modified.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_nanos()),
        Err(before) => i64::try_from(before.duration().as_nanos()).map(|nanoseconds| -nanoseconds),
    };
    Ok(FileStamp {
        size_bytes: metadata.len(),
        modified_ns: nanoseconds.map_err(io::Error::other)?,
    })
}

/// Reads every audio file under `folder` into the database and never writes to the folder.
///
/// A file whose size and modified time match the last scan is left as it is. `own_files` are
/// paths that belong to the program, such as where it runs from and where it keeps its data. If
/// one is inside `folder`, the folder directly under `folder` that holds it is skipped.
pub fn scan(
    database: &mut Database,
    folder: &Path,
    own_files: &[PathBuf],
) -> Result<ScanReport, ScanError> {
    let unreadable_folder = |source| ScanError::Folder {
        path: folder.to_owned(),
        source,
    };
    // Walked in its canonical form because on Windows that is the only form that can open a
    // folder whose name ends in a space or a dot, and music folders have them.
    let root = fs::canonicalize(folder).map_err(unreadable_folder)?;
    if has_scan_ignore_marker(&root).map_err(unreadable_folder)? {
        return Err(ScanError::ExcludedFolder {
            path: folder.to_owned(),
        });
    }
    fs::read_dir(&root).map_err(unreadable_folder)?;
    let source_folder = dunce::simplified(&root)
        .to_str()
        .ok_or_else(|| {
            unreadable_folder(io::Error::new(
                io::ErrorKind::InvalidInput,
                "its path is not valid Unicode",
            ))
        })?
        .to_owned();

    let own_folders: Vec<PathBuf> = own_files
        .iter()
        .filter_map(|own| fs::canonicalize(own).ok())
        .filter_map(|own| folder_under_source_holding(&root, &own))
        .collect();
    let source = database.source(&source_folder)?;
    let (include_subfolders, excluded_subfolders) = match database.source_config(source)? {
        Some(config) => (config.include_subfolders, config.excluded_subfolders),
        None => (true, Vec::new()),
    };
    let ignored_folders = database.ignored_folders()?;

    let mut walk = Walk {
        root: &root,
        own_folders: &own_folders,
        include_subfolders,
        excluded_subfolders: &excluded_subfolders,
        ignored_folders: &ignored_folders,
        files: Vec::new(),
        unreadable: Vec::new(),
    };
    walk.folder(&root);
    let Walk {
        mut files,
        mut unreadable,
        ..
    } = walk;
    files.sort();

    let stamps = database.file_stamps(source)?;
    let has_identities = database.has_identified_assets()?;
    let walked_paths: HashSet<String> = files
        .iter()
        .filter_map(|path| stored_path(&root, path))
        .collect();
    let mut claimed_moves: HashSet<i64> = HashSet::new();
    let mut missing_identified_exists: Option<bool> = None;

    let mut report = ScanReport {
        source_folder,
        found: files.len(),
        added: 0,
        changed: 0,
        unchanged: 0,
        missing_changed: 0,
        unreadable: Vec::new(),
    };
    let mut read = Vec::new();
    for path in &files {
        let shown = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let (Some(stored), Some(format)) = (stored_path(&root, path), AudioFormat::of(path)) else {
            unreadable.push(Unreadable {
                path: shown,
                reason: "Its name is not valid Unicode".to_owned(),
            });
            continue;
        };
        let now = match fs::metadata(path).and_then(|metadata| stamp(&metadata)) {
            Ok(now) => now,
            Err(source) => {
                unreadable.push(Unreadable {
                    path: shown,
                    reason: source.to_string(),
                });
                continue;
            }
        };
        let is_new_path = match stamps.get(&stored) {
            Some(before) if *before == now => {
                report.unchanged += 1;
                continue;
            }
            Some(_) => false,
            None => true,
        };
        let (audio, tags) = match file_tags::read(path) {
            Ok(contents) => (Some(contents.audio), contents.tags),
            Err(source) => {
                unreadable.push(Unreadable {
                    path: shown,
                    reason: source.to_string(),
                });
                (None, Vec::new())
            }
        };
        let scanned = ScannedFile {
            path: stored,
            size_bytes: now.size_bytes,
            modified_ns: now.modified_ns,
            format,
            audio,
            tags,
        };

        if is_new_path
            && has_identities
            && try_reconcile_moved_on_scan(
                database,
                source,
                path,
                &scanned,
                &walked_paths,
                &mut claimed_moves,
                &mut missing_identified_exists,
            )?
        {
            report.changed += 1;
            continue;
        }

        if is_new_path {
            report.added += 1;
        } else {
            report.changed += 1;
        }
        read.push(scanned);
    }
    database.record_scan(source, &read)?;

    let now_ms = resonance_db::now_unix_ms();
    let mut presence_updates: Vec<(i64, Option<i64>)> = Vec::new();
    for asset in database.included_assets_presence(source)? {
        if walked_paths.contains(&asset.path) {
            if asset.missing_since.is_some() {
                presence_updates.push((asset.asset_id, None));
            }
        } else if asset.missing_since.is_none()
            && !identity::full_asset_path(&report.source_folder, &asset.path).exists()
        {
            presence_updates.push((asset.asset_id, Some(now_ms)));
        }
    }
    report.missing_changed = database.set_assets_missing_since(&presence_updates)?;

    report.unreadable = unreadable;
    Ok(report)
}

pub(crate) fn scan_single_file(file_path: &Path, stored: String) -> io::Result<ScannedFile> {
    let format = AudioFormat::of(file_path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a supported audio file extension",
        )
    })?;
    let now = fs::metadata(file_path).and_then(|metadata| stamp(&metadata))?;
    let (audio, tags) = match file_tags::read(file_path) {
        Ok(contents) => (Some(contents.audio), contents.tags),
        Err(_) => (None, Vec::new()),
    };
    Ok(ScannedFile {
        path: stored,
        size_bytes: now.size_bytes,
        modified_ns: now.modified_ns,
        format,
        audio,
        tags,
    })
}

fn try_reconcile_moved_on_scan(
    database: &mut Database,
    source: SourceId,
    path: &Path,
    scanned: &ScannedFile,
    walked_paths: &HashSet<String>,
    claimed_moves: &mut HashSet<i64>,
    missing_identified_exists: &mut Option<bool>,
) -> Result<bool, DbError> {
    if let Ok(byte_hash) = identity::compute_byte_hash(path) {
        let candidates = database.assets_with_byte_hash(&byte_hash)?;
        if let Some(matched) = candidates.iter().find(|cand| {
            !claimed_moves.contains(&cand.asset_id)
                && !identity::full_asset_path(&cand.source_folder, &cand.path).exists()
        }) {
            claimed_moves.insert(matched.asset_id);
            database.record_moved_scan_file(matched.asset_id, source, scanned, None)?;
            return Ok(true);
        }
    }

    let any_missing = match *missing_identified_exists {
        Some(value) => value,
        None => {
            let computed = any_identified_asset_missing(database, source, walked_paths)?;
            *missing_identified_exists = Some(computed);
            computed
        }
    };
    if !any_missing {
        return Ok(false);
    }

    if let Ok(computed_id) = identity::compute_identity(path)
        && let Some(audio) = &computed_id.audio
    {
        let candidates = database.assets_with_audio_hash(&audio.hash)?;
        if let Some(matched) = candidates.iter().find(|cand| {
            !claimed_moves.contains(&cand.asset_id)
                && !identity::full_asset_path(&cand.source_folder, &cand.path).exists()
        }) {
            claimed_moves.insert(matched.asset_id);
            database.record_moved_scan_file(
                matched.asset_id,
                source,
                scanned,
                Some(&computed_id),
            )?;
            return Ok(true);
        }
    }

    Ok(false)
}

fn any_identified_asset_missing(
    database: &Database,
    current_source: SourceId,
    walked_paths: &HashSet<String>,
) -> Result<bool, DbError> {
    for source_cfg in database.sources()? {
        let source_id = source_cfg.id;
        for asset in database.identified_assets_in_source(source_id)? {
            if source_id == current_source && walked_paths.contains(&asset.path) {
                continue;
            }
            if !identity::full_asset_path(&asset.source_folder, &asset.path).exists() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
