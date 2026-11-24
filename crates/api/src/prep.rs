//! The Prep queue and batch tagging commands.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use resonance_core::identify::{
    Candidate, RankedCandidate, has_online_attempt, is_viable_alternative, should_expand_candidates,
};
use resonance_core::library::TagField;
use resonance_core::prep::{TextCase, release_group};
use resonance_core::staging::{dirty_file_fields, normalise_value, staging_diff};
use resonance_db::{
    BatchTagOperation, CachedArtworkSource, Database, IdentificationPass, StoredIdentification,
    TrackArtworkState, TrackRow,
};
use resonance_identify::library::{
    parse_match_evidence, parse_ranked_candidates, parse_step_reports, preview_candidate_artwork,
    run_local_pass_bounded, run_local_pass_for_tracks, select_candidate, select_reviewed_candidate,
    use_reviewed_candidate_artwork,
};
use resonance_identify::pipeline::StepReport;
use resonance_identify::provider::{CACHE_TTL, IdentifyError, display_name};

use crate::command::{
    AcceptedProposals, BatchTagTool, CandidateArtworkState, IdentifyStepOutcome,
    MatchCandidateSummary, PrepItem, PrepPageSnapshot, PrepQueueSnapshot, PrepReleaseGroup,
    ProposedTrackArtwork, StagedField, TagName, TextCaseName, TrackArtwork, TrackCandidateArtwork,
    TrackQuery,
};
use crate::error::ApiError;
use crate::online::{to_command_evidence, to_command_step_reports};
use crate::staging::{fields_of, staged_field, tag_field};
use crate::tag_writes::journal_operation;
use crate::{Api, core_track_query, library_track};

const MAX_PREP_ITEMS: u32 = 500;

fn map_identify_error(error: IdentifyError) -> ApiError {
    match error {
        IdentifyError::Database(resonance_db::DbError::DecisionChanged) => {
            ApiError::invalid_input(error)
        }
        IdentifyError::Database(source) => ApiError::library_unreadable(source),
        other => ApiError::internal(other),
    }
}

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

fn candidate_artwork_state_from_cache(
    index: u32,
    c: &Candidate,
    cache: &HashMap<String, CachedArtworkSource>,
) -> CandidateArtworkState {
    let Some(provider) = c.artwork_fetcher_provider() else {
        return CandidateArtworkState {
            index,
            has_artwork: false,
            artwork_provider: None,
            artwork_hash: None,
        };
    };
    let cached = c
        .artwork_url
        .as_deref()
        .and_then(|url| cache.get(url).cloned());
    match cached {
        Some(CachedArtworkSource::Stored(stored)) => CandidateArtworkState {
            index,
            has_artwork: true,
            artwork_provider: Some(display_name(provider).to_owned()),
            artwork_hash: Some(stored.hash),
        },
        Some(CachedArtworkSource::Unavailable) => CandidateArtworkState {
            index,
            has_artwork: false,
            artwork_provider: None,
            artwork_hash: None,
        },
        None => CandidateArtworkState {
            index,
            has_artwork: true,
            artwork_provider: Some(display_name(provider).to_owned()),
            artwork_hash: None,
        },
    }
}

struct ParsedIdentificationData {
    stored_id: Option<StoredIdentification>,
    ranked: Vec<RankedCandidate>,
    selected_candidate: u32,
    visible_indices: Vec<usize>,
    candidates_expanded: bool,
    step_reports: Vec<StepReport>,
}

fn prepare_identification_data(
    stored_id: Option<StoredIdentification>,
) -> ParsedIdentificationData {
    let ranked = stored_id
        .as_ref()
        .map(|id| parse_ranked_candidates(&id.candidates_json))
        .unwrap_or_default();
    let selected_candidate = stored_id.as_ref().map_or(0, |id| id.selected_candidate);
    let anchor = ranked.first();
    let visible_indices: Vec<usize> = ranked
        .iter()
        .enumerate()
        .filter(|(index, item)| {
            *index == 0
                || *index as u32 == selected_candidate
                || anchor.is_some_and(|top| is_viable_alternative(top, item))
        })
        .map(|(index, _)| index)
        .collect();
    let visible_slice: Vec<RankedCandidate> = visible_indices
        .iter()
        .map(|&idx| ranked[idx].clone())
        .collect();
    let candidates_expanded = should_expand_candidates(&visible_slice);
    let step_reports = stored_id
        .as_ref()
        .map(|id| parse_step_reports(&id.steps_json))
        .unwrap_or_default();
    ParsedIdentificationData {
        stored_id,
        ranked,
        selected_candidate,
        visible_indices,
        candidates_expanded,
        step_reports,
    }
}

fn build_prep_items(
    library: &Database,
    rows: &[TrackRow],
    filter_pending: bool,
) -> Result<Vec<PrepItem>, ApiError> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let track_ids: Vec<i64> = rows.iter().map(|row| row.track_id).collect();
    let mut layers_map = library
        .tag_layers_by_track_ids(&track_ids)
        .map_err(ApiError::library_unreadable)?;
    let mut artwork_map = library
        .artwork_states_by_track_ids(&track_ids)
        .map_err(ApiError::library_unreadable)?;
    let mut id_map = library
        .identifications_by_track_ids(&track_ids)
        .map_err(ApiError::library_unreadable)?;

    let mut parsed_by_track: HashMap<i64, ParsedIdentificationData> =
        HashMap::with_capacity(track_ids.len());
    let mut artwork_urls: Vec<String> = Vec::new();
    for &track_id in &track_ids {
        let data = prepare_identification_data(id_map.remove(&track_id));
        for &idx in &data.visible_indices {
            let candidate = &data.ranked[idx].candidate;
            if candidate.artwork_fetcher_provider().is_some()
                && let Some(url) = &candidate.artwork_url
            {
                artwork_urls.push(url.clone());
            }
        }
        parsed_by_track.insert(track_id, data);
    }

    let url_refs: Vec<&str> = artwork_urls.iter().map(String::as_str).collect();
    let artwork_cache = library
        .cached_artwork_sources(&url_refs, CACHE_TTL)
        .map_err(ApiError::library_unreadable)?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(layers) = layers_map.remove(&row.track_id) else {
            continue;
        };
        let dirty_fields = dirty_file_fields(&layers.file, &layers.dirty_user);
        let diffs = staging_diff(&layers.file, &layers.user, &layers.proposals);
        let fields: Vec<StagedField> = diffs
            .into_iter()
            .map(|diff| staged_field(diff, &dirty_fields))
            .collect();
        let artwork_state: TrackArtworkState =
            artwork_map.remove(&row.track_id).unwrap_or_default();
        let parsed = parsed_by_track
            .remove(&row.track_id)
            .unwrap_or_else(|| prepare_identification_data(None));
        let stored_id = parsed.stored_id;
        let review_token = layers.review_token(row.track_id, stored_id.as_ref(), &artwork_state);
        let artwork = TrackArtwork {
            track_id: row.track_id,
            artwork: artwork_state.shown.map(|art| art.hash),
            proposed: artwork_state
                .proposed
                .filter(|proposed| proposed.pending)
                .map(|proposed| ProposedTrackArtwork {
                    artwork: proposed.artwork.hash,
                    provider: display_name(&proposed.provider).to_owned(),
                }),
            dirty_on_disk: artwork_state.dirty_on_disk,
        };
        let has_unwritten = fields.iter().any(|f| f.dirty_on_disk) || artwork.dirty_on_disk;

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

        if let Some(proposed_art) = &artwork.proposed {
            pending_fields += 1;
            if best_provider.is_none() {
                best_provider = Some(proposed_art.provider.clone());
                best_confidence = best_confidence.max(0.92);
            }
        }

        let selected_candidate = parsed.selected_candidate;
        let step_kinds: Vec<resonance_core::identify::Step> = parsed
            .step_reports
            .iter()
            .map(|report| report.step)
            .collect();
        let online_checked =
            stored_id.as_ref().is_some_and(|id| id.online) || has_online_attempt(&step_kinds);
        let steps = to_command_step_reports(&parsed.step_reports);
        let has_online_match = stored_id
            .as_ref()
            .and_then(|id| id.best_json.as_ref())
            .is_some();
        let needs_url = !has_online_match
            && steps
                .iter()
                .any(|step| matches!(step.outcome, IdentifyStepOutcome::NeedsUrl));
        let candidates: Vec<MatchCandidateSummary> = parsed
            .visible_indices
            .iter()
            .map(|&index| {
                let item = &parsed.ranked[index];
                let c = &item.candidate;
                let art = candidate_artwork_state_from_cache(index as u32, c, &artwork_cache);
                MatchCandidateSummary {
                    index: index as u32,
                    provider: c.provider.clone(),
                    providers: item.providers.clone(),
                    confidence: item.confidence,
                    audio_matched: item.audio_matched,
                    artist: c.artist.clone(),
                    title: c.title.clone(),
                    mix: c.mix.clone(),
                    album: c.album.clone(),
                    label: c.label.clone(),
                    catno: c.catno.clone(),
                    year: c.year,
                    genre: c.genre.clone(),
                    subgenre: c.subgenre.clone(),
                    track_number: c.track_number.clone(),
                    duration_delta_ms: item.duration_delta_ms,
                    url: c.url.clone(),
                    has_artwork: art.has_artwork,
                    artwork_provider: art.artwork_provider,
                    artwork_hash: art.artwork_hash,
                    evidence: to_command_evidence(&item.evidence),
                }
            })
            .collect();
        if let Some(selected) = candidates.iter().find(|c| c.index == selected_candidate) {
            best_confidence = selected.confidence;
            best_provider = Some(selected.provider.clone());
            audio_matched = selected.audio_matched;
        }
        let evidence = candidates
            .iter()
            .find(|c| c.index == selected_candidate)
            .map(|c| c.evidence.clone())
            .or_else(|| {
                stored_id
                    .as_ref()
                    .and_then(|id| id.best_json.as_deref())
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                    .and_then(|v| parse_match_evidence(&v))
                    .map(|e| to_command_evidence(&e))
            });

        if filter_pending && pending_fields == 0 && !needs_url {
            continue;
        }

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

        items.push(PrepItem {
            track: library_track(row),
            review_token,
            fields,
            artwork,
            pending_fields,
            provider: best_provider,
            confidence: best_confidence,
            audio_matched,
            needs_url,
            changeable_confidences,
            release,
            evidence,
            candidates,
            selected_candidate,
            candidates_expanded: parsed.candidates_expanded,
            steps,
            has_unwritten,
            online_checked,
        });
    }

    Ok(items)
}

impl Api {
    pub(crate) fn prep_track_ids(
        &self,
        query: Option<TrackQuery>,
        track_ids: Option<Vec<i64>>,
    ) -> Result<Vec<i64>, ApiError> {
        let query = core_track_query(query)?;
        if track_ids.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }
        let mut ids = self
            .locked_library()
            .prep_track_ids(&query)
            .map_err(ApiError::library_unreadable)?;
        if let Some(selected) = track_ids {
            let selected: HashSet<_> = selected.into_iter().collect();
            ids.retain(|id| selected.contains(id));
        }
        Ok(ids)
    }

    fn finish_prep_queue(
        &self,
        library: &Database,
        rows: &[TrackRow],
        filter_pending: bool,
    ) -> Result<PrepQueueSnapshot, ApiError> {
        let items = build_prep_items(library, rows, filter_pending)?;
        let unwritten_tracks = library
            .count_unwritten_tracks()
            .map_err(ApiError::library_unreadable)?;
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

    pub(crate) fn prep_page(
        &self,
        query: Option<TrackQuery>,
        track_ids: Option<Vec<i64>>,
        offset: u32,
        limit: u32,
    ) -> Result<PrepPageSnapshot, ApiError> {
        let limit = limit.clamp(1, MAX_PREP_ITEMS);
        let core_query = core_track_query(query)?;
        let (total, page_ids) = {
            let library = self.locked_library();
            if let Some(selected) = track_ids {
                let mut seen = HashSet::with_capacity(selected.len());
                let unique: Vec<i64> = selected.into_iter().filter(|id| seen.insert(*id)).collect();
                let ordered = if unique.is_empty() {
                    Vec::new()
                } else if core_query.is_narrowed() {
                    let allowed: HashSet<i64> = unique.into_iter().collect();
                    let mut all = library
                        .prep_track_ids(&core_query)
                        .map_err(ApiError::library_unreadable)?;
                    all.retain(|id| allowed.contains(id));
                    all
                } else {
                    unique
                };
                let total = ordered.len() as u32;
                let start = (offset as usize).min(ordered.len());
                let end = start.saturating_add(limit as usize).min(ordered.len());
                (total, ordered[start..end].to_vec())
            } else {
                library
                    .prep_track_page(&core_query, offset, limit)
                    .map_err(ApiError::library_unreadable)?
            }
        };

        if self.core.local_pass_dirty.load(Ordering::Acquire) {
            if !page_ids.is_empty() {
                let _ = run_local_pass_for_tracks(
                    &self.core.library,
                    &page_ids,
                    &AtomicBool::new(false),
                );
            }
            if !self
                .locked_library()
                .has_tracks_needing_identification(IdentificationPass::Local)
                .map_err(ApiError::library_unreadable)?
            {
                self.core.local_pass_dirty.store(false, Ordering::Release);
            }
        }

        let library = self.locked_library();
        let rows: Vec<TrackRow> = if page_ids.is_empty() {
            Vec::new()
        } else {
            let mut by_id = library
                .tracks_by_ids(&page_ids)
                .map_err(ApiError::library_unreadable)?;
            page_ids.iter().filter_map(|id| by_id.remove(id)).collect()
        };
        let snapshot = self.finish_prep_queue(&library, &rows, false)?;
        Ok(PrepPageSnapshot {
            total,
            offset,
            items: snapshot.items,
            unwritten_tracks: snapshot.unwritten_tracks,
            last_operation: snapshot.last_operation,
        })
    }

    pub(crate) fn prep_queue(
        &self,
        query: Option<TrackQuery>,
        proposals_only: bool,
        track_ids: Option<Vec<i64>>,
    ) -> Result<PrepQueueSnapshot, ApiError> {
        let explicit_ids = track_ids.is_some();
        let ordered_explicit_ids: Option<Vec<i64>> = track_ids.map(|ids| {
            let mut seen = HashSet::new();
            let mut ordered = Vec::new();
            for id in ids {
                if !seen.insert(id) {
                    continue;
                }
                if ordered.len() >= MAX_PREP_ITEMS as usize {
                    break;
                }
                ordered.push(id);
            }
            ordered
        });

        let dirty_local = self.core.local_pass_dirty.load(Ordering::Acquire);
        if dirty_local {
            if let Some(ids) = ordered_explicit_ids.as_deref() {
                let _ = run_local_pass_for_tracks(&self.core.library, ids, &AtomicBool::new(false));
            } else if query.is_none()
                && proposals_only
                && let Ok((_, exhausted)) = run_local_pass_bounded(
                    &self.core.library,
                    MAX_PREP_ITEMS,
                    &AtomicBool::new(false),
                )
                && exhausted
            {
                self.core.local_pass_dirty.store(false, Ordering::Release);
            }
        }

        let library = self.locked_library();
        let rows: Vec<TrackRow> = if let Some(ordered_ids) = ordered_explicit_ids {
            let mut by_id = library
                .tracks_by_ids(&ordered_ids)
                .map_err(ApiError::library_unreadable)?;
            ordered_ids
                .into_iter()
                .filter_map(|id| by_id.remove(&id))
                .collect()
        } else if let Some(track_query) = query {
            let core_query = core_track_query(Some(track_query))?;
            library
                .query_tracks_page(&core_query, 0, MAX_PREP_ITEMS)
                .map_err(ApiError::library_unreadable)?
        } else if proposals_only {
            let ids = library
                .pending_prep_track_ids(MAX_PREP_ITEMS)
                .map_err(ApiError::library_unreadable)?;
            let mut by_id = library
                .tracks_by_ids(&ids)
                .map_err(ApiError::library_unreadable)?;
            ids.into_iter().filter_map(|id| by_id.remove(&id)).collect()
        } else {
            let core_query = core_track_query(None)?;
            library
                .query_tracks_page(&core_query, 0, MAX_PREP_ITEMS)
                .map_err(ApiError::library_unreadable)?
        };

        let filter_pending = !explicit_ids && proposals_only;
        if dirty_local && !explicit_ids && !rows.is_empty() {
            let row_ids: Vec<i64> = rows.iter().map(|row| row.track_id).collect();
            let existing = library
                .identifications_by_track_ids(&row_ids)
                .map_err(ApiError::library_unreadable)?;
            let missing_ids: Vec<i64> = row_ids
                .into_iter()
                .filter(|id| !existing.contains_key(id))
                .collect();
            if !missing_ids.is_empty() {
                drop(library);
                let _ = run_local_pass_for_tracks(
                    &self.core.library,
                    &missing_ids,
                    &AtomicBool::new(false),
                );
                let library = self.locked_library();
                if !library
                    .has_tracks_needing_identification(IdentificationPass::Local)
                    .map_err(ApiError::library_unreadable)?
                {
                    self.core.local_pass_dirty.store(false, Ordering::Release);
                }
                return self.finish_prep_queue(&library, &rows, filter_pending);
            }
        }
        if dirty_local
            && !library
                .has_tracks_needing_identification(IdentificationPass::Local)
                .map_err(ApiError::library_unreadable)?
        {
            self.core.local_pass_dirty.store(false, Ordering::Release);
        }

        self.finish_prep_queue(&library, &rows, filter_pending)
    }

    fn prep_item_for(&self, track_id: i64) -> Result<PrepItem, ApiError> {
        let library = self.locked_library();
        let row = library
            .track_by_id(track_id)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| ApiError::invalid_input(format!("Track {track_id} does not exist.")))?;
        build_prep_items(&library, &[row], false)?
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::invalid_input(format!("Track {track_id} does not exist.")))
    }

    pub(crate) fn select_prep_candidate(
        &self,
        track_id: i64,
        candidate_index: u32,
        expected_review: Option<&str>,
    ) -> Result<PrepItem, ApiError> {
        let pipeline = Arc::clone(
            &self
                .core
                .pipeline
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let switched = select_reviewed_candidate(
            &self.core.library,
            &pipeline,
            track_id,
            candidate_index,
            expected_review,
        )
        .map_err(map_identify_error)?;
        if switched.is_none() {
            return Err(ApiError::invalid_input(format!(
                "Candidate {candidate_index} is not available for track {track_id}."
            )));
        }
        self.prep_item_for(track_id)
    }

    pub(crate) fn use_prep_candidate_artwork(
        &self,
        track_id: i64,
        candidate_index: u32,
        accept: bool,
        expected_review: Option<&str>,
    ) -> Result<PrepItem, ApiError> {
        let pipeline = Arc::clone(
            &self
                .core
                .pipeline
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let resolved = use_reviewed_candidate_artwork(
            &self.core.library,
            &pipeline,
            track_id,
            candidate_index,
            accept,
            expected_review,
        )
        .map_err(map_identify_error)?;
        if resolved.is_none() {
            return Err(ApiError::invalid_input(
                "That match does not include a downloadable release cover.",
            ));
        }
        if accept {
            self.library_edited();
        }
        self.prep_item_for(track_id)
    }

    pub(crate) fn fetch_prep_candidate_artwork(
        &self,
        track_id: i64,
        candidate_index: u32,
    ) -> Result<TrackCandidateArtwork, ApiError> {
        let pipeline = Arc::clone(
            &self
                .core
                .pipeline
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        preview_candidate_artwork(&self.core.library, &pipeline, track_id, candidate_index)
            .map_err(map_identify_error)?;
        let library = self.locked_library();
        let _row = library
            .track_by_id(track_id)
            .map_err(ApiError::library_unreadable)?
            .ok_or_else(|| ApiError::invalid_input(format!("Track {track_id} does not exist.")))?;
        let ranked = library
            .identification(track_id)
            .map_err(ApiError::library_unreadable)?
            .map(|id| parse_ranked_candidates(&id.candidates_json))
            .unwrap_or_default();
        let urls: Vec<&str> = ranked
            .iter()
            .filter_map(|item| {
                item.candidate
                    .artwork_fetcher_provider()
                    .and(item.candidate.artwork_url.as_deref())
            })
            .collect();
        let artwork_cache = library
            .cached_artwork_sources(&urls, CACHE_TTL)
            .map_err(ApiError::library_unreadable)?;
        let candidates = ranked
            .iter()
            .enumerate()
            .map(|(index, item)| {
                candidate_artwork_state_from_cache(index as u32, &item.candidate, &artwork_cache)
            })
            .collect();
        Ok(TrackCandidateArtwork {
            track_id,
            candidates,
        })
    }

    pub(crate) fn accept_prep_items(
        &self,
        track_ids: &[i64],
        fields: &[TagName],
        candidate_index: Option<u32>,
        skip_remaining: bool,
        include_artwork: Option<bool>,
    ) -> Result<AcceptedProposals, ApiError> {
        if let Some(index) = candidate_index {
            if track_ids.len() != 1 {
                return Err(ApiError::invalid_input(
                    "Selecting a candidate requires a single track.",
                ));
            }
            let track_id = track_ids[0];
            let current_selected = self
                .locked_library()
                .identification(track_id)
                .map_err(ApiError::library_unreadable)?
                .map(|id| id.selected_candidate);
            if current_selected != Some(index) {
                let pipeline = Arc::clone(
                    &self
                        .core
                        .pipeline
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                );
                let switched = select_candidate(&self.core.library, &pipeline, track_id, index)
                    .map_err(map_identify_error)?;
                if switched.is_none() {
                    return Err(ApiError::invalid_input(format!(
                        "Candidate {index} is not available for track {track_id}."
                    )));
                }
            }
        }
        let mut library = self.locked_library();
        let target_fields: Vec<TagField> = fields_of(fields);
        let report = library
            .accept_prep_proposals(track_ids, &target_fields, skip_remaining, include_artwork)
            .map_err(ApiError::library_unreadable)?;
        drop(library);
        if report.fields > 0 || skip_remaining {
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
        if skipped > 0 {
            self.library_edited();
        }
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
