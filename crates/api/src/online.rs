use std::sync::{Arc, Mutex, PoisonError};

use resonance_core::identify::{StepOutcome, parse_release_url};
use resonance_core::jobs::{JOB_KIND_IDENTIFY_ONLINE, JobTier, NewJob};
use resonance_db::Database;
use resonance_identify::credentials::CredentialStore;
use resonance_identify::http::HttpClient;
use resonance_identify::library::{count_online_pending, identify_track_with_url, run_online_pass};
use resonance_identify::pipeline::{Identification, Pipeline};
use resonance_identify::provider::IdentifyError;
use resonance_jobs::{JobExecution, JobInterrupt, JobOutcome};
use serde::{Deserialize, Serialize};

use crate::command::{
    ActivitySnapshot, IdentifiedRelease, IdentifyStepOutcome, IdentifyStepReport,
    TrackIdentification,
};
use crate::error::ApiError;
use crate::jobs::{enqueue_identify_if_needed, job_api_error};
use crate::providers::to_command;
use crate::{Api, LibraryCore};

#[derive(Debug, Serialize, Deserialize)]
struct OnlineJobPayload {
    priority_track_ids: Vec<i64>,
}

pub(crate) fn pipeline(
    http: &Arc<dyn HttpClient>,
    database: &Arc<Mutex<Database>>,
    credentials: &Arc<dyn CredentialStore>,
) -> Arc<Pipeline> {
    Arc::new(Pipeline::with_free_providers(
        Arc::clone(http),
        Arc::clone(database),
        Arc::clone(credentials),
    ))
}

fn identify_error(error: IdentifyError) -> ApiError {
    match error {
        IdentifyError::Database(source) => ApiError::library_unreadable(source),
        IdentifyError::Credentials(source) => ApiError::credentials_unavailable(source),
        other => ApiError::internal(other),
    }
}

pub(crate) fn execute_identify_online_job(
    exec: &JobExecution<'_>,
    core: &LibraryCore,
) -> JobOutcome {
    let payload: OnlineJobPayload = match serde_json::from_str(&exec.job().payload_json) {
        Ok(payload) => payload,
        Err(error) => return JobOutcome::Failed(ApiError::bad_request(error).message),
    };
    match exec.wait_if_paused() {
        Ok(()) => {}
        Err(JobInterrupt::Cancelled) => return JobOutcome::Cancelled,
        Err(JobInterrupt::Shutdown) => return JobOutcome::Interrupted,
    }
    let pipeline = Arc::clone(&core.pipeline.lock().unwrap_or_else(PoisonError::into_inner));
    let result = run_online_pass(
        &core.library,
        &pipeline,
        &payload.priority_track_ids,
        exec.abort_flag(),
        |done, total| {
            let _ = exec.report_progress(done, Some(total));
            // Waits here while Mix plays; a cancel or shutdown sets the abort flag the pass
            // checks before its next batch.
            let _ = exec.wait_if_paused();
        },
    );
    match (result, exec.check_interrupted()) {
        (_, Err(JobInterrupt::Cancelled)) => JobOutcome::Cancelled,
        (_, Err(JobInterrupt::Shutdown)) => JobOutcome::Interrupted,
        (Ok(report), Ok(())) => JobOutcome::Completed {
            done_items: report.looked_up,
            total_items: Some(report.looked_up),
        },
        (Err(error), Ok(())) => JobOutcome::Failed(identify_error(error).message),
    }
}

fn to_command_identification(identification: &Identification) -> TrackIdentification {
    TrackIdentification {
        track_id: identification.track_id,
        best: identification.best.as_ref().map(|best| {
            let candidate = &best.candidate;
            IdentifiedRelease {
                provider: candidate.provider.clone(),
                confidence: best.confidence,
                audio_matched: best.audio_matched,
                artist: candidate.artist.clone(),
                title: candidate.title.clone(),
                mix: candidate.mix.clone(),
                album: candidate.album.clone(),
                label: candidate.label.clone(),
                catno: candidate.catno.clone(),
                year: candidate.year,
                url: candidate.url.clone(),
            }
        }),
        needs_url: identification.needs_url(),
        steps: identification
            .steps
            .iter()
            .map(|report| IdentifyStepReport {
                step: report.step.number(),
                provider: report.provider.map(str::to_owned),
                outcome: match &report.outcome {
                    StepOutcome::Read { fields } => IdentifyStepOutcome::Read {
                        fields: *fields as u32,
                    },
                    StepOutcome::Ran { candidates, best } => IdentifyStepOutcome::Ran {
                        candidates: *candidates as u32,
                        best: *best,
                    },
                    StepOutcome::SkippedMissingKey(key) => IdentifyStepOutcome::SkippedMissingKey {
                        key: to_command(*key),
                    },
                    StepOutcome::SkippedNoInput => IdentifyStepOutcome::SkippedNoInput,
                    StepOutcome::SkippedConfident => IdentifyStepOutcome::SkippedConfident,
                    StepOutcome::Failed(reason) => IdentifyStepOutcome::Failed {
                        reason: reason.clone(),
                    },
                    StepOutcome::NeedsUrl => IdentifyStepOutcome::NeedsUrl,
                },
            })
            .collect(),
    }
}

impl Api {
    pub(crate) fn identify_online(&self, track_ids: &[i64]) -> Result<ActivitySnapshot, ApiError> {
        // Fingerprints still pending are computed first, on the same lane, so step 3 has them.
        enqueue_identify_if_needed(&self.core, &self.jobs);
        let pending = count_online_pending(&self.core.library).map_err(identify_error)?;
        let payload_json = serde_json::to_string(&OnlineJobPayload {
            priority_track_ids: track_ids.to_vec(),
        })
        .map_err(ApiError::internal)?;
        self.jobs
            .enqueue_coalesced(&NewJob {
                kind: JOB_KIND_IDENTIFY_ONLINE.to_owned(),
                tier: JobTier::BackgroundCpu,
                label: "Look up tracks online".to_owned(),
                payload_json,
                total_items: Some(pending),
            })
            .map_err(job_api_error)?;
        self.jobs_snapshot()
    }

    pub(crate) fn identify_with_url(
        &self,
        track_id: i64,
        url: &str,
    ) -> Result<TrackIdentification, ApiError> {
        let url = parse_release_url(url).map_err(ApiError::invalid_input)?;
        let pipeline = Arc::clone(
            &self
                .core
                .pipeline
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        let identification = identify_track_with_url(&self.core.library, &pipeline, track_id, &url)
            .map_err(identify_error)?
            .ok_or_else(|| ApiError::invalid_input(format!("Track {track_id} does not exist.")))?;
        Ok(to_command_identification(&identification))
    }
}
