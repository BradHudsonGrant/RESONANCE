//! M3U and M3U8 playlists. Each playlist file is a library of its own holding one playlist.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use resonance_core::playlist::{
    M3uDocument, PathChange, PlaylistTrack, normalise_path, plan_path_changes, render_m3u8,
    resolve_entry, same_path,
};

use crate::{
    ChangePlan, ExternalEntry, ExternalLibrary, ExternalLibraryAdapter, ExternalLibraryKind,
    ExternalPlaylist, IntegrationError, PlannedFile, file_error,
};

pub struct M3u;

/// Whether the file's extension says it is a playlist.
pub fn is_playlist(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("m3u") || extension.eq_ignore_ascii_case("m3u8")
        })
}

fn folder_of(location: &Path) -> String {
    normalise_path(&location.parent().unwrap_or(Path::new("")).to_string_lossy())
}

fn read(location: &Path) -> Result<(Vec<u8>, M3uDocument), IntegrationError> {
    if !is_playlist(location) {
        return Err(IntegrationError::NotAPlaylist {
            path: location.to_owned(),
        });
    }
    let bytes = fs::read(location).map_err(file_error(location))?;
    let document = M3uDocument::decode(&bytes);
    Ok((bytes, document))
}

fn find_playlists(folder: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            find_playlists(&path, found);
        } else if kind.is_file() && is_playlist(&path) {
            found.push(path);
        }
    }
}

impl ExternalLibraryAdapter for M3u {
    fn kind(&self) -> ExternalLibraryKind {
        ExternalLibraryKind::M3u
    }

    fn detect(&self, folder: &Path) -> Result<Vec<PathBuf>, IntegrationError> {
        fs::metadata(folder).map_err(file_error(folder))?;
        let mut found = Vec::new();
        find_playlists(folder, &mut found);
        Ok(found)
    }

    fn read_library(&self, location: &Path) -> Result<ExternalLibrary, IntegrationError> {
        let (_, document) = read(location)?;
        let folder = folder_of(location);
        let mut entries = Vec::new();
        let mut skipped = 0;
        for entry in document.entries() {
            match resolve_entry(&folder, &entry.raw) {
                Some(path) => entries.push(ExternalEntry {
                    path,
                    title: entry.extinf.as_ref().and_then(|info| info.title.clone()),
                    duration_s: entry.extinf.as_ref().and_then(|info| info.duration_s),
                }),
                None => skipped += 1,
            }
        }
        let name = location
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        Ok(ExternalLibrary {
            kind: ExternalLibraryKind::M3u,
            location: location.to_owned(),
            playlists: vec![ExternalPlaylist {
                name,
                entries,
                skipped,
            }],
        })
    }

    fn preview_changes(
        &self,
        location: &Path,
        changes: &[PathChange],
    ) -> Result<ChangePlan, IntegrationError> {
        let (before, mut document) = read(location)?;
        let folder = folder_of(location);
        let named: Vec<String> = document
            .entries()
            .iter()
            .filter_map(|entry| resolve_entry(&folder, &entry.raw))
            .collect();
        let not_found = changes
            .iter()
            .filter(|change| !named.iter().any(|path| same_path(path, &change.old_path)))
            .map(|change| change.old_path.clone())
            .collect();
        let edits = plan_path_changes(&document, &folder, changes);
        if edits.is_empty() {
            return Ok(ChangePlan {
                files: Vec::new(),
                not_found,
            });
        }
        for edit in &edits {
            document.set_line(edit.line, edit.after.clone());
        }
        let after = document.encode().map_err(|source| IntegrationError::Text {
            path: location.to_owned(),
            source,
        })?;
        Ok(ChangePlan {
            files: vec![PlannedFile::new(location.to_owned(), edits, before, after)],
            not_found,
        })
    }
}

/// Writes a new extended M3U8 playlist at `path`. An existing file is never overwritten.
pub fn export(path: &Path, tracks: &[PlaylistTrack]) -> Result<(), IntegrationError> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("m3u8"))
    {
        return Err(IntegrationError::NotAPlaylist {
            path: path.to_owned(),
        });
    }
    let text = render_m3u8(tracks);
    let mut out = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(out) => out,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(IntegrationError::AlreadyExists {
                path: path.to_owned(),
            });
        }
        Err(error) => return Err(file_error(path)(error)),
    };
    let written = out.write_all(text.as_bytes()).and_then(|()| out.sync_all());
    drop(out);
    if let Err(error) = written {
        let _ = fs::remove_file(path);
        return Err(file_error(path)(error));
    }
    if fs::read(path).map_err(file_error(path))? != text.as_bytes() {
        return Err(IntegrationError::VerifyFailed {
            path: path.to_owned(),
            restored: false,
        });
    }
    Ok(())
}
