use std::collections::HashSet;
use std::fmt;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use resonance_core::identity::{AssetIdentity, ContentHash};
use resonance_core::library::{
    AudioFormat, SCAN_IGNORE_MARKER, ScannedFile, folder_under_source_holding,
    is_ignored_folder_with, is_subfolder_excluded,
};
use resonance_db::{Database, DbError, FileStamp, IdentityMatch, SourceId};

use crate::{file_tags, identity};

pub const SCAN_BATCH_SIZE: usize = 200;

#[derive(Debug)]
pub enum ScanError {
    Folder { path: PathBuf, source: io::Error },
    ExcludedFolder { path: PathBuf },
    Cancelled,
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
            Self::Cancelled => write!(f, "The scan was cancelled."),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanBatchProgress {
    pub done_files: u64,
    pub total_files: u64,
    pub wrote_changes: bool,
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
    cancel: Option<&'a AtomicBool>,
    files: Vec<PathBuf>,
    unreadable: Vec<Unreadable>,
}

impl Walk<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancel.is_some_and(|flag| flag.load(Ordering::Acquire))
    }

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
        if self.is_cancelled() {
            return;
        }
        let entries = match fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(source) => return self.could_not_read(folder, &source),
        };
        for entry in entries {
            if self.is_cancelled() {
                return;
            }
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

pub(crate) fn stamp(metadata: &Metadata) -> io::Result<FileStamp> {
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
    let cancel = AtomicBool::new(false);
    let wrapped = Mutex::new(std::mem::replace(
        database,
        Database::open_in_memory().map_err(ScanError::Database)?,
    ));
    let result = scan_chunked_with_batch_size(
        &wrapped,
        folder,
        own_files,
        SCAN_BATCH_SIZE,
        &cancel,
        |_| {},
    );
    *database = wrapped.into_inner().unwrap_or_else(PoisonError::into_inner);
    result
}

/// Walks `folder` and reads file tags outside the database mutex, committing batches of
/// `SCAN_BATCH_SIZE` (`200`) files per short WAL transaction so concurrent UI reads never block.
pub fn scan_chunked<F>(
    database: &Mutex<Database>,
    folder: &Path,
    own_files: &[PathBuf],
    cancel: &AtomicBool,
    on_batch: F,
) -> Result<ScanReport, ScanError>
where
    F: FnMut(ScanBatchProgress),
{
    scan_chunked_with_batch_size(
        database,
        folder,
        own_files,
        SCAN_BATCH_SIZE,
        cancel,
        on_batch,
    )
}

#[doc(hidden)]
pub fn scan_chunked_with_batch_size<F>(
    database: &Mutex<Database>,
    folder: &Path,
    own_files: &[PathBuf],
    batch_size: usize,
    cancel: &AtomicBool,
    mut on_batch: F,
) -> Result<ScanReport, ScanError>
where
    F: FnMut(ScanBatchProgress),
{
    let check_cancelled = || {
        if cancel.load(Ordering::Acquire) {
            Err(ScanError::Cancelled)
        } else {
            Ok(())
        }
    };
    check_cancelled()?;

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

    let (source, include_subfolders, excluded_subfolders, ignored_folders, stamps, has_identities) = {
        let mut db = database.lock().unwrap_or_else(PoisonError::into_inner);
        let source = db.source(&source_folder)?;
        let (include_subfolders, excluded_subfolders) = match db.source_config(source)? {
            Some(config) => (config.include_subfolders, config.excluded_subfolders),
            None => (true, Vec::new()),
        };
        let ignored_folders = db.ignored_folders()?;
        let stamps = db.file_stamps(source)?;
        let has_identities = db.has_identified_assets()?;
        (
            source,
            include_subfolders,
            excluded_subfolders,
            ignored_folders,
            stamps,
            has_identities,
        )
    };
    check_cancelled()?;

    let mut walk = Walk {
        root: &root,
        own_folders: &own_folders,
        include_subfolders,
        excluded_subfolders: &excluded_subfolders,
        ignored_folders: &ignored_folders,
        cancel: Some(cancel),
        files: Vec::new(),
        unreadable: Vec::new(),
    };
    walk.folder(&root);
    check_cancelled()?;

    let Walk {
        mut files,
        mut unreadable,
        ..
    } = walk;
    files.sort();

    let walked_paths: HashSet<String> = files
        .iter()
        .filter_map(|path| stored_path(&root, path))
        .collect();
    let total_files = files.len() as u64;
    on_batch(ScanBatchProgress {
        done_files: 0,
        total_files,
        wrote_changes: false,
    });
    check_cancelled()?;

    let any_missing_identified = if has_identities {
        check_any_identified_asset_missing_outside_lock(database, source, &walked_paths)?
    } else {
        false
    };

    let mut claimed_moves: HashSet<i64> = HashSet::new();
    let mut report = ScanReport {
        source_folder,
        found: files.len(),
        added: 0,
        changed: 0,
        unchanged: 0,
        missing_changed: 0,
        unreadable: Vec::new(),
    };

    let mut done_files = 0u64;
    for chunk in files.chunks(batch_size.max(1)) {
        check_cancelled()?;

        let mut prepared: Vec<PreparedScanFile> = Vec::new();
        let mut chunk_unchanged = 0usize;

        for path in chunk {
            check_cancelled()?;
            let shown = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let (Some(stored), Some(format)) = (stored_path(&root, path), AudioFormat::of(path))
            else {
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
                    chunk_unchanged += 1;
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
            let byte_hash = if is_new_path && has_identities {
                identity::compute_byte_hash(path).ok()
            } else {
                None
            };
            prepared.push(PreparedScanFile {
                disk_path: path.clone(),
                is_new_path,
                scanned,
                byte_hash,
            });
        }

        check_cancelled()?;

        let (resolved_moves, remaining) = if has_identities && !prepared.is_empty() {
            resolve_chunk_moves_outside_lock(
                database,
                prepared,
                any_missing_identified,
                &mut claimed_moves,
                cancel,
            )?
        } else {
            (Vec::new(), prepared)
        };
        check_cancelled()?;

        let mut chunk_added = 0usize;
        let mut chunk_changed = resolved_moves.len();
        let mut batch_to_record = Vec::with_capacity(remaining.len());
        for item in remaining {
            if item.is_new_path {
                chunk_added += 1;
            } else {
                chunk_changed += 1;
            }
            batch_to_record.push(item.scanned);
        }

        let wrote_changes = !resolved_moves.is_empty() || !batch_to_record.is_empty();
        if wrote_changes {
            let mut db = database.lock().unwrap_or_else(PoisonError::into_inner);
            for mv in &resolved_moves {
                db.record_moved_scan_file(
                    mv.matched_asset_id,
                    source,
                    &mv.scanned,
                    mv.computed_identity.as_ref(),
                )?;
            }
            if !batch_to_record.is_empty() {
                db.record_scan(source, &batch_to_record)?;
            }
        }

        report.unchanged += chunk_unchanged;
        report.added += chunk_added;
        report.changed += chunk_changed;
        done_files += chunk.len() as u64;

        on_batch(ScanBatchProgress {
            done_files,
            total_files,
            wrote_changes,
        });
        check_cancelled()?;
    }

    let presence = {
        let db = database.lock().unwrap_or_else(PoisonError::into_inner);
        db.included_assets_presence(source)?
    };
    let now_ms = resonance_db::now_unix_ms();
    let mut presence_updates: Vec<(i64, Option<i64>)> = Vec::new();
    for asset in presence {
        check_cancelled()?;
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
    check_cancelled()?;
    if !presence_updates.is_empty() {
        let mut db = database.lock().unwrap_or_else(PoisonError::into_inner);
        report.missing_changed = db.set_assets_missing_since(&presence_updates)?;
    }

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

struct PreparedScanFile {
    disk_path: PathBuf,
    is_new_path: bool,
    scanned: ScannedFile,
    byte_hash: Option<ContentHash>,
}

struct ResolvedMove {
    matched_asset_id: i64,
    scanned: ScannedFile,
    computed_identity: Option<AssetIdentity>,
}

fn resolve_chunk_moves_outside_lock(
    database: &Mutex<Database>,
    prepared: Vec<PreparedScanFile>,
    any_missing_identified: bool,
    claimed_moves: &mut HashSet<i64>,
    cancel: &AtomicBool,
) -> Result<(Vec<ResolvedMove>, Vec<PreparedScanFile>), ScanError> {
    let byte_candidates: Vec<Vec<IdentityMatch>> = {
        let db = database.lock().unwrap_or_else(PoisonError::into_inner);
        let mut out = Vec::with_capacity(prepared.len());
        for item in &prepared {
            match &item.byte_hash {
                Some(hash) => out.push(db.assets_with_byte_hash(hash)?),
                None => out.push(Vec::new()),
            }
        }
        out
    };

    let mut resolved = Vec::new();
    let mut unmatched = Vec::new();

    for (item, candidates) in prepared.into_iter().zip(byte_candidates) {
        if cancel.load(Ordering::Acquire) {
            return Err(ScanError::Cancelled);
        }
        if item.is_new_path
            && let Some(matched) = candidates.iter().find(|cand| {
                !claimed_moves.contains(&cand.asset_id)
                    && !identity::full_asset_path(&cand.source_folder, &cand.path).exists()
            })
        {
            claimed_moves.insert(matched.asset_id);
            resolved.push(ResolvedMove {
                matched_asset_id: matched.asset_id,
                scanned: item.scanned,
                computed_identity: None,
            });
            continue;
        }
        unmatched.push(item);
    }

    if !any_missing_identified {
        return Ok((resolved, unmatched));
    }

    let mut still_unmatched = Vec::with_capacity(unmatched.len());
    for item in unmatched {
        if cancel.load(Ordering::Acquire) {
            return Err(ScanError::Cancelled);
        }
        if item.is_new_path
            && let Ok(computed_id) = identity::compute_identity(&item.disk_path)
            && let Some(audio) = &computed_id.audio
        {
            let candidates = {
                let db = database.lock().unwrap_or_else(PoisonError::into_inner);
                db.assets_with_audio_hash(&audio.hash)?
            };
            if let Some(matched) = candidates.iter().find(|cand| {
                !claimed_moves.contains(&cand.asset_id)
                    && !identity::full_asset_path(&cand.source_folder, &cand.path).exists()
            }) {
                claimed_moves.insert(matched.asset_id);
                resolved.push(ResolvedMove {
                    matched_asset_id: matched.asset_id,
                    scanned: item.scanned,
                    computed_identity: Some(computed_id),
                });
                continue;
            }
        }
        still_unmatched.push(item);
    }

    Ok((resolved, still_unmatched))
}

fn check_any_identified_asset_missing_outside_lock(
    database: &Mutex<Database>,
    current_source: SourceId,
    walked_paths: &HashSet<String>,
) -> Result<bool, DbError> {
    let assets: Vec<IdentityMatch> = {
        let db = database.lock().unwrap_or_else(PoisonError::into_inner);
        let mut all = Vec::new();
        for source_cfg in db.sources()? {
            all.extend(db.identified_assets_in_source(source_cfg.id)?);
        }
        all
    };
    for asset in assets {
        if asset.source_id == current_source && walked_paths.contains(&asset.path) {
            continue;
        }
        if !identity::full_asset_path(&asset.source_folder, &asset.path).exists() {
            return Ok(true);
        }
    }
    Ok(false)
}
