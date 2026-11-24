//! Identification over the library database: the local pass on every track that has no result,
//! then the online pass, tracks the user is looking at first. Each batch is stored as soon as it
//! is done, so a run stopped by a restart resumes where it was.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use resonance_core::identify::{RankedCandidate, ReleaseUrl};
use resonance_core::prep::local_proposals;
use resonance_core::staging::proposals_from;
use resonance_db::{
    ArtworkError, CachedArtworkSource, Database, IdentificationInput, IdentificationPass,
    StoredArtwork,
};

use crate::pipeline::{Identification, Pipeline, identify_locally};
use crate::provider::IdentifyError;
use crate::store::{ranked_candidate_json, stored};

const LOCAL_BATCH: u32 = 500;
const ONLINE_BATCH_SIZE: u32 = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OnlineReport {
    pub looked_up: u64,
    pub confident: u64,
    pub needs_url: u64,
    /// Tracks with a step that failed or lacked its free key, which the next run tries again.
    pub incomplete: u64,
    pub network_requests: u32,
    pub cached_answers: u32,
}

/// Steps 1 and 2 for every track with no result yet. Returns how many were read.
pub fn run_local_pass(
    database: &Mutex<Database>,
    abort: &AtomicBool,
) -> Result<u64, IdentifyError> {
    run_local_pass_with_hook(database, abort, || {})
}

/// Steps 1 and 2 across all pending tracks, invoking `between_batches` after each 500-track batch
/// so background jobs can check Mix pause state without blocking until the end of a large sweep.
pub fn run_local_pass_with_hook(
    database: &Mutex<Database>,
    abort: &AtomicBool,
    mut between_batches: impl FnMut(),
) -> Result<u64, IdentifyError> {
    let mut after = 0;
    let mut done = 0;
    loop {
        if abort.load(Ordering::Acquire) {
            return Ok(done);
        }
        let inputs = {
            let database = database.lock().unwrap_or_else(PoisonError::into_inner);
            database.identification_inputs(IdentificationPass::Local, after, LOCAL_BATCH)?
        };
        let Some(last) = inputs.last() else {
            return Ok(done);
        };
        after = last.track_id;
        let count = inputs.len() as u64;
        let items: Vec<_> = inputs
            .into_iter()
            .map(|input| {
                let id_row = stored(&identify_locally(&input), false);
                let proposals = local_proposals(&input.tags, &input.path);
                (id_row, input.tags, proposals)
            })
            .collect();
        database
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .record_local_identifications_batch(&items)?;
        done += count;
        between_batches();
    }
}

/// Runs steps 1 and 2 for at most `max_tracks` tracks needing a local result, returning `(done,
/// exhausted)` so UI endpoints never block on a full 100k sweep.
pub fn run_local_pass_bounded(
    database: &Mutex<Database>,
    max_tracks: u32,
    abort: &AtomicBool,
) -> Result<(u64, bool), IdentifyError> {
    if max_tracks == 0 || abort.load(Ordering::Acquire) {
        return Ok((0, false));
    }
    let mut after = 0;
    let mut done = 0u64;
    loop {
        if abort.load(Ordering::Acquire) {
            return Ok((done, false));
        }
        let remaining = max_tracks.saturating_sub(done as u32);
        if remaining == 0 {
            return Ok((done, false));
        }
        let limit = LOCAL_BATCH.min(remaining);
        let inputs = {
            let database = database.lock().unwrap_or_else(PoisonError::into_inner);
            database.identification_inputs(IdentificationPass::Local, after, limit)?
        };
        let fetched = inputs.len() as u32;
        let Some(last) = inputs.last() else {
            return Ok((done, true));
        };
        after = last.track_id;
        let items: Vec<_> = inputs
            .into_iter()
            .map(|input| {
                let id_row = stored(&identify_locally(&input), false);
                let proposals = local_proposals(&input.tags, &input.path);
                (id_row, input.tags, proposals)
            })
            .collect();
        database
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .record_local_identifications_batch(&items)?;
        done += u64::from(fetched);
        if fetched < limit {
            return Ok((done, true));
        }
    }
}

/// Runs steps 1 and 2 strictly for the given `track_ids` that lack any stored identification row.
pub fn run_local_pass_for_tracks(
    database: &Mutex<Database>,
    track_ids: &[i64],
    abort: &AtomicBool,
) -> Result<u64, IdentifyError> {
    if track_ids.is_empty() || abort.load(Ordering::Acquire) {
        return Ok(0);
    }
    let inputs = database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .identification_inputs_for_tracks(IdentificationPass::Local, track_ids)?;
    if inputs.is_empty() || abort.load(Ordering::Acquire) {
        return Ok(0);
    }
    let count = inputs.len() as u64;
    let items: Vec<_> = inputs
        .into_iter()
        .map(|input| {
            let id_row = stored(&identify_locally(&input), false);
            let proposals = local_proposals(&input.tags, &input.path);
            (id_row, input.tags, proposals)
        })
        .collect();
    database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .record_local_identifications_batch(&items)?;
    Ok(count)
}

pub fn count_online_pending(database: &Mutex<Database>) -> Result<u64, IdentifyError> {
    Ok(database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .count_tracks_needing_identification(IdentificationPass::Online)?)
}

/// Steps 1 to 6 for every track without an online result, `priority` first.
pub fn run_online_pass(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    priority: &[i64],
    abort: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<OnlineReport, IdentifyError> {
    run_pass_inner(database, pipeline, priority, false, abort, progress)
}

/// Explicitly re-runs identification for specific `track_ids` even if already identified online.
pub fn reidentify_tracks(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_ids: &[i64],
    abort: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<OnlineReport, IdentifyError> {
    run_pass_inner(database, pipeline, track_ids, true, abort, progress)
}

fn run_pass_inner(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    priority: &[i64],
    force_priority_any: bool,
    abort: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<OnlineReport, IdentifyError> {
    let mut unique_seen = HashSet::new();
    let mut queue: Vec<i64> = priority
        .iter()
        .copied()
        .filter(|id| unique_seen.insert(*id))
        .collect();
    let batch_size: u32 = if force_priority_any && queue.len() <= 5 {
        1
    } else {
        ONLINE_BATCH_SIZE
    };
    let mut total = if force_priority_any {
        queue.len() as u64
    } else {
        count_online_pending(database)?
    };
    if force_priority_any {
        run_local_pass_for_tracks(database, priority, abort)?;
    } else {
        run_local_pass_with_hook(database, abort, || progress(0, total))?;
    }
    let mut report = OnlineReport::default();
    let mut seen = HashSet::new();
    queue.reverse();
    let mut after = 0;
    let mut exhausted = force_priority_any;
    let start = pipeline.fetch_counts();
    progress(0, total);

    loop {
        if abort.load(Ordering::Acquire) {
            break;
        }
        let batch = {
            let database = database.lock().unwrap_or_else(PoisonError::into_inner);
            let mut batch = Vec::new();
            let priority_pass = if force_priority_any {
                IdentificationPass::Any
            } else {
                IdentificationPass::Online
            };
            while batch.len() < batch_size as usize && !queue.is_empty() {
                let mut candidate_ids = Vec::with_capacity(batch_size as usize - batch.len());
                while candidate_ids.len() < batch_size as usize - batch.len()
                    && let Some(track_id) = queue.pop()
                {
                    if seen.insert(track_id) {
                        candidate_ids.push(track_id);
                    }
                }
                if candidate_ids.is_empty() {
                    break;
                }
                batch.extend(
                    database.identification_inputs_for_tracks(priority_pass, &candidate_ids)?,
                );
            }
            if !force_priority_any && batch.len() < batch_size as usize {
                let wanted = batch_size - batch.len() as u32;
                let inputs =
                    database.identification_inputs(IdentificationPass::Online, after, wanted)?;
                exhausted = inputs.is_empty();
                for input in inputs {
                    after = input.track_id;
                    if seen.insert(input.track_id) {
                        batch.push(input);
                    }
                }
            }
            batch
        };
        if batch.is_empty() {
            if exhausted && queue.is_empty() {
                break;
            }
            continue;
        }

        let identified = pipeline.identify(&batch, abort);
        if abort.load(Ordering::Acquire) {
            break;
        }
        let input_by_track: std::collections::HashMap<i64, &IdentificationInput> =
            batch.iter().map(|item| (item.track_id, item)).collect();
        let mut records = Vec::with_capacity(identified.len());
        for identification in &identified {
            if let Some(input) = input_by_track.get(&identification.track_id) {
                let (proposals, clear_pending_artwork) = match &identification.best {
                    Some(best) => (proposals_from(best), false),
                    None => (local_proposals(&input.tags, &input.path), true),
                };
                records.push(resonance_db::IdentificationBatchItem {
                    result: stored(identification, identification.is_complete()),
                    file_tags: input.tags.clone(),
                    proposals,
                    clear_pending_artwork,
                });
            }
            report.looked_up += 1;
            report.confident += u64::from(identification.is_confident());
            report.needs_url += u64::from(identification.needs_url());
            report.incomplete += u64::from(!identification.is_complete());
        }
        database
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .record_online_identifications_batch(&records)?;
        for identification in &identified {
            if abort.load(Ordering::Acquire) {
                break;
            }
            propose_artwork(database, pipeline, identification)?;
        }
        total = total.max(report.looked_up);
        progress(report.looked_up, total);
    }
    let end = pipeline.fetch_counts();
    report.network_requests = end.network - start.network;
    report.cached_answers = end.cached - start.cached;
    Ok(report)
}

/// Step 6 for one track, stored over any earlier result.
pub fn identify_track_with_url(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    url: &ReleaseUrl,
) -> Result<Option<Identification>, IdentifyError> {
    let input = database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .identification_input(IdentificationPass::Any, track_id)?;
    let Some(input) = input else {
        return Ok(None);
    };
    let identification = pipeline.identify_with_url(&input, url);
    record(
        &mut database.lock().unwrap_or_else(PoisonError::into_inner),
        &input,
        &identification,
    )?;
    propose_artwork(database, pipeline, &identification)?;
    Ok(Some(identification))
}

/// Switches a track's active candidate among stored ranked alternatives, re-staging its pending
/// tag proposals and cover art without touching user-locked fields.
pub fn select_candidate(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    candidate_index: u32,
) -> Result<Option<RankedCandidate>, IdentifyError> {
    select_reviewed_candidate(database, pipeline, track_id, candidate_index, None)
}

/// Switches a track's active candidate only when the reviewed state matches `expected_review`.
pub fn select_reviewed_candidate(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    candidate_index: u32,
    expected_review: Option<&str>,
) -> Result<Option<RankedCandidate>, IdentifyError> {
    let chosen = {
        let mut db = database.lock().unwrap_or_else(PoisonError::into_inner);
        db.check_review(track_id, expected_review)?;
        let Some(stored_row) = db.identification(track_id)? else {
            return Ok(None);
        };
        let candidates = parse_ranked_candidates(&stored_row.candidates_json);
        let Some(chosen) = candidates.get(candidate_index as usize).cloned() else {
            return Ok(None);
        };
        let best_str = ranked_candidate_json(&chosen).to_string();
        db.set_selected_candidate(
            track_id,
            candidate_index,
            Some(&best_str),
            Some(chosen.confidence),
            chosen.audio_matched,
        )?;
        db.store_proposals(track_id, &proposals_from(&chosen))?;
        // The last match's cover goes with it, even when the new match's cover cannot be fetched,
        // so a save never pairs one release's tags with another's picture.
        db.clear_pending_proposed_artwork(track_id)?;
        chosen
    };
    if chosen.candidate.artwork_url.is_some()
        && chosen.candidate.artwork_fetcher_provider().is_some()
    {
        let _ = propose_candidate_artwork_checked(
            database,
            pipeline,
            track_id,
            &chosen,
            false,
            Some(candidate_index),
        )?;
    }
    Ok(Some(chosen))
}

/// Stages or applies release artwork from a stored candidate without changing `selected_candidate`
/// or tag proposals. Returns the resolved artwork hash if that candidate has downloadable art.
pub fn use_candidate_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    candidate_index: u32,
    accept: bool,
) -> Result<Option<String>, IdentifyError> {
    use_reviewed_candidate_artwork(database, pipeline, track_id, candidate_index, accept, None)
}

/// Stages or applies candidate artwork only when the reviewed state matches `expected_review`.
pub fn use_reviewed_candidate_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    candidate_index: u32,
    accept: bool,
    expected_review: Option<&str>,
) -> Result<Option<String>, IdentifyError> {
    let source = {
        let db = database.lock().unwrap_or_else(PoisonError::into_inner);
        db.check_review(track_id, expected_review)?;
        candidate_artwork_source_in(&db, track_id, candidate_index)?
    };
    let Some((url, fetcher_provider)) = source else {
        return Ok(None);
    };
    let Some(artwork) = fetch_or_load_artwork(database, pipeline, &url, &fetcher_provider)? else {
        return Ok(None);
    };
    let mut db = database.lock().unwrap_or_else(PoisonError::into_inner);
    db.check_review(track_id, expected_review)?;
    if accept {
        db.set_user_artwork(track_id, &artwork)?;
        db.clear_pending_proposed_artwork(track_id)?;
    } else {
        let same_existing = db
            .track_artwork(track_id)?
            .is_some_and(|existing| existing.hash == artwork.hash);
        if same_existing {
            db.clear_pending_proposed_artwork(track_id)?;
        } else {
            db.store_proposed_artwork(track_id, &artwork, &fetcher_provider, &url)?;
        }
    }
    Ok(Some(artwork.hash))
}

/// Fetches and caches release artwork for a stored candidate without changing `selected_candidate`,
/// `proposed_artwork`, or `user_artwork`.
pub fn preview_candidate_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    candidate_index: u32,
) -> Result<Option<String>, IdentifyError> {
    let Some((url, fetcher_provider)) =
        candidate_artwork_source(database, track_id, candidate_index)?
    else {
        return Ok(None);
    };
    Ok(fetch_or_load_artwork(database, pipeline, &url, &fetcher_provider)?.map(|art| art.hash))
}

fn candidate_artwork_source(
    database: &Mutex<Database>,
    track_id: i64,
    candidate_index: u32,
) -> Result<Option<(String, String)>, IdentifyError> {
    let db = database.lock().unwrap_or_else(PoisonError::into_inner);
    candidate_artwork_source_in(&db, track_id, candidate_index)
}

fn candidate_artwork_source_in(
    database: &Database,
    track_id: i64,
    candidate_index: u32,
) -> Result<Option<(String, String)>, IdentifyError> {
    let Some(stored_row) = database.identification(track_id)? else {
        return Ok(None);
    };
    let candidates = parse_ranked_candidates(&stored_row.candidates_json);
    let Some(chosen) = candidates.get(candidate_index as usize) else {
        return Ok(None);
    };
    Ok(chosen
        .candidate
        .artwork_url
        .as_deref()
        .zip(chosen.candidate.artwork_fetcher_provider())
        .map(|(url, provider)| (url.to_owned(), provider.to_owned())))
}

pub use crate::store::{parse_match_evidence, parse_ranked_candidates, parse_step_reports};

fn record(
    database: &mut Database,
    input: &IdentificationInput,
    identification: &Identification,
) -> Result<(), IdentifyError> {
    database.record_identification(&stored(identification, identification.is_complete()))?;
    if let Some(best) = &identification.best {
        database.store_proposals(identification.track_id, &proposals_from(best))?;
    } else {
        let local = local_proposals(&input.tags, &input.path);
        database.store_proposals(identification.track_id, &local)?;
        database.clear_pending_proposed_artwork(identification.track_id)?;
    }
    Ok(())
}

fn propose_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    identification: &Identification,
) -> Result<(), IdentifyError> {
    let Some(best) = &identification.best else {
        return Ok(());
    };
    let _ = propose_candidate_artwork(database, pipeline, identification.track_id, best, false)?;
    Ok(())
}

fn propose_candidate_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    best: &RankedCandidate,
    allow_replace_file_artwork: bool,
) -> Result<Option<StoredArtwork>, IdentifyError> {
    propose_candidate_artwork_checked(
        database,
        pipeline,
        track_id,
        best,
        allow_replace_file_artwork,
        None,
    )
}

fn propose_candidate_artwork_checked(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    track_id: i64,
    best: &RankedCandidate,
    allow_replace_file_artwork: bool,
    expected_candidate_index: Option<u32>,
) -> Result<Option<StoredArtwork>, IdentifyError> {
    let Some((url, fetcher_provider)) = best
        .candidate
        .artwork_url
        .as_deref()
        .zip(best.candidate.artwork_fetcher_provider())
    else {
        return Ok(None);
    };
    {
        let library = database.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(_store) = library.artwork_store() else {
            return Ok(None);
        };
        if !allow_replace_file_artwork && library.track_artwork(track_id)?.is_some() {
            return Ok(None);
        }
    }
    let Some(artwork) = fetch_or_load_artwork(database, pipeline, url, fetcher_provider)? else {
        return Ok(None);
    };
    let mut library = database.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(expected_index) = expected_candidate_index {
        let Some(stored_row) = library.identification(track_id)? else {
            return Ok(None);
        };
        if stored_row.selected_candidate != expected_index {
            return Ok(None);
        }
        let candidates = parse_ranked_candidates(&stored_row.candidates_json);
        let still_matches = candidates.get(expected_index as usize).is_some_and(|cand| {
            cand.candidate.artwork_url.as_deref() == Some(url)
                && cand.candidate.artwork_fetcher_provider() == Some(fetcher_provider)
        });
        if !still_matches {
            return Ok(None);
        }
    }
    let current = library.track_artwork(track_id)?;
    if !allow_replace_file_artwork && current.is_some() {
        return Ok(None);
    }
    if current.as_ref().map(|art| art.hash.as_str()) == Some(artwork.hash.as_str()) {
        library.clear_pending_proposed_artwork(track_id)?;
    } else {
        library.store_proposed_artwork(track_id, &artwork, fetcher_provider, url)?;
    }
    Ok(Some(artwork))
}

fn fetch_or_load_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    url: &str,
    fetcher_provider: &str,
) -> Result<Option<StoredArtwork>, IdentifyError> {
    let (store, cached) = {
        let library = database.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(store) = library.artwork_store() else {
            return Ok(None);
        };
        (
            store,
            library.cached_artwork_source(url, crate::provider::CACHE_TTL)?,
        )
    };
    match cached {
        Some(CachedArtworkSource::Stored(artwork)) => return Ok(Some(artwork)),
        Some(CachedArtworkSource::Unavailable) => return Ok(None),
        None => {}
    }
    let bytes = match pipeline.fetch_artwork(fetcher_provider, url) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            let mut library = database.lock().unwrap_or_else(PoisonError::into_inner);
            library.record_fetched_artwork(url, None)?;
            return Ok(None);
        }
        Err(_) => return Ok(None),
    };
    match store.put(&bytes) {
        Ok(artwork) => {
            let mut library = database.lock().unwrap_or_else(PoisonError::into_inner);
            library.record_fetched_artwork(url, Some(&artwork))?;
            Ok(Some(artwork))
        }
        Err(ArtworkError::Refused(_)) => {
            let mut library = database.lock().unwrap_or_else(PoisonError::into_inner);
            library.record_fetched_artwork(url, None)?;
            Ok(None)
        }
        Err(ArtworkError::Io(_)) => Ok(None),
    }
}
