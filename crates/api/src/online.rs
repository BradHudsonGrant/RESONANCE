use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use resonance_core::identify::{
    MatchBasis as CoreMatchBasis, MatchEvidence as CoreMatchEvidence, StepOutcome,
    parse_release_url,
};
use resonance_core::jobs::{JOB_KIND_IDENTIFY_ONLINE, JobTier, NewJob};
use resonance_core::providers::IdentificationSettings as CoreIdentificationSettings;
use resonance_db::Database;
use resonance_identify::audio::{AudioDecodeError, AudioFingerprinter};
use resonance_identify::credentials::CredentialStore;
use resonance_identify::http::HttpClient;
use resonance_identify::library::{
    count_online_pending, identify_track_with_url, reidentify_tracks, run_online_pass,
};
use resonance_identify::pipeline::{Identification, Pipeline, StepReport};
use resonance_identify::provider::IdentifyError;
use resonance_jobs::{JobExecution, JobInterrupt, JobOutcome};
use resonance_library::identity::{
    AUDIO_DECODER_VERSION, apply_computed_identity_batch, compute_batch, extract_pcm_wav_excerpt,
    fingerprint_audio_bytes, fingerprint_file_full,
};
use serde::{Deserialize, Serialize};

use crate::command::{
    ActivitySnapshot, IdentificationSettings, IdentifiedRelease, IdentifyStepOutcome,
    IdentifyStepReport, MatchBasis, MatchEvidence, TrackIdentification,
};
use crate::error::ApiError;
use crate::jobs::{enqueue_identify_if_needed, job_api_error};
use crate::providers::to_command;
use crate::{Api, LibraryCore};

pub struct LibraryAudioFingerprinter;

impl AudioFingerprinter for LibraryAudioFingerprinter {
    fn fingerprint_bytes(
        &self,
        bytes: &[u8],
        ext_hint: Option<&str>,
    ) -> Result<Vec<u32>, AudioDecodeError> {
        fingerprint_audio_bytes(bytes, ext_hint).map_err(|e| AudioDecodeError(e.to_string()))
    }

    fn fingerprint_file_full(&self, path: &Path) -> Result<Vec<u32>, AudioDecodeError> {
        fingerprint_file_full(path).map_err(|e| AudioDecodeError(e.to_string()))
    }

    fn extract_wav_excerpt(
        &self,
        path: &Path,
        start_ratio: f32,
        duration_s: u32,
    ) -> Result<Vec<u8>, AudioDecodeError> {
        extract_pcm_wav_excerpt(path, start_ratio, duration_s)
            .map_err(|e| AudioDecodeError(e.to_string()))
    }

    fn resolve_asset_path(&self, source_folder: &str, relative_path: &str) -> std::path::PathBuf {
        resonance_library::identity::full_asset_path(source_folder, relative_path)
    }
}

const ONLINE_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const ON_DEMAND_MAX_TRACKS: usize = 25;
const FINGERPRINT_BATCH: usize = 16;

#[derive(Debug, Serialize, Deserialize)]
struct OnlineJobPayload {
    priority_track_ids: Vec<i64>,
    #[serde(default)]
    only_selected: bool,
}

pub(crate) fn pipeline(
    http: &Arc<dyn HttpClient>,
    database: &Arc<Mutex<Database>>,
    credentials: &Arc<dyn CredentialStore>,
) -> Arc<Pipeline> {
    let verify = database
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .identification_settings()
        .map(|s| s.verify_audio_previews)
        .unwrap_or(true);
    Arc::new(
        Pipeline::with_free_providers(
            Arc::clone(http),
            Arc::clone(database),
            Arc::clone(credentials),
        )
        .with_fingerprinter(Arc::new(LibraryAudioFingerprinter))
        .with_verify_previews(verify),
    )
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
    let mut last_push: Option<Instant> = None;
    let mut last_pushed_done = 0u64;
    let mut on_progress = |done: u64, total: u64| {
        let _ = exec.report_progress(done, Some(total));
        if done > last_pushed_done {
            let now = Instant::now();
            if last_push.is_none_or(|prev| now.duration_since(prev) >= ONLINE_REFRESH_INTERVAL) {
                last_push = Some(now);
                last_pushed_done = done;
                core.watch_revisions.send_modify(|rev| *rev += 1);
            }
        }
        // Waits here while Mix plays; a cancel or shutdown sets the abort flag the pass
        // checks before its next batch.
        let _ = exec.wait_if_paused();
    };
    let result = if payload.only_selected {
        search_selected(
            core,
            exec,
            &pipeline,
            &payload.priority_track_ids,
            &mut on_progress,
        )
    } else {
        run_online_pass(
            &core.library,
            &pipeline,
            &payload.priority_track_ids,
            exec.abort_flag(),
            &mut on_progress,
        )
        .map(|report| report.looked_up)
    };
    let pushed_final = matches!(&result, Ok(done) if *done > last_pushed_done);
    match (result, exec.check_interrupted()) {
        (_, Err(JobInterrupt::Cancelled)) => {
            if pushed_final {
                core.watch_revisions.send_modify(|rev| *rev += 1);
            }
            JobOutcome::Cancelled
        }
        (_, Err(JobInterrupt::Shutdown)) => JobOutcome::Interrupted,
        (Ok(done), Ok(())) => {
            if !payload.only_selected {
                core.local_pass_dirty.store(false, Ordering::Release);
            }
            if pushed_final {
                core.watch_revisions.send_modify(|rev| *rev += 1);
            }
            JobOutcome::Completed {
                done_items: done,
                total_items: Some(done),
            }
        }
        (Err(error), Ok(())) => JobOutcome::Failed(identify_error(error).message),
    }
}

/// Deliver the first suggestions while the rest of a large selection is still waiting to decode.
fn search_selected(
    core: &LibraryCore,
    exec: &JobExecution<'_>,
    pipeline: &Pipeline,
    track_ids: &[i64],
    mut progress: impl FnMut(u64, u64),
) -> Result<u64, IdentifyError> {
    let mut done = 0;
    let total = track_ids.len() as u64;
    for ids in track_ids.chunks(FINGERPRINT_BATCH) {
        if exec.wait_if_paused().is_err() {
            break;
        }
        let pending = core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pending_identity_assets_for_tracks(ids, AUDIO_DECODER_VERSION)?;
        if !pending.is_empty() {
            let computed = compute_batch(&pending, Some(exec.abort_flag()));
            if exec.check_interrupted().is_err() {
                break;
            }
            apply_computed_identity_batch(&core.library, &pending, computed)?;
        }
        let report = reidentify_tracks(
            &core.library,
            pipeline,
            ids,
            exec.abort_flag(),
            |completed, _| progress(done + completed, total),
        )?;
        done += report.looked_up;
    }
    Ok(done)
}

pub(crate) fn to_command_evidence(evidence: &CoreMatchEvidence) -> MatchEvidence {
    MatchEvidence {
        basis: match evidence.basis {
            CoreMatchBasis::AudioFingerprint => MatchBasis::AudioFingerprint,
            CoreMatchBasis::AudioPreview => MatchBasis::AudioPreview,
            CoreMatchBasis::AudioRecogniser => MatchBasis::AudioRecogniser,
            CoreMatchBasis::UserUrl => MatchBasis::UserUrl,
            CoreMatchBasis::EmbeddedUrl => MatchBasis::EmbeddedUrl,
            CoreMatchBasis::CatalogAndDuration => MatchBasis::CatalogAndDuration,
            CoreMatchBasis::TagsAndDuration => MatchBasis::TagsAndDuration,
            CoreMatchBasis::TagsOnly => MatchBasis::TagsOnly,
            CoreMatchBasis::LocalTags => MatchBasis::LocalTags,
        },
        providers: evidence.providers.clone(),
        primary_provider: evidence.primary_provider.clone(),
        duration_delta_ms: evidence.duration_delta_ms,
        preview_similarity: evidence.preview_similarity,
        catno_matched: evidence.catno_matched,
        isrc_matched: evidence.isrc_matched,
        url: evidence.url.clone(),
    }
}

pub(crate) fn to_command_step_reports(steps: &[StepReport]) -> Vec<IdentifyStepReport> {
    steps
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
        .collect()
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
        steps: to_command_step_reports(&identification.steps),
    }
}

impl Api {
    pub(crate) fn identification_settings(&self) -> Result<IdentificationSettings, ApiError> {
        let settings = self
            .locked_library()
            .identification_settings()
            .map_err(ApiError::library_unreadable)?;
        Ok(IdentificationSettings {
            verify_audio_previews: settings.verify_audio_previews,
        })
    }

    pub(crate) fn set_identification_settings(
        &self,
        settings: IdentificationSettings,
    ) -> Result<IdentificationSettings, ApiError> {
        self.locked_library()
            .save_identification_settings(&CoreIdentificationSettings {
                verify_audio_previews: settings.verify_audio_previews,
            })
            .map_err(ApiError::library_unreadable)?;
        self.rebuild_pipeline();
        self.identification_settings()
    }
    pub(crate) fn identify_online(
        &self,
        track_ids: &[i64],
        only_selected: bool,
    ) -> Result<ActivitySnapshot, ApiError> {
        let mut seen = HashSet::new();
        let unique_ids: Vec<i64> = track_ids
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .collect();
        if only_selected && unique_ids.is_empty() {
            return Err(ApiError::invalid_input(
                "Select at least one track to look up online.",
            ));
        }

        let (tier, label, total_items) = if only_selected {
            let n = unique_ids.len();
            let tier = if n <= ON_DEMAND_MAX_TRACKS {
                JobTier::OnDemand
            } else {
                JobTier::BackgroundCpu
            };
            let label = if n == 1 {
                "Look up 1 track online".to_owned()
            } else {
                format!("Look up {n} tracks online")
            };
            (tier, label, Some(n as u64))
        } else {
            // Fingerprints still pending are computed first, on the same lane, so step 3 has them.
            enqueue_identify_if_needed(&self.core, &self.jobs);
            let pending = count_online_pending(&self.core.library).map_err(identify_error)?;
            (
                JobTier::BackgroundCpu,
                "Look up tracks online".to_owned(),
                Some(pending),
            )
        };

        let payload_json = serde_json::to_string(&OnlineJobPayload {
            priority_track_ids: unique_ids,
            only_selected,
        })
        .map_err(ApiError::internal)?;
        self.jobs
            .enqueue_coalesced(&NewJob {
                kind: JOB_KIND_IDENTIFY_ONLINE.to_owned(),
                tier,
                label,
                payload_json,
                total_items,
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
        self.library_edited();
        Ok(to_command_identification(&identification))
    }
}
