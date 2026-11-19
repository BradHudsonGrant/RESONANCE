//! Identification over the library database: the local pass on every track that has no result,
//! then the online pass, tracks the user is looking at first. Each batch is stored as soon as it
//! is done, so a run stopped by a restart resumes where it was.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use resonance_core::identify::{Candidate, KnownTags, ReleaseUrl, StepOutcome};
use resonance_core::staging::proposals_from;
use resonance_db::{Database, IdentificationPass, StoredIdentification};
use serde_json::{Value, json};

use crate::pipeline::{Identification, Pipeline, identify_locally};
use crate::provider::IdentifyError;
use crate::providers::acoustid;

const LOCAL_BATCH: u32 = 500;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OnlineReport {
    pub looked_up: u64,
    pub confident: u64,
    pub needs_url: u64,
    /// Tracks with a step that failed or lacked its key, which the next run tries again.
    pub incomplete: u64,
    pub network_requests: u32,
    pub cached_answers: u32,
}

/// Steps 1 and 2 for every track with no result yet. Returns how many were read.
pub fn run_local_pass(
    database: &Mutex<Database>,
    abort: &AtomicBool,
) -> Result<u64, IdentifyError> {
    let mut after = 0;
    let mut done = 0;
    loop {
        if abort.load(Ordering::Acquire) {
            return Ok(done);
        }
        let mut database = database.lock().unwrap_or_else(PoisonError::into_inner);
        let inputs =
            database.identification_inputs(IdentificationPass::Local, after, LOCAL_BATCH)?;
        let Some(last) = inputs.last() else {
            return Ok(done);
        };
        after = last.track_id;
        for input in &inputs {
            database.record_identification(&stored(&identify_locally(input), false))?;
        }
        done += inputs.len() as u64;
    }
}

pub fn count_online_pending(database: &Mutex<Database>) -> Result<u64, IdentifyError> {
    Ok(database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .count_tracks_needing_identification(IdentificationPass::Online)?)
}

/// Steps 1 to 6 for every track without an online result, `priority` first. `progress` hears
/// how many are done and how many there are in all.
pub fn run_online_pass(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    priority: &[i64],
    abort: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<OnlineReport, IdentifyError> {
    run_local_pass(database, abort)?;
    let mut report = OnlineReport::default();
    let mut total = count_online_pending(database)?;
    let mut seen = HashSet::new();
    let mut queue = priority.to_vec();
    queue.reverse();
    let mut after = 0;
    let mut exhausted = false;
    let batch_size = acoustid::BATCH_SIZE as u32;
    let start = pipeline.fetch_counts();
    progress(0, total);

    loop {
        if abort.load(Ordering::Acquire) {
            break;
        }
        let batch = {
            let database = database.lock().unwrap_or_else(PoisonError::into_inner);
            let mut batch = Vec::new();
            while batch.len() < batch_size as usize
                && let Some(track_id) = queue.pop()
            {
                if seen.insert(track_id)
                    && let Some(input) =
                        database.identification_input(IdentificationPass::Online, track_id)?
                {
                    batch.push(input);
                }
            }
            if batch.len() < batch_size as usize {
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
        let mut library = database.lock().unwrap_or_else(PoisonError::into_inner);
        for identification in &identified {
            record(&mut library, identification)?;
            report.looked_up += 1;
            report.confident += u64::from(identification.is_confident());
            report.needs_url += u64::from(identification.needs_url());
            report.incomplete += u64::from(!identification.is_complete());
        }
        drop(library);
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
        &identification,
    )?;
    propose_artwork(database, pipeline, &identification)?;
    Ok(Some(identification))
}

/// Stores an online result and stages the best candidate's fields. A run without a best
/// candidate leaves earlier proposals as they are.
fn record(database: &mut Database, identification: &Identification) -> Result<(), IdentifyError> {
    database.record_identification(&stored(identification, identification.is_complete()))?;
    if let Some(best) = &identification.best {
        database.store_proposals(identification.track_id, &proposals_from(best))?;
    }
    Ok(())
}

/// Stages the best release's front cover for a track that shows no picture yet. A picture
/// already fetched from the same address is reused, so the tracks of a release fetch it once.
/// A cover that cannot be fetched or is not a JPEG or PNG is left out; it never fails the pass.
fn propose_artwork(
    database: &Mutex<Database>,
    pipeline: &Pipeline,
    identification: &Identification,
) -> Result<(), IdentifyError> {
    let Some(best) = &identification.best else {
        return Ok(());
    };
    let Some(url) = &best.candidate.artwork_url else {
        return Ok(());
    };
    let track_id = identification.track_id;
    let (store, fetched) = {
        let library = database.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(store) = library.artwork_store() else {
            return Ok(());
        };
        if library.track_artwork(track_id)?.is_some() {
            return Ok(());
        }
        (store, library.artwork_fetched_from(url)?)
    };
    let artwork = match fetched {
        Some(artwork) => artwork,
        None => {
            let Ok(Some(bytes)) = pipeline.fetch_artwork(&best.candidate.provider, url) else {
                return Ok(());
            };
            match store.put(&bytes) {
                Ok(artwork) => artwork,
                Err(_) => return Ok(()),
            }
        }
    };
    database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .store_proposed_artwork(track_id, &artwork, &best.candidate.provider, url)?;
    Ok(())
}

fn stored(identification: &Identification, online: bool) -> StoredIdentification {
    StoredIdentification {
        track_id: identification.track_id,
        online,
        confidence: identification.best.as_ref().map(|best| best.confidence),
        audio_matched: identification
            .best
            .as_ref()
            .is_some_and(|best| best.audio_matched),
        known_json: known_json(&identification.known).to_string(),
        best_json: identification.best.as_ref().map(|best| {
            let mut value = candidate_json(&best.candidate);
            value["duration_delta_ms"] = json!(best.duration_delta_ms);
            value.to_string()
        }),
        steps_json: Value::Array(
            identification
                .steps
                .iter()
                .map(|report| {
                    let mut value = outcome_json(&report.outcome);
                    value["step"] = json!(report.step.name());
                    value["provider"] = json!(report.provider);
                    value
                })
                .collect(),
        )
        .to_string(),
        updated_ns: 0,
    }
}

fn known_json(known: &KnownTags) -> Value {
    json!({
        "artist": known.artist,
        "title": known.title,
        "mix": known.mix,
        "album": known.album,
        "label": known.label,
        "catno": known.catno,
        "year": known.year,
        "track_number": known.track_number,
    })
}

fn candidate_json(candidate: &Candidate) -> Value {
    json!({
        "provider": candidate.provider,
        "artist": candidate.artist,
        "title": candidate.title,
        "mix": candidate.mix,
        "album": candidate.album,
        "label": candidate.label,
        "catno": candidate.catno,
        "year": candidate.year,
        "duration_ms": candidate.duration_ms,
        "recording_id": candidate.recording_id,
        "release_id": candidate.release_id,
        "url": candidate.url,
        "artwork_url": candidate.artwork_url,
        "fingerprint_score": candidate.fingerprint_score,
        "user_supplied": candidate.user_supplied,
    })
}

fn outcome_json(outcome: &StepOutcome) -> Value {
    match outcome {
        StepOutcome::Read { fields } => json!({ "outcome": "read", "fields": fields }),
        StepOutcome::Ran { candidates, best } => {
            json!({ "outcome": "ran", "candidates": candidates, "best": best })
        }
        StepOutcome::SkippedMissingKey(key) => {
            json!({ "outcome": "skipped_missing_key", "key": key.account() })
        }
        StepOutcome::SkippedNoInput => json!({ "outcome": "skipped_no_input" }),
        StepOutcome::SkippedConfident => json!({ "outcome": "skipped_confident" }),
        StepOutcome::Failed(reason) => json!({ "outcome": "failed", "reason": reason }),
        StepOutcome::NeedsUrl => json!({ "outcome": "needs_url" }),
    }
}
