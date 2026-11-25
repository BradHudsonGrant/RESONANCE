//! A track's copies and versions as the Inspector and Prep show them.

use std::path::Path;

use resonance_core::duplicates::PairKind;
use resonance_db::Database;
use resonance_library::copies::{
    CopiesError, RelatedFile, TrackCopies as FoundCopies, find_copies, keep_copy, move_to_bin,
};

use crate::Api;
use crate::command::{CopyFile, CopyKind, TrackAudio, TrackCopies};
use crate::error::ApiError;

fn copy_file(related: &RelatedFile, this_file: bool) -> CopyFile {
    let file = &related.file;
    let (kind, similarity) = match related.kind {
        _ if this_file => (CopyKind::ThisFile, None),
        PairKind::ByteIdentical => (CopyKind::ByteIdentical, None),
        PairKind::AudioIdentical => (CopyKind::AudioIdentical, None),
        PairKind::SameRecording { similarity } => {
            (CopyKind::SameRecording, Some(f64::from(similarity)))
        }
        PairKind::DifferentVersion { .. } | PairKind::Unrelated => (CopyKind::Version, None),
    };
    let mut location = Path::new(&file.source_folder).to_owned();
    location.extend(file.path.split('/'));
    CopyFile {
        asset_id: file.asset_id,
        track_id: file.track_id,
        kind,
        relation: if this_file {
            "This file".to_owned()
        } else {
            related.kind.label().to_owned()
        },
        similarity,
        title: file.title.clone(),
        artist: file.artist.clone(),
        format: file.format.name().to_owned(),
        location: location.to_string_lossy().into_owned(),
        audio: TrackAudio {
            duration_ms: file.duration_ms.unwrap_or_default(),
            sample_rate: file.sample_rate,
            channels: None,
            bit_depth: file.bit_depth,
            bitrate_kbps: file.bitrate_kbps,
        },
        flagged: file.flagged,
        missing: file.missing,
    }
}

fn track_copies(found: &FoundCopies, removed: u32) -> TrackCopies {
    TrackCopies {
        track_id: found.track_id,
        files: found
            .files
            .iter()
            .enumerate()
            .map(|(index, related)| copy_file(related, index == 0))
            .collect(),
        copies: found
            .copies
            .iter()
            .map(|related| copy_file(related, false))
            .collect(),
        versions: found
            .versions
            .iter()
            .map(|related| copy_file(related, false))
            .collect(),
        keep_asset_id: found.keep_asset_id,
        removed,
    }
}

fn map_copies_error(error: CopiesError) -> ApiError {
    match error {
        CopiesError::Database(source) => ApiError::library_unreadable(source),
        other => ApiError::invalid_input(other),
    }
}

pub(crate) fn copies_of(library: &Database, track_id: i64) -> Result<TrackCopies, ApiError> {
    let found = find_copies(library, track_id).map_err(map_copies_error)?;
    Ok(track_copies(&found, 0))
}

impl Api {
    pub(crate) fn track_copies(&self, track_id: i64) -> Result<TrackCopies, ApiError> {
        copies_of(&self.locked_library(), track_id)
    }

    pub(crate) fn keep_copy(&self, track_id: i64, asset_id: i64) -> Result<TrackCopies, ApiError> {
        let copies = {
            let mut library = self.locked_library();
            let kept = keep_copy(&mut library, track_id, asset_id, move_to_bin)
                .map_err(map_copies_error)?;
            let found = find_copies(&library, kept.track_id).map_err(map_copies_error)?;
            track_copies(
                &found,
                u32::try_from(kept.removed.len()).unwrap_or(u32::MAX),
            )
        };
        self.core
            .watch_revisions
            .send_modify(|revision| *revision += 1);
        Ok(copies)
    }
}
