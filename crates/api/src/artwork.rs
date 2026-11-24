//! Track pictures: what a track shows and what identification proposes, the user's own pick,
//! and the stored bytes the page loads by hash.

use std::path::PathBuf;

use resonance_core::artwork::decode_base64;
use resonance_db::{ArtworkError, Database};
use resonance_identify::provider::display_name;

use crate::Api;
use crate::command::{ProposedTrackArtwork, TrackArtwork};
use crate::error::ApiError;

/// A stored picture as the page receives it.
pub struct ArtworkFile {
    pub path: PathBuf,
    pub mime: &'static str,
}

pub(crate) fn artwork_of(library: &Database, track_id: i64) -> Result<TrackArtwork, ApiError> {
    let shown = library
        .track_artwork(track_id)
        .map_err(ApiError::library_unreadable)?;
    let proposed = library
        .proposed_artwork(track_id)
        .map_err(ApiError::library_unreadable)?
        .filter(|proposed| proposed.pending)
        .map(|proposed| ProposedTrackArtwork {
            artwork: proposed.artwork.hash,
            provider: display_name(&proposed.provider).to_owned(),
        });
    let dirty_on_disk = library
        .has_unwritten_artwork(track_id)
        .map_err(ApiError::library_unreadable)?;
    Ok(TrackArtwork {
        track_id,
        artwork: shown.map(|artwork| artwork.hash),
        proposed,
        dirty_on_disk,
    })
}

impl Api {
    pub(crate) fn track_artwork(&self, track_id: i64) -> Result<TrackArtwork, ApiError> {
        artwork_of(&self.locked_library(), track_id)
    }

    pub(crate) fn accept_proposed_artwork(&self, track_id: i64) -> Result<TrackArtwork, ApiError> {
        let mut library = self.locked_library();
        let accepted = library
            .accept_proposed_artwork(track_id)
            .map_err(ApiError::library_unreadable)?;
        let artwork = artwork_of(&library, track_id)?;
        drop(library);
        if accepted {
            self.library_edited();
        }
        Ok(artwork)
    }

    pub(crate) fn skip_proposed_artwork(&self, track_id: i64) -> Result<TrackArtwork, ApiError> {
        let mut library = self.locked_library();
        library
            .skip_proposed_artwork(track_id)
            .map_err(ApiError::library_unreadable)?;
        artwork_of(&library, track_id)
    }

    /// Makes a picture the user picked or dropped the track's picture. It reaches the file the
    /// next time the user writes tags.
    pub(crate) fn set_track_artwork(
        &self,
        track_id: i64,
        image_base64: &str,
    ) -> Result<TrackArtwork, ApiError> {
        let bytes = decode_base64(image_base64)
            .ok_or_else(|| ApiError::invalid_input("The picture did not arrive in one piece."))?;
        let store = self
            .locked_library()
            .artwork_store()
            .ok_or_else(ApiError::data_folder_unavailable)?;
        let stored = store.put(&bytes).map_err(|error| match error {
            ArtworkError::Refused(refusal) => ApiError::invalid_input(refusal.message()),
            ArtworkError::Io(_) => ApiError::internal(error),
        })?;
        let mut library = self.locked_library();
        let set = library
            .set_user_artwork(track_id, &stored)
            .map_err(ApiError::library_unreadable)?;
        if !set {
            return Err(ApiError::invalid_input(format!(
                "There is no track {track_id} in the library."
            )));
        }
        library
            .clear_pending_proposed_artwork(track_id)
            .map_err(ApiError::library_unreadable)?;
        let artwork = artwork_of(&library, track_id)?;
        drop(library);
        self.library_edited();
        Ok(artwork)
    }

    /// The stored picture with this hash, for the page to load. Nothing but a hash reaches the
    /// file system.
    pub fn artwork_file(&self, hash: &str) -> Option<ArtworkFile> {
        let store = self.locked_library().artwork_store()?;
        let (path, kind) = store.find(hash)?;
        Some(ArtworkFile {
            path,
            mime: kind.mime(),
        })
    }
}
