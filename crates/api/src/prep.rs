//! The Prep queue and batch tagging commands.

use std::sync::atomic::AtomicBool;

use resonance_core::library::TagField;
use resonance_core::prep::{TextCase, release_group};
use resonance_core::staging::{normalise_value, staging_diff};
use resonance_db::{BatchTagOperation, Database, TrackRow};
use resonance_identify::library::run_local_pass;
use resonance_library::write_tags::preview_tag_writes;

use crate::artwork::artwork_of;
use crate::command::{
    AcceptedProposals, BatchTagTool, PrepItem, PrepQueueSnapshot, PrepReleaseGroup, StagedField,
    TagName, TextCaseName, TrackQuery,
};
use crate::error::ApiError;
use crate::staging::{fields_of, staged_field, tag_field};
use crate::tag_writes::journal_operation;
use crate::{Api, core_track_query, library_track};

const MAX_PREP_ITEMS: u32 = 500;

fn convert_batch_tool(tool: BatchTagTool) -> Result<BatchTagOperation, ApiError> {
    match tool {
        BatchTagTool::SetFields { edits } => {
            let edits = edits
                .into_iter()
                .map(|edit| {
                    let field = tag_field(edit.field);
                    let value = edit
                        .value
                        .as_deref()
                        .map(|raw| normalise_value(field, raw))
                        .transpose()
                        .map_err(ApiError::invalid_input)?;
                    Ok((field, value))
                })
                .collect::<Result<Vec<_>, ApiError>>()?;
            Ok(BatchTagOperation::SetFields { edits })
        }
        BatchTagTool::FindAndReplace {
            fields,
            find,
            replace,
            case_sensitive,
        } => {
            if find.is_empty() {
                return Err(ApiError::invalid_input(
                    "Enter the text to find before replacing.",
                ));
            }
            Ok(BatchTagOperation::FindAndReplace {
                fields: fields_of(&fields),
                find,
                replace,
                case_sensitive,
            })
        }
        BatchTagTool::FixCase { fields, text_case } => Ok(BatchTagOperation::FixCase {
            fields: fields_of(&fields),
            case: match text_case {
                TextCaseName::Title => TextCase::Title,
                TextCaseName::Sentence => TextCase::Sentence,
                TextCaseName::Upper => TextCase::Upper,
            },
        }),
        BatchTagTool::ApplyReleaseFields {
            source_track_id,
            fields,
        } => Ok(BatchTagOperation::ApplyReleaseFields {
            source_track_id,
            fields: fields_of(&fields),
        }),
    }
}

fn build_prep_item(library: &Database, row: &TrackRow) -> Result<Option<PrepItem>, ApiError> {
    let Some(layers) = library
        .tag_layers(row.track_id)
        .map_err(ApiError::library_unreadable)?
    else {
        return Ok(None);
    };
    let diffs = staging_diff(&layers.file, &layers.user, &layers.proposals);
    let fields: Vec<StagedField> = diffs.into_iter().map(staged_field).collect();
    let artwork = artwork_of(library, row.track_id)?;

    let mut pending_fields = 0u32;
    let mut best_confidence = 0.0f64;
    let mut best_provider: Option<String> = None;
    let mut audio_matched = false;
    let mut changeable_confidences = Vec::new();

    for field in &fields {
        if field.changes {
            pending_fields += 1;
            let conf = field.confidence.unwrap_or(0.0);
            if !field.locked_by_user {
                changeable_confidences.push(conf);
            }
            if conf >= best_confidence {
                best_confidence = conf;
                best_provider.clone_from(&field.provider);
            }
            if field.audio_matched {
                audio_matched = true;
            }
        }
    }

    if artwork.proposed.is_some() {
        pending_fields += 1;
        if best_provider.is_none() {
            best_provider = artwork.proposed.as_ref().map(|p| p.provider.clone());
            best_confidence = best_confidence.max(0.92);
        }
    }

    let stored_id = library
        .identification(row.track_id)
        .map_err(ApiError::library_unreadable)?;
    let needs_url = stored_id
        .as_ref()
        .is_some_and(|id| id.steps_json.contains("\"needs_url\""))
        && pending_fields == 0;

    let field_value = |wanted: TagName| -> Option<&str> {
        fields.iter().find(|f| f.field == wanted).and_then(|f| {
            if f.changes && !f.locked_by_user {
                f.proposed_value.as_deref().or(f.current_value.as_deref())
            } else {
                f.current_value.as_deref().or(f.proposed_value.as_deref())
            }
        })
    };

    let release = release_group(
        &row.file.path,
        field_value(TagName::Album),
        field_value(TagName::Label),
        field_value(TagName::CatNo),
        field_value(TagName::Year),
    )
    .map(|group| PrepReleaseGroup {
        key: group.key,
        title: group.title,
        subtitle: group.subtitle,
    });

    Ok(Some(PrepItem {
        track: library_track(row),
        fields,
        artwork,
        pending_fields,
        provider: best_provider,
        confidence: best_confidence,
        audio_matched,
        needs_url,
        changeable_confidences,
        release,
    }))
}

impl Api {
    pub(crate) fn prep_queue(
        &self,
        query: Option<TrackQuery>,
        proposals_only: bool,
    ) -> Result<PrepQueueSnapshot, ApiError> {
        let _ = run_local_pass(&self.core.library, &AtomicBool::new(false));
        let library = self.locked_library();

        let rows: Vec<TrackRow> = if let Some(track_query) = query {
            let core_query = core_track_query(Some(track_query))?;
            library
                .query_tracks_page(&core_query, 0, MAX_PREP_ITEMS)
                .map_err(ApiError::library_unreadable)?
        } else if proposals_only {
            let ids = library
                .pending_prep_track_ids(MAX_PREP_ITEMS)
                .map_err(ApiError::library_unreadable)?;
            let mut found = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(row) = library
                    .track_by_id(id)
                    .map_err(ApiError::library_unreadable)?
                {
                    found.push(row);
                }
            }
            found
        } else {
            let core_query = core_track_query(None)?;
            library
                .query_tracks_page(&core_query, 0, MAX_PREP_ITEMS)
                .map_err(ApiError::library_unreadable)?
        };

        let mut items = Vec::with_capacity(rows.len());
        for row in &rows {
            if let Some(item) = build_prep_item(&library, row)? {
                if proposals_only && item.pending_fields == 0 && !item.needs_url {
                    continue;
                }
                items.push(item);
            }
        }

        let unwritten_tracks = preview_tag_writes(&library, &[])
            .map_err(ApiError::library_unreadable)?
            .into_iter()
            .filter(|file| file.blocked.is_none())
            .count() as u32;
        let last_operation = library
            .recent_operations(10)
            .map_err(ApiError::library_unreadable)?
            .into_iter()
            .map(journal_operation)
            .find(|op| op.can_undo);

        Ok(PrepQueueSnapshot {
            items,
            unwritten_tracks,
            last_operation,
        })
    }

    pub(crate) fn accept_prep_items(
        &self,
        track_ids: &[i64],
        fields: &[TagName],
    ) -> Result<AcceptedProposals, ApiError> {
        let mut library = self.locked_library();
        let target_fields: Vec<TagField> = fields_of(fields);
        let mut report = library
            .accept_proposals_for_tracks(track_ids, &target_fields)
            .map_err(ApiError::library_unreadable)?;
        if target_fields.is_empty() {
            for &track_id in track_ids {
                if library
                    .accept_proposed_artwork(track_id)
                    .map_err(ApiError::library_unreadable)?
                {
                    report.fields += 1;
                    if report.tracks == 0 {
                        report.tracks = 1;
                    }
                }
            }
        }
        drop(library);
        if report.fields > 0 {
            self.library_edited();
        }
        Ok(AcceptedProposals {
            tracks: report.tracks,
            fields: report.fields,
        })
    }

    pub(crate) fn skip_prep_items(
        &self,
        track_ids: &[i64],
        fields: &[TagName],
    ) -> Result<AcceptedProposals, ApiError> {
        let mut library = self.locked_library();
        let target_fields = fields_of(fields);
        let mut skipped = library
            .skip_proposals_for_tracks(track_ids, &target_fields)
            .map_err(ApiError::library_unreadable)?;
        if target_fields.is_empty() {
            for &track_id in track_ids {
                if library
                    .skip_proposed_artwork(track_id)
                    .map_err(ApiError::library_unreadable)?
                {
                    skipped += 1;
                }
            }
        }
        drop(library);
        Ok(AcceptedProposals {
            tracks: track_ids.len() as u32,
            fields: skipped,
        })
    }

    pub(crate) fn preview_batch_tag(
        &self,
        track_ids: &[i64],
        tool: BatchTagTool,
    ) -> Result<AcceptedProposals, ApiError> {
        let op = convert_batch_tool(tool)?;
        let report = self
            .locked_library()
            .run_batch_tag_operation(track_ids, &op, true)
            .map_err(ApiError::library_unreadable)?;
        Ok(AcceptedProposals {
            tracks: report.tracks,
            fields: report.fields,
        })
    }

    pub(crate) fn apply_batch_tag(
        &self,
        track_ids: &[i64],
        tool: BatchTagTool,
    ) -> Result<AcceptedProposals, ApiError> {
        let op = convert_batch_tool(tool)?;
        let report = self
            .locked_library()
            .run_batch_tag_operation(track_ids, &op, false)
            .map_err(ApiError::library_unreadable)?;
        if report.fields > 0 {
            self.library_edited();
        }
        Ok(AcceptedProposals {
            tracks: report.tracks,
            fields: report.fields,
        })
    }
}
