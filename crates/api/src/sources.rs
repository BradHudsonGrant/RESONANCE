use std::path::Path;
use std::sync::PoisonError;

use resonance_core::library::is_subfolder_excluded;
use resonance_db::{Database, SourceId};
use resonance_library::folders::list_subfolders;

use crate::Api;
use crate::command::{LibrarySource, LibrarySources, SourceSubfolder};
use crate::error::ApiError;

impl Api {
    pub(crate) fn reconcile_watchers(&self, library: &Database) {
        let Ok(sources) = library.sources() else {
            return;
        };
        let Ok(ignored) = library.ignored_folders() else {
            return;
        };
        let own_files = self.own_files_snapshot();
        self.watcher.reconcile(&sources, &ignored, &own_files);
    }

    pub(crate) fn sources_from(&self, library: &Database) -> Result<LibrarySources, ApiError> {
        let configs = library.sources().map_err(ApiError::library_unreadable)?;
        let ignored_folders = library
            .ignored_folders()
            .map_err(ApiError::library_unreadable)?;
        let active = self.watcher.watching_ids();
        let sources = configs
            .into_iter()
            .map(|source| LibrarySource {
                id: source.id.raw(),
                folder: source.folder,
                include_subfolders: source.include_subfolders,
                watch: source.watch,
                watching_active: active.contains(&source.id),
                excluded_subfolders: source.excluded_subfolders,
                track_count: source.track_count,
            })
            .collect();
        Ok(LibrarySources {
            sources,
            ignored_folders,
        })
    }

    pub(crate) fn sources(&self) -> Result<LibrarySources, ApiError> {
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.sources_from(&library)
    }

    pub(crate) fn source_subfolders(
        &self,
        source_id: i64,
        parent: Option<&str>,
    ) -> Result<Vec<SourceSubfolder>, ApiError> {
        let (config, ignored) = {
            let library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let config = library
                .source_config(SourceId::from_raw(source_id))
                .map_err(ApiError::library_unreadable)?
                .ok_or_else(|| ApiError::bad_request(format!("unknown source {source_id}")))?;
            let ignored = library
                .ignored_folders()
                .map_err(ApiError::library_unreadable)?;
            (config, ignored)
        };
        let own_files = self.own_files_snapshot();
        let entries = list_subfolders(&config, parent, &ignored, &own_files)
            .map_err(ApiError::scan_failed)?;
        Ok(entries
            .into_iter()
            .map(|entry| SourceSubfolder {
                path: entry.path,
                name: entry.name,
                enabled: entry.enabled,
                has_subfolders: entry.has_subfolders,
            })
            .collect())
    }

    pub(crate) fn update_source(
        &self,
        source_id: i64,
        include_subfolders: bool,
        watch: bool,
        excluded_subfolders: &[String],
    ) -> Result<LibrarySources, ApiError> {
        let own_files = self.own_files_snapshot();
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let id = SourceId::from_raw(source_id);
        let previous = library
            .source_config(id)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| ApiError::bad_request(format!("unknown source {source_id}")))?;
        let updated = library
            .update_source(id, include_subfolders, watch, excluded_subfolders)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| ApiError::bad_request(format!("unknown source {source_id}")))?;

        let reopened_subfolders = (!previous.include_subfolders && updated.include_subfolders)
            || (updated.include_subfolders
                && previous
                    .excluded_subfolders
                    .iter()
                    .any(|old| !is_subfolder_excluded(old, &updated.excluded_subfolders)));

        if reopened_subfolders {
            let path = Path::new(&updated.folder);
            if path.exists() {
                let _ = resonance_library::scan::scan(&mut library, path, &own_files);
            }
        }
        self.reconcile_watchers(&library);
        let result = self.sources_from(&library);
        drop(library);
        if reopened_subfolders {
            self.identity.wake();
        }
        result
    }

    pub(crate) fn remove_source(&self, source_id: i64) -> Result<LibrarySources, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        library
            .remove_source(SourceId::from_raw(source_id))
            .map_err(ApiError::library_unreadable)?;
        self.reconcile_watchers(&library);
        self.sources_from(&library)
    }

    pub(crate) fn set_ignored_folders(
        &self,
        folders: &[String],
    ) -> Result<LibrarySources, ApiError> {
        let own_files = self.own_files_snapshot();
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let previous = library
            .ignored_folders()
            .map_err(ApiError::library_unreadable)?;
        let updated = library
            .set_ignored_folders(folders)
            .map_err(ApiError::library_unreadable)?;
        let unignored_any = previous
            .iter()
            .any(|old| !updated.iter().any(|next| next.eq_ignore_ascii_case(old)));

        if unignored_any && let Ok(configs) = library.sources() {
            for source in configs {
                let path = Path::new(&source.folder);
                if path.exists() {
                    let _ = resonance_library::scan::scan(&mut library, path, &own_files);
                }
            }
        }
        self.reconcile_watchers(&library);
        let result = self.sources_from(&library);
        drop(library);
        if unignored_any {
            self.identity.wake();
        }
        result
    }
}
