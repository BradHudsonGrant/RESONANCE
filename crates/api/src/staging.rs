use std::sync::{MutexGuard, PoisonError};

use resonance_core::library::TagField;
use resonance_core::staging::{
    FieldDiff, ProposalStatus, Provenance, normalise_value, staging_diff,
};
use resonance_db::Database;

use crate::Api;
use crate::command::{
    AcceptedProposals, ProposalState, StagedField, StagingQueueItem, StagingQueuePage, TagName,
    TagProvenance, TrackStagingDiff, UserTagEdit, UserTagReport,
};
use crate::error::ApiError;

const MAX_QUEUE_PAGE: u32 = 500;

pub(crate) fn tag_field(name: TagName) -> TagField {
    match name {
        TagName::Title => TagField::Title,
        TagName::Artist => TagField::Artist,
        TagName::Mix => TagField::Mix,
        TagName::Album => TagField::Album,
        TagName::AlbumArtist => TagField::AlbumArtist,
        TagName::Genre => TagField::Genre,
        TagName::Subgenre => TagField::Subgenre,
        TagName::Label => TagField::Label,
        TagName::CatNo => TagField::CatNo,
        TagName::Year => TagField::Year,
        TagName::Bpm => TagField::Bpm,
        TagName::Key => TagField::Key,
        TagName::TrackNumber => TagField::TrackNumber,
        TagName::Comment => TagField::Comment,
    }
}

pub(crate) fn tag_name(field: TagField) -> TagName {
    match field {
        TagField::Title => TagName::Title,
        TagField::Artist => TagName::Artist,
        TagField::Mix => TagName::Mix,
        TagField::Album => TagName::Album,
        TagField::AlbumArtist => TagName::AlbumArtist,
        TagField::Genre => TagName::Genre,
        TagField::Subgenre => TagName::Subgenre,
        TagField::Label => TagName::Label,
        TagField::CatNo => TagName::CatNo,
        TagField::Year => TagName::Year,
        TagField::Bpm => TagName::Bpm,
        TagField::Key => TagName::Key,
        TagField::TrackNumber => TagName::TrackNumber,
        TagField::Comment => TagName::Comment,
    }
}

fn staged_field(diff: FieldDiff) -> StagedField {
    let changes = diff.changes();
    let proposal = diff.proposal.as_ref();
    StagedField {
        field: tag_name(diff.field),
        current_provenance: diff.current_provenance.map(|provenance| match provenance {
            Provenance::File => TagProvenance::File,
            Provenance::User => TagProvenance::User,
        }),
        proposed_value: proposal.map(|stored| stored.proposal.value.clone()),
        provider: proposal.map(|stored| stored.proposal.provider.clone()),
        confidence: proposal.map(|stored| stored.proposal.confidence),
        audio_matched: proposal.is_some_and(|stored| stored.proposal.audio_matched),
        status: proposal.map(|stored| match stored.status {
            ProposalStatus::Pending => ProposalState::Pending,
            ProposalStatus::Accepted => ProposalState::Accepted,
            ProposalStatus::Skipped => ProposalState::Skipped,
        }),
        locked_by_user: diff.locked_by_user,
        changes,
        current_value: diff.current_value,
    }
}

fn fields_of(names: &[TagName]) -> Vec<TagField> {
    names.iter().copied().map(tag_field).collect()
}

fn diff_of(library: &Database, track_id: i64) -> Result<TrackStagingDiff, ApiError> {
    let layers = library
        .tag_layers(track_id)
        .map_err(ApiError::library_unreadable)?
        .ok_or_else(|| {
            ApiError::invalid_input(format!("Track {track_id} is no longer in the library."))
        })?;
    Ok(TrackStagingDiff {
        track_id,
        fields: staging_diff(&layers.file, &layers.user, &layers.proposals)
            .into_iter()
            .map(staged_field)
            .collect(),
    })
}

impl Api {
    pub(crate) fn locked_library(&self) -> MutexGuard<'_, Database> {
        self.core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn library_edited(&self) {
        self.core
            .watch_revisions
            .send_modify(|revision| *revision += 1);
    }

    pub(crate) fn staging_queue(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<StagingQueuePage, ApiError> {
        let library = self.locked_library();
        let entries = library
            .staging_queue(offset, limit.min(MAX_QUEUE_PAGE))
            .map_err(ApiError::library_unreadable)?;
        Ok(StagingQueuePage {
            total: library
                .staging_queue_count()
                .map_err(ApiError::library_unreadable)?,
            offset,
            items: entries
                .into_iter()
                .map(|entry| StagingQueueItem {
                    track_id: entry.track_id,
                    pending_fields: entry.pending_fields,
                    confidence: entry.confidence,
                    audio_matched: entry.audio_matched,
                })
                .collect(),
        })
    }

    pub(crate) fn staging_diff(&self, track_id: i64) -> Result<TrackStagingDiff, ApiError> {
        diff_of(&self.locked_library(), track_id)
    }

    pub(crate) fn accept_proposed_fields(
        &self,
        track_id: i64,
        fields: &[TagName],
    ) -> Result<TrackStagingDiff, ApiError> {
        let mut library = self.locked_library();
        let accepted = library
            .accept_proposals(track_id, &fields_of(fields))
            .map_err(ApiError::library_unreadable)?;
        let diff = diff_of(&library, track_id)?;
        drop(library);
        if accepted > 0 {
            self.library_edited();
        }
        Ok(diff)
    }

    pub(crate) fn accept_all_above_threshold(
        &self,
        min_confidence: f64,
    ) -> Result<AcceptedProposals, ApiError> {
        if !(0.0..=1.0).contains(&min_confidence) {
            return Err(ApiError::invalid_input(format!(
                "A confidence threshold runs from 0 to 1, and {min_confidence} is outside it."
            )));
        }
        let report = self
            .locked_library()
            .accept_above_threshold(min_confidence)
            .map_err(ApiError::library_unreadable)?;
        if report.fields > 0 {
            self.library_edited();
        }
        Ok(AcceptedProposals {
            tracks: report.tracks,
            fields: report.fields,
        })
    }

    pub(crate) fn skip_proposal(
        &self,
        track_id: i64,
        fields: &[TagName],
    ) -> Result<TrackStagingDiff, ApiError> {
        let mut library = self.locked_library();
        library
            .skip_proposals(track_id, &fields_of(fields))
            .map_err(ApiError::library_unreadable)?;
        diff_of(&library, track_id)
    }

    pub(crate) fn set_user_tag(
        &self,
        track_ids: &[i64],
        edits: &[UserTagEdit],
    ) -> Result<UserTagReport, ApiError> {
        let edits = edits
            .iter()
            .map(|edit| {
                let field = tag_field(edit.field);
                let value = edit
                    .value
                    .as_deref()
                    .map(|value| normalise_value(field, value))
                    .transpose()
                    .map_err(ApiError::invalid_input)?;
                Ok((field, value))
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        let tracks = self
            .locked_library()
            .set_user_tags(track_ids, &edits)
            .map_err(ApiError::library_unreadable)?;
        if tracks > 0 {
            self.library_edited();
        }
        Ok(UserTagReport { tracks })
    }
}
