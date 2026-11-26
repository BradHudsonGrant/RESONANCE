//! Playlists from other apps: mirrors, export, and the path changes waiting to be written.

use std::path::{self, Path, PathBuf};

use resonance_core::playlist::{PathChange, same_path};
use resonance_db::{Database, MirrorReport, PendingExternalChange, StoredExternalLibrary};
use resonance_integrations::{ExternalLibraryAdapter, ExternalLibraryKind, IntegrationError, m3u};

use crate::Api;
use crate::collections::library_collection;
use crate::command::{
    AppliedExternalChanges, ExportedPlaylist, ExternalChangePreview, ExternalLineEdit,
    FoundPlaylist, PlaylistMirror,
};
use crate::error::ApiError;

/// Where playlist files are copied before Resonance writes to them, inside the data folder.
const BACKUP_FOLDER: [&str; 2] = ["backups", "playlists"];

fn integration_error(error: IntegrationError) -> ApiError {
    ApiError::invalid_input(error)
}

/// A path the user typed or pasted, without the quotes a file manager's "copy as path" adds.
fn typed_path(raw: &str) -> Result<PathBuf, ApiError> {
    let cleaned = raw.trim().trim_matches('"');
    let path = Path::new(cleaned);
    if cleaned.is_empty() || !path.is_absolute() {
        return Err(ApiError::invalid_input(
            "Give the full path of the playlist file.",
        ));
    }
    path::absolute(path).map_err(|error| ApiError::invalid_input(error))
}

fn kind_of(library: &StoredExternalLibrary) -> Result<ExternalLibraryKind, ApiError> {
    ExternalLibraryKind::from_name(&library.kind).ok_or_else(|| {
        ApiError::library_unreadable(format!(
            "The library holds an unknown kind of playlist, {}",
            library.kind
        ))
    })
}

fn path_changes(pending: &[&PendingExternalChange]) -> Vec<PathChange> {
    pending
        .iter()
        .map(|change| PathChange {
            old_path: change.old_path.clone(),
            new_path: change.new_path.clone(),
            title: Some(change.title.clone()),
            duration_s: change.duration_s,
        })
        .collect()
}

fn mirror(library: &mut Database, location: &Path) -> Result<PlaylistMirror, ApiError> {
    let read = m3u::M3u.read_library(location).map_err(integration_error)?;
    let mut entries = Vec::new();
    let mut skipped = 0;
    let mut name = String::new();
    for playlist in read.playlists {
        entries.extend(playlist.entries.into_iter().map(|entry| entry.path));
        skipped += playlist.skipped;
        name = playlist.name;
    }
    let MirrorReport {
        collection,
        matched,
        not_in_library,
        ..
    } = library
        .mirror_external_library(
            ExternalLibraryKind::M3u.name(),
            &location.display().to_string(),
            &name,
            &entries,
        )
        .map_err(ApiError::library_unreadable)?;
    Ok(PlaylistMirror {
        collection: library_collection(collection),
        matched,
        not_in_library,
        skipped,
    })
}

impl Api {
    pub(crate) fn import_playlist(&self, path: &str) -> Result<PlaylistMirror, ApiError> {
        let location = typed_path(path)?;
        mirror(&mut self.locked_library(), &location)
    }

    pub(crate) fn read_playlist_again(
        &self,
        collection_id: i64,
    ) -> Result<PlaylistMirror, ApiError> {
        let mut library = self.locked_library();
        let followed = library
            .external_library_of_collection(collection_id)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| {
                ApiError::invalid_input("This collection does not follow a playlist file.")
            })?;
        mirror(&mut library, Path::new(&followed.location))
    }

    pub(crate) fn find_playlists(&self) -> Result<Vec<FoundPlaylist>, ApiError> {
        let (folders, followed) = {
            let library = self.locked_library();
            let folders: Vec<String> = library
                .sources()
                .map_err(ApiError::library_unreadable)?
                .into_iter()
                .map(|source| source.folder)
                .collect();
            let followed = library
                .external_libraries()
                .map_err(ApiError::library_unreadable)?;
            (folders, followed)
        };
        let mut found = Vec::new();
        for folder in folders {
            let Ok(paths) = m3u::M3u.detect(Path::new(&folder)) else {
                continue;
            };
            for path in paths {
                let shown = path.display().to_string();
                found.push(FoundPlaylist {
                    imported: followed
                        .iter()
                        .any(|library| same_path(&library.location, &shown)),
                    name: path
                        .file_stem()
                        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned()),
                    path: shown,
                });
            }
        }
        Ok(found)
    }

    pub(crate) fn export_collection(
        &self,
        collection_id: i64,
        path: &str,
    ) -> Result<ExportedPlaylist, ApiError> {
        let target = typed_path(path)?;
        let tracks = {
            let library = self.locked_library();
            library
                .collection(collection_id)
                .map_err(ApiError::library_unreadable)?
                .ok_or_else(|| {
                    ApiError::invalid_input(format!(
                        "No collection exists with id {collection_id}."
                    ))
                })?;
            library
                .playlist_tracks(collection_id)
                .map_err(ApiError::library_unreadable)?
        };
        m3u::export(&target, &tracks).map_err(integration_error)?;
        Ok(ExportedPlaylist {
            path: target.display().to_string(),
            tracks: u32::try_from(tracks.len()).unwrap_or(u32::MAX),
        })
    }

    pub(crate) fn external_changes(&self) -> Result<Vec<ExternalChangePreview>, ApiError> {
        let library = self.locked_library();
        let pending = library
            .pending_external_changes()
            .map_err(ApiError::library_unreadable)?;
        let mut previews = Vec::new();
        for followed in library
            .external_libraries()
            .map_err(ApiError::library_unreadable)?
        {
            let waiting: Vec<&PendingExternalChange> = pending
                .iter()
                .filter(|change| change.library_id == followed.id)
                .collect();
            if waiting.is_empty() {
                continue;
            }
            let name = match followed.collection_id {
                Some(id) => library
                    .collection(id)
                    .map_err(ApiError::library_unreadable)?
                    .map(|collection| collection.name),
                None => None,
            }
            .unwrap_or_else(|| followed.location.clone());
            let plan = kind_of(&followed)?
                .adapter()
                .preview_changes(Path::new(&followed.location), &path_changes(&waiting));
            let (edits, not_found, problem) = match plan {
                Ok(plan) => (
                    plan.files
                        .iter()
                        .flat_map(|file| &file.edits)
                        .map(|edit| ExternalLineEdit {
                            line: u32::try_from(edit.line + 1).unwrap_or(u32::MAX),
                            before: edit.before.clone(),
                            after: edit.after.clone(),
                        })
                        .collect(),
                    u32::try_from(plan.not_found.len()).unwrap_or(u32::MAX),
                    None,
                ),
                Err(error) => (Vec::new(), 0, Some(error.to_string())),
            };
            previews.push(ExternalChangePreview {
                library_id: followed.id,
                name,
                location: followed.location,
                changes: u32::try_from(waiting.len()).unwrap_or(u32::MAX),
                edits,
                not_found,
                problem,
            });
        }
        Ok(previews)
    }

    pub(crate) fn apply_external_changes(
        &self,
        library_id: i64,
    ) -> Result<AppliedExternalChanges, ApiError> {
        let mut library = self.locked_library();
        let followed = library
            .external_library(library_id)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| {
                ApiError::invalid_input(format!("No playlist is followed with id {library_id}."))
            })?;
        let backups = library
            .data_folder()
            .ok_or_else(|| {
                ApiError::invalid_input(
                    "Playlists are backed up in the data folder, and this library has none.",
                )
            })?
            .join(BACKUP_FOLDER[0])
            .join(BACKUP_FOLDER[1]);
        let pending = library
            .pending_external_changes()
            .map_err(ApiError::library_unreadable)?;
        let waiting: Vec<&PendingExternalChange> = pending
            .iter()
            .filter(|change| change.library_id == library_id)
            .collect();
        let applied = kind_of(&followed)?
            .adapter()
            .apply_changes(
                Path::new(&followed.location),
                &path_changes(&waiting),
                &backups,
            )
            .map_err(integration_error)?;
        let ids: Vec<i64> = waiting.iter().map(|change| change.id).collect();
        library
            .clear_external_changes(&ids)
            .map_err(ApiError::library_unreadable)?;
        Ok(AppliedExternalChanges {
            library_id,
            edits: u32::try_from(applied.edits).unwrap_or(u32::MAX),
            backups: applied
                .backups
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
        })
    }
}
