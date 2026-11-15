use std::path::Path;
use std::sync::PoisonError;

use resonance_core::identity::RelinkMatchKind;
use resonance_library::relink::{
    RelinkError, RelinkFileFacts, apply_relink as lib_apply_relink, find_relink_candidates,
    preview_relink as lib_preview_relink,
};

use crate::command::{
    LibraryTrack, RelinkCandidate, RelinkFileSummary, RelinkMatchEvidence, RelinkPreview,
};
use crate::error::ApiError;
use crate::{Api, library_track};

impl Api {
    pub(crate) fn relink_candidates(
        &self,
        track_id: i64,
        search_folder: Option<&str>,
    ) -> Result<Vec<RelinkCandidate>, ApiError> {
        let folder = search_folder
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(Path::new);
        let candidates = {
            let library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            find_relink_candidates(&library, track_id, folder).map_err(map_relink_error)?
        };
        Ok(candidates
            .into_iter()
            .map(|cand| RelinkCandidate {
                candidate_path: cand.candidate_path,
                existing_track_id: cand.existing_track_id,
                evidence: to_evidence(cand.verdict),
                file: to_file_summary(cand.file),
            })
            .collect())
    }

    pub(crate) fn preview_relink(
        &self,
        track_id: i64,
        candidate_path: &str,
    ) -> Result<RelinkPreview, ApiError> {
        let trimmed = candidate_path.trim();
        if trimmed.is_empty() {
            return Err(ApiError::invalid_input("Choose an audio file to inspect."));
        }
        let preview = {
            let library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            lib_preview_relink(&library, track_id, Path::new(trimmed)).map_err(map_relink_error)?
        };
        Ok(RelinkPreview {
            track_id: preview.track_id,
            evidence: to_evidence(preview.verdict),
            requires_confirmation: preview.requires_confirmation,
            current: to_file_summary(preview.current),
            candidate: to_file_summary(preview.candidate),
            merges_existing_track_id: preview.merges_existing_track_id,
        })
    }

    pub(crate) fn apply_relink(
        &self,
        track_id: i64,
        candidate_path: &str,
        force: bool,
    ) -> Result<LibraryTrack, ApiError> {
        let trimmed = candidate_path.trim();
        if trimmed.is_empty() {
            return Err(ApiError::invalid_input("Choose an audio file to relink."));
        }
        let row = {
            let mut library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            lib_apply_relink(&mut library, track_id, Path::new(trimmed), force)
                .map_err(map_relink_error)?;
            self.reconcile_watchers(&library);
            library
                .track_by_id(track_id)
                .map_err(ApiError::library_unreadable)?
                .ok_or_else(|| {
                    ApiError::invalid_input(format!(
                        "Track #{track_id} is no longer in the library."
                    ))
                })?
        };
        self.core
            .watch_revisions
            .send_modify(|revision| *revision += 1);
        Ok(library_track(&row))
    }
}

fn map_relink_error(error: RelinkError) -> ApiError {
    match error {
        RelinkError::Database(source) => ApiError::library_unreadable(source),
        other => ApiError::invalid_input(other),
    }
}

fn to_evidence(verdict: RelinkMatchKind) -> RelinkMatchEvidence {
    match verdict {
        RelinkMatchKind::ExactBytes => RelinkMatchEvidence::ExactBytes,
        RelinkMatchKind::ExactAudio => RelinkMatchEvidence::ExactAudio,
        RelinkMatchKind::SameRecording { similarity } => {
            RelinkMatchEvidence::SameRecording { similarity }
        }
        RelinkMatchKind::DifferentRecording { similarity } => {
            RelinkMatchEvidence::DifferentRecording { similarity }
        }
    }
}

fn to_file_summary(facts: RelinkFileFacts) -> RelinkFileSummary {
    RelinkFileSummary {
        path: facts.path,
        format: facts.format.name().to_owned(),
        size_bytes: facts.size_bytes,
        duration_ms: facts.duration_ms,
        sample_rate: facts.sample_rate,
        bit_depth: facts.bit_depth,
        bitrate_kbps: facts.bitrate_kbps,
    }
}
