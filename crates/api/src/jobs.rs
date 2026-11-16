use std::path::Path;
use std::sync::{PoisonError, Weak};
use std::time::{Duration, Instant};

use resonance_core::jobs::{
    JOB_KIND_IDENTIFY_LIBRARY, JOB_KIND_SCAN_SOURCE, Job, JobStatus, JobTier, NewJob,
};
use resonance_jobs::{JobError, JobExecution, JobInterrupt, JobOutcome, JobRunner};
use resonance_library::identity::{
    AUDIO_DECODER_VERSION, apply_computed_identity_batch, compute_batch,
};
use resonance_library::scan::{ScanError, scan_chunked};
use resonance_library::watch::SourceWatcher;
use serde::{Deserialize, Serialize};

use crate::command::{
    ActivityJob, ActivityJobStatus, ActivityJobTier, ActivitySnapshot, ScanReport, UnreadableFile,
};
use crate::error::ApiError;
use crate::identity::IDENTIFY_BATCH_SIZE;
use crate::{Api, LibraryCore, REPORTED_UNREADABLE_FILES, reconcile_watchers_for};

const LIBRARY_PROGRESS_PUSH_INTERVAL: Duration = Duration::from_millis(100);
const MAX_STORED_SCAN_OUTCOMES: usize = 64;

#[derive(Debug, Serialize, Deserialize)]
struct ScanJobPayload {
    folder: String,
}

pub(crate) fn scan_source_job(folder: &Path) -> Result<NewJob, ApiError> {
    let payload_json = serde_json::to_string(&ScanJobPayload {
        folder: folder.to_string_lossy().into_owned(),
    })
    .map_err(ApiError::internal)?;
    Ok(NewJob {
        kind: JOB_KIND_SCAN_SOURCE.to_owned(),
        tier: JobTier::Scan,
        label: scan_job_label(folder),
        payload_json,
        total_items: None,
    })
}

pub(crate) fn identify_library_job(total_items: Option<u64>) -> NewJob {
    NewJob {
        kind: JOB_KIND_IDENTIFY_LIBRARY.to_owned(),
        tier: JobTier::BackgroundCpu,
        label: "Identify audio files".to_owned(),
        payload_json: "{}".to_owned(),
        total_items,
    }
}

fn scan_job_label(folder: &Path) -> String {
    let name = folder
        .file_name()
        .and_then(|leaf| leaf.to_str())
        .map(str::trim)
        .filter(|leaf| !leaf.is_empty());
    match name {
        Some(leaf) => format!("Scan {leaf}"),
        None => format!("Scan {}", folder.display()),
    }
}

pub(crate) fn enqueue_identify_if_needed(core: &LibraryCore, jobs: &JobRunner) {
    let pending = {
        let library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
        library
            .count_pending_identity_assets(AUDIO_DECODER_VERSION)
            .unwrap_or(0)
    };
    if pending > 0 {
        let _ = jobs.enqueue_coalesced(&identify_library_job(Some(pending)));
    }
}

pub(crate) fn execute_job(
    exec: &JobExecution<'_>,
    core: &Weak<LibraryCore>,
    watcher: &Weak<SourceWatcher>,
) -> JobOutcome {
    let Some(core) = core.upgrade() else {
        return JobOutcome::Interrupted;
    };
    match exec.job().kind.as_str() {
        JOB_KIND_SCAN_SOURCE => execute_scan_job(exec, &core, watcher),
        JOB_KIND_IDENTIFY_LIBRARY => execute_identify_job(exec, &core),
        other => JobOutcome::Failed(format!("Unknown job kind: {other}")),
    }
}

fn execute_scan_job(
    exec: &JobExecution<'_>,
    core: &LibraryCore,
    watcher: &Weak<SourceWatcher>,
) -> JobOutcome {
    let payload: ScanJobPayload = match serde_json::from_str(&exec.job().payload_json) {
        Ok(payload) => payload,
        Err(error) => {
            let api_error = ApiError::bad_request(error);
            store_scan_outcome(core, exec.job().id, Err(api_error.clone()));
            return JobOutcome::Failed(api_error.message);
        }
    };

    let folder = Path::new(&payload.folder);
    let own_files = core
        .own_files
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let mut last_library_push: Option<Instant> = None;

    let result = scan_chunked(
        &core.library,
        folder,
        &own_files,
        exec.abort_flag(),
        |progress| {
            let _ = exec.report_progress(progress.done_files, Some(progress.total_files));
            if progress.wrote_changes && progress.done_files < progress.total_files {
                let now = Instant::now();
                let should_push = match last_library_push {
                    Some(previous)
                        if now.duration_since(previous) < LIBRARY_PROGRESS_PUSH_INTERVAL =>
                    {
                        false
                    }
                    _ => {
                        last_library_push = Some(now);
                        true
                    }
                };
                if should_push {
                    core.watch_revisions.send_modify(|revision| *revision += 1);
                }
            }
        },
    );

    match result {
        Ok(report) => {
            if let Some(watcher) = watcher.upgrade() {
                let library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
                reconcile_watchers_for(core, &watcher, &library);
            }
            let found = report.found as u64;
            let library_changed =
                report.added > 0 || report.changed > 0 || report.missing_changed > 0;
            let command_report = to_command_scan_report(report);
            store_scan_outcome(core, exec.job().id, Ok(command_report));

            if library_changed && !exec.has_waiters() {
                core.watch_revisions.send_modify(|revision| *revision += 1);
            }
            if let Some(jobs) = core.jobs.get().and_then(Weak::upgrade) {
                enqueue_identify_if_needed(core, &jobs);
            }
            JobOutcome::Completed {
                done_items: found,
                total_items: Some(found),
            }
        }
        Err(ScanError::Cancelled) => {
            if exec.is_cancelled() {
                store_scan_outcome(
                    core,
                    exec.job().id,
                    Err(ApiError::scan_failed("The scan was cancelled")),
                );
                JobOutcome::Cancelled
            } else {
                JobOutcome::Interrupted
            }
        }
        Err(error) => {
            let api_error = ApiError::scan_failed(error);
            store_scan_outcome(core, exec.job().id, Err(api_error.clone()));
            JobOutcome::Failed(api_error.message)
        }
    }
}

fn execute_identify_job(exec: &JobExecution<'_>, core: &LibraryCore) -> JobOutcome {
    let mut total = {
        let library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
        match library.count_pending_identity_assets(AUDIO_DECODER_VERSION) {
            Ok(count) => count,
            Err(error) => return JobOutcome::Failed(ApiError::library_unreadable(error).message),
        }
    };
    let mut done = 0u64;
    let _ = exec.report_progress(0, Some(total));
    if total == 0 {
        return JobOutcome::Completed {
            done_items: 0,
            total_items: Some(0),
        };
    }

    let mut after_asset_id = 0i64;
    let mut wrapped_once = false;

    loop {
        match exec.wait_if_paused() {
            Ok(()) => {}
            Err(JobInterrupt::Cancelled) => return JobOutcome::Cancelled,
            Err(JobInterrupt::Shutdown) => return JobOutcome::Interrupted,
        }

        let _pass = core
            .identify_pass
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match exec.check_interrupted() {
            Ok(()) => {}
            Err(JobInterrupt::Cancelled) => return JobOutcome::Cancelled,
            Err(JobInterrupt::Shutdown) => return JobOutcome::Interrupted,
        }

        let (batch, remaining) = {
            let library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
            let batch = match library.pending_identity_assets(
                after_asset_id,
                AUDIO_DECODER_VERSION,
                IDENTIFY_BATCH_SIZE,
            ) {
                Ok(batch) => batch,
                Err(error) => {
                    return JobOutcome::Failed(ApiError::library_unreadable(error).message);
                }
            };
            let remaining = library
                .count_pending_identity_assets(AUDIO_DECODER_VERSION)
                .unwrap_or(0);
            (batch, remaining)
        };

        let Some(last) = batch.last() else {
            if !wrapped_once && remaining > 0 {
                after_asset_id = 0;
                wrapped_once = true;
                continue;
            }
            return JobOutcome::Completed {
                done_items: done,
                total_items: Some(total.max(done)),
            };
        };
        wrapped_once = false;
        after_asset_id = last.asset_id;
        total = total.max(done.saturating_add(remaining));

        let computed = compute_batch(&batch, Some(exec.abort_flag()));
        match exec.check_interrupted() {
            Ok(()) => {}
            Err(JobInterrupt::Cancelled) => return JobOutcome::Cancelled,
            Err(JobInterrupt::Shutdown) => return JobOutcome::Interrupted,
        }

        let batch_report = match apply_computed_identity_batch(&core.library, &batch, computed) {
            Ok(report) => report,
            Err(error) => return JobOutcome::Failed(ApiError::library_unreadable(error).message),
        };

        done = done.saturating_add(batch.len() as u64);
        total = total.max(done);
        let _ = exec.report_progress(done, Some(total));
        if batch_report.reconciled_moves > 0 {
            core.watch_revisions.send_modify(|revision| *revision += 1);
        }
    }
}

fn store_scan_outcome(core: &LibraryCore, job_id: i64, outcome: Result<ScanReport, ApiError>) {
    let mut outcomes = core
        .scan_outcomes
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    outcomes.insert(job_id, outcome);
    while outcomes.len() > MAX_STORED_SCAN_OUTCOMES {
        let Some(&oldest_id) = outcomes.keys().min() else {
            break;
        };
        outcomes.remove(&oldest_id);
    }
}

fn to_command_scan_report(report: resonance_library::scan::ScanReport) -> ScanReport {
    ScanReport {
        source_folder: report.source_folder,
        found: report.found,
        added: report.added,
        changed: report.changed,
        unchanged: report.unchanged,
        unreadable_count: report.unreadable.len(),
        unreadable: report
            .unreadable
            .into_iter()
            .take(REPORTED_UNREADABLE_FILES)
            .map(|file| UnreadableFile {
                path: file.path,
                reason: file.reason,
            })
            .collect(),
    }
}

pub(crate) fn job_api_error(error: JobError) -> ApiError {
    match error {
        JobError::Database(source) => ApiError::library_unreadable(source),
        JobError::NotFound { .. }
        | JobError::NotCancellable { .. }
        | JobError::NotRetryable { .. } => ApiError::invalid_input(error),
        JobError::Cancelled | JobError::Shutdown | JobError::Failed(_) => ApiError::internal(error),
    }
}

fn ns_to_ms(nanos: i64) -> i64 {
    nanos / 1_000_000
}

fn to_activity_job(job: Job, mix_live: bool) -> ActivityJob {
    let paused_for_mix = mix_live
        && job.tier.backs_off_for_mix()
        && matches!(job.status, JobStatus::Queued | JobStatus::Running);
    ActivityJob {
        id: job.id,
        kind: job.kind,
        tier: match job.tier {
            JobTier::Scan => ActivityJobTier::Interactive,
            JobTier::BackgroundCpu => ActivityJobTier::Metadata,
            JobTier::Heavy => ActivityJobTier::Analysis,
            JobTier::OnDemand => ActivityJobTier::OnDemand,
        },
        status: match job.status {
            JobStatus::Queued => ActivityJobStatus::Queued,
            JobStatus::Running => ActivityJobStatus::Running,
            JobStatus::Completed => ActivityJobStatus::Completed,
            JobStatus::Failed => ActivityJobStatus::Failed,
            JobStatus::Cancelled => ActivityJobStatus::Cancelled,
        },
        label: job.label,
        done_items: job.done_items,
        total_items: job.total_items,
        error_message: job.error_message,
        paused_for_mix,
        created_ms: ns_to_ms(job.created_ns),
        started_ms: job.started_ns.map(ns_to_ms),
        finished_ms: job.finished_ns.map(ns_to_ms),
    }
}

impl Api {
    pub(crate) fn run_scan_job(&self, folder: &Path) -> Result<ScanReport, ApiError> {
        let new_job = scan_source_job(folder)?;
        let (_queued, rx) = self
            .jobs
            .enqueue_coalesced_with_waiter(&new_job)
            .map_err(job_api_error)?;
        let finished = rx
            .recv()
            .map_err(|_| ApiError::internal("The scan job stopped before reporting a result."))?;

        let stored = self
            .core
            .scan_outcomes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&finished.id)
            .cloned();
        if let Some(result) = stored {
            return result;
        }
        match finished.status {
            JobStatus::Cancelled => Err(ApiError::scan_failed("The scan was cancelled")),
            JobStatus::Failed => Err(ApiError::scan_failed(
                finished
                    .error_message
                    .unwrap_or_else(|| "The scan failed".to_owned()),
            )),
            _ => Err(ApiError::internal(
                "The scan job completed without storing its report.",
            )),
        }
    }

    pub(crate) fn jobs_snapshot(&self) -> Result<ActivitySnapshot, ApiError> {
        let mix_live = self.jobs.is_mix_live();
        let snapshot = self.jobs.snapshot().map_err(job_api_error)?;
        Ok(ActivitySnapshot {
            active: snapshot
                .active
                .into_iter()
                .map(|job| to_activity_job(job, mix_live))
                .collect(),
            recent: snapshot
                .recent
                .into_iter()
                .map(|job| to_activity_job(job, mix_live))
                .collect(),
            mix_backoff_active: mix_live,
        })
    }

    pub(crate) fn cancel_job(&self, job_id: i64) -> Result<ActivitySnapshot, ApiError> {
        self.jobs.cancel(job_id).map_err(job_api_error)?;
        self.jobs_snapshot()
    }

    pub(crate) fn retry_job(&self, job_id: i64) -> Result<ActivitySnapshot, ApiError> {
        self.jobs.retry(job_id).map_err(job_api_error)?;
        self.jobs_snapshot()
    }

    pub(crate) fn clear_finished_jobs(&self) -> Result<ActivitySnapshot, ApiError> {
        self.jobs.clear_finished().map_err(job_api_error)?;
        self.jobs_snapshot()
    }
}
