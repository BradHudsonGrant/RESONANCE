use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use resonance_core::library::{
    folder_under_source_holding, is_ignored_folder_with, is_subfolder_excluded,
    normalise_subfolder_path,
};
use resonance_db::SourceConfig;

use crate::scan::{ScanError, has_scan_ignore_marker, is_hidden, stored_path};

/// One subfolder inside a configured library source, for the subfolder toggle tree in Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubfolderEntry {
    /// Relative to the source folder, with forward slashes on every platform.
    pub path: String,
    pub name: String,
    pub enabled: bool,
    pub has_subfolders: bool,
}

fn visible_directory(
    entry_name: &str,
    path: &Path,
    ignored_folders: &[String],
    own_folders: &[PathBuf],
) -> io::Result<bool> {
    if is_ignored_folder_with(entry_name, ignored_folders) {
        return Ok(false);
    }
    if own_folders.iter().any(|own| own == path) {
        return Ok(false);
    }
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir() || is_hidden(&metadata) {
        return Ok(false);
    }
    Ok(!has_scan_ignore_marker(path)?)
}

fn has_visible_child_directory(
    folder: &Path,
    ignored_folders: &[String],
    own_folders: &[PathBuf],
) -> bool {
    let Ok(entries) = fs::read_dir(folder) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if visible_directory(name, &entry.path(), ignored_folders, own_folders).unwrap_or(false) {
            return true;
        }
    }
    false
}

/// Lists the direct subfolders under `parent` (or directly under the source root when `parent` is
/// `None`), skipping hidden folders, `.resonance-ignore`, the app's own folder and names in
/// `ignored_folders`, while keeping user-toggled `excluded_subfolders` visible so they can be
/// switched back on.
pub fn list_subfolders(
    config: &SourceConfig,
    parent: Option<&str>,
    ignored_folders: &[String],
    own_files: &[PathBuf],
) -> Result<Vec<SubfolderEntry>, ScanError> {
    let source_path = Path::new(&config.folder);
    let root = fs::canonicalize(source_path).map_err(|source| ScanError::Folder {
        path: source_path.to_owned(),
        source,
    })?;

    let target = match parent.map(str::trim).filter(|rel| !rel.is_empty()) {
        None => root.clone(),
        Some(_) => {
            let raw = parent.unwrap_or_default();
            let Some(clean) = normalise_subfolder_path(raw) else {
                return Err(ScanError::Folder {
                    path: source_path.join(raw),
                    source: io::Error::new(io::ErrorKind::InvalidInput, "invalid subfolder path"),
                });
            };
            let mut candidate = root.clone();
            for segment in clean.split('/') {
                candidate.push(segment);
            }
            let resolved = fs::canonicalize(&candidate).map_err(|source| ScanError::Folder {
                path: candidate,
                source,
            })?;
            if !resolved.starts_with(&root) {
                return Err(ScanError::Folder {
                    path: resolved,
                    source: io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "subfolder resolves outside source",
                    ),
                });
            }
            resolved
        }
    };

    let own_folders: Vec<PathBuf> = own_files
        .iter()
        .filter_map(|own| fs::canonicalize(own).ok())
        .filter_map(|own| folder_under_source_holding(&root, &own))
        .collect();

    let entries = fs::read_dir(&target).map_err(|source| ScanError::Folder {
        path: target.clone(),
        source,
    })?;

    let mut listed = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let path = entry.path();
        if !visible_directory(name, &path, ignored_folders, &own_folders).unwrap_or(false) {
            continue;
        }
        let Some(relative) = stored_path(&root, &path) else {
            continue;
        };
        let enabled = config.include_subfolders
            && !is_subfolder_excluded(&relative, &config.excluded_subfolders);
        let has_subfolders = has_visible_child_directory(&path, ignored_folders, &own_folders);
        listed.push(SubfolderEntry {
            path: relative,
            name: name.to_owned(),
            enabled,
            has_subfolders,
        });
    }

    listed.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(listed)
}
