use std::sync::PoisonError;

use resonance_db::{CollectionSummary, Database, DbError};

use crate::Api;
use crate::command::LibraryCollection;
use crate::error::ApiError;

fn library_collection(summary: CollectionSummary) -> LibraryCollection {
    LibraryCollection {
        id: summary.id,
        name: summary.name,
        position: summary.position,
        track_count: summary.track_count,
    }
}

fn map_collection_db_error(error: DbError) -> ApiError {
    match error {
        DbError::InvalidCollectionName(_) | DbError::DuplicateCollectionName { .. } => {
            ApiError::invalid_input(error)
        }
        other => ApiError::library_unreadable(other),
    }
}

impl Api {
    pub(crate) fn collections_from(
        &self,
        library: &Database,
    ) -> Result<Vec<LibraryCollection>, ApiError> {
        library
            .collections()
            .map(|items| items.into_iter().map(library_collection).collect())
            .map_err(ApiError::library_unreadable)
    }

    pub(crate) fn collections(&self) -> Result<Vec<LibraryCollection>, ApiError> {
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.collections_from(&library)
    }

    pub(crate) fn create_collection(&self, name: &str) -> Result<LibraryCollection, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let created = library
            .create_collection(name)
            .map_err(map_collection_db_error)?;
        Ok(library_collection(created))
    }

    pub(crate) fn rename_collection(
        &self,
        collection_id: i64,
        name: &str,
    ) -> Result<LibraryCollection, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let renamed = library
            .rename_collection(collection_id, name)
            .map_err(map_collection_db_error)?
            .ok_or_else(|| {
                ApiError::invalid_input(format!("No collection exists with id {collection_id}."))
            })?;
        Ok(library_collection(renamed))
    }

    pub(crate) fn delete_collection(
        &self,
        collection_id: i64,
    ) -> Result<Vec<LibraryCollection>, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        library
            .delete_collection(collection_id)
            .map_err(ApiError::library_unreadable)?;
        self.collections_from(&library)
    }

    pub(crate) fn add_tracks_to_collection(
        &self,
        collection_id: i64,
        track_ids: &[i64],
    ) -> Result<LibraryCollection, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let updated = library
            .add_tracks_to_collection(collection_id, track_ids)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| {
                ApiError::invalid_input(format!("No collection exists with id {collection_id}."))
            })?;
        Ok(library_collection(updated))
    }

    pub(crate) fn remove_tracks_from_collection(
        &self,
        collection_id: i64,
        track_ids: &[i64],
    ) -> Result<LibraryCollection, ApiError> {
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let updated = library
            .remove_tracks_from_collection(collection_id, track_ids)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| {
                ApiError::invalid_input(format!("No collection exists with id {collection_id}."))
            })?;
        Ok(library_collection(updated))
    }
}
