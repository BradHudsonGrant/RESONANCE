use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use resonance_core::jobs::{Job, JobStatus, JobsSnapshot, NewJob};
use resonance_db::{Database, DbError};
use tokio::sync::watch;

pub const RECENT_JOBS_LIMIT: u32 = 50;
const PROGRESS_PUSH_INTERVAL: Duration = Duration::from_millis(100);
const MIX_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub enum JobError {
    Database(DbError),
    NotFound { job_id: i64 },
    NotCancellable { job_id: i64, status: JobStatus },
    NotRetryable { job_id: i64, status: JobStatus },
    NotPausable { job_id: i64, status: JobStatus },
    Cancelled,
    Shutdown,
    Failed(String),
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(source) => source.fmt(f),
            Self::NotFound { job_id } => write!(f, "Job {job_id} does not exist."),
            Self::NotCancellable { job_id, status } => write!(
                f,
                "Job {job_id} is already {} and cannot be cancelled.",
                status.name()
            ),
            Self::NotRetryable { job_id, status } => write!(
                f,
                "Job {job_id} is {} and cannot be retried.",
                status.name()
            ),
            Self::NotPausable { job_id, status } => write!(
                f,
                "Job {job_id} is {} and cannot be paused or resumed.",
                status.name()
            ),
            Self::Cancelled => write!(f, "The job was cancelled."),
            Self::Shutdown => write!(f, "The job queue has stopped."),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for JobError {}

impl From<DbError> for JobError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobInterrupt {
    Cancelled,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    Completed {
        done_items: u64,
        total_items: Option<u64>,
    },
    Cancelled,
    Interrupted,
    Failed(String),
}

#[derive(Clone)]
struct ActiveHandle {
    cancelled: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
}

struct CoordinatorState {
    active: HashMap<i64, ActiveHandle>,
    waiters: HashMap<i64, Vec<Sender<Job>>>,
    last_progress_push: Option<Instant>,
    shutdown: bool,
}

struct Shared {
    database: Arc<Mutex<Database>>,
    state: Mutex<CoordinatorState>,
    wake: Condvar,
    mix_live: AtomicBool,
    mix_probe: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    revisions: watch::Sender<u64>,
}

impl Shared {
    fn bump_revision(&self) {
        self.revisions.send_modify(|revision| *revision += 1);
    }

    fn bump_progress_throttled(&self) {
        let now = Instant::now();
        let should_push = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            match state.last_progress_push {
                Some(previous) if now.duration_since(previous) < PROGRESS_PUSH_INTERVAL => false,
                _ => {
                    state.last_progress_push = Some(now);
                    true
                }
            }
        };
        if should_push {
            self.bump_revision();
        }
    }

    fn effective_mix_live(&self) -> bool {
        if let Some(probe) = &self.mix_probe {
            let live = probe();
            let previous = self.mix_live.swap(live, Ordering::AcqRel);
            if previous != live {
                self.bump_revision();
            }
            live
        } else {
            self.mix_live.load(Ordering::Acquire)
        }
    }

    fn notify_waiters(&self, job: &Job) {
        let senders = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.waiters.remove(&job.id).unwrap_or_default()
        };
        for tx in senders {
            let _ = tx.send(job.clone());
        }
    }
}

pub struct JobExecution<'a> {
    job: &'a Job,
    shared: &'a Shared,
    cancelled: Arc<AtomicBool>,
    abort: Arc<AtomicBool>,
}

impl<'a> JobExecution<'a> {
    pub fn job(&self) -> &'a Job {
        self.job
    }

    /// Flag set whenever either the user cancels this job or the runner is shutting down.
    pub fn abort_flag(&self) -> &Arc<AtomicBool> {
        &self.abort
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn has_waiters(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .waiters
            .get(&self.job.id)
            .is_some_and(|waiters| !waiters.is_empty())
    }

    pub fn is_shutdown(&self) -> bool {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .shutdown
    }

    pub fn check_interrupted(&self) -> Result<(), JobInterrupt> {
        if self.is_cancelled() {
            return Err(JobInterrupt::Cancelled);
        }
        if self.is_shutdown() {
            return Err(JobInterrupt::Shutdown);
        }
        Ok(())
    }

    /// Blocks while the user has paused this job, and blocks background jobs (`Tier 1` and
    /// `Tier 2`) while Mix playback is active or an interactive job (`Tier 0` or `Tier 3`) is
    /// waiting or running. Wakes immediately on resume, cancellation, shutdown or Mix stop.
    pub fn wait_if_paused(&self) -> Result<(), JobInterrupt> {
        let backs_off = self.job.tier.backs_off_for_mix();
        loop {
            self.check_interrupted()?;
            let mix_active = backs_off && self.shared.effective_mix_live();
            // The pause flag is read with the coordinator held, as `pause` and `resume` write it,
            // so a resume can never land between the read and the wait.
            let state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let (user_paused, interactive_busy) = {
                let db = self
                    .shared
                    .database
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                (
                    db.is_job_paused(self.job.id).unwrap_or(false),
                    backs_off && db.has_active_interactive_jobs().unwrap_or(false),
                )
            };
            if !user_paused && !mix_active && !interactive_busy {
                return Ok(());
            }
            if self.cancelled.load(Ordering::Acquire) {
                return Err(JobInterrupt::Cancelled);
            }
            if state.shutdown {
                return Err(JobInterrupt::Shutdown);
            }
            if mix_active && self.shared.mix_probe.is_some() {
                drop(self.shared.wake.wait_timeout(state, MIX_POLL_INTERVAL));
            } else {
                drop(self.shared.wake.wait(state));
            }
        }
    }

    pub fn report_progress(
        &self,
        done_items: u64,
        total_items: Option<u64>,
    ) -> Result<(), JobError> {
        let updated = {
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            db.update_job_progress(self.job.id, done_items, total_items)?
        };
        if updated {
            self.shared.bump_progress_throttled();
        }
        Ok(())
    }
}

type JobHandler = Arc<dyn Fn(&JobExecution<'_>) -> JobOutcome + Send + Sync>;

pub struct JobRunner {
    shared: Arc<Shared>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl JobRunner {
    pub fn start<F>(database: Arc<Mutex<Database>>, handler: F) -> Result<Self, JobError>
    where
        F: Fn(&JobExecution<'_>) -> JobOutcome + Send + Sync + 'static,
    {
        Self::start_inner(database, None, Arc::new(handler))
    }

    pub fn start_with_mix_probe<F, P>(
        database: Arc<Mutex<Database>>,
        mix_probe: P,
        handler: F,
    ) -> Result<Self, JobError>
    where
        F: Fn(&JobExecution<'_>) -> JobOutcome + Send + Sync + 'static,
        P: Fn() -> bool + Send + Sync + 'static,
    {
        Self::start_inner(database, Some(Arc::new(mix_probe)), Arc::new(handler))
    }

    fn start_inner(
        database: Arc<Mutex<Database>>,
        mix_probe: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
        handler: JobHandler,
    ) -> Result<Self, JobError> {
        {
            let mut db = database.lock().unwrap_or_else(PoisonError::into_inner);
            db.recover_interrupted_jobs()?;
        }
        let (revisions, _) = watch::channel(0u64);
        let initial_mix = mix_probe.as_ref().is_some_and(|probe| probe());
        let shared = Arc::new(Shared {
            database,
            state: Mutex::new(CoordinatorState {
                active: HashMap::new(),
                waiters: HashMap::new(),
                last_progress_push: None,
                shutdown: false,
            }),
            wake: Condvar::new(),
            mix_live: AtomicBool::new(initial_mix),
            mix_probe,
            revisions,
        });

        let mut workers = Vec::with_capacity(2);
        for (name, interactive) in [("job interactive", true), ("job background", false)] {
            let worker_shared = Arc::clone(&shared);
            let worker_handler = Arc::clone(&handler);
            if let Ok(handle) = thread::Builder::new()
                .name(name.to_owned())
                .spawn(move || run_lane(worker_shared, worker_handler, interactive))
            {
                workers.push(handle);
            }
        }

        shared.wake.notify_all();
        Ok(Self {
            shared,
            workers: Mutex::new(workers),
        })
    }

    pub fn revisions(&self) -> watch::Receiver<u64> {
        let mut receiver = self.shared.revisions.subscribe();
        receiver.mark_unchanged();
        receiver
    }

    pub fn set_mix_live(&self, live: bool) {
        let previous = self.shared.mix_live.swap(live, Ordering::AcqRel);
        if previous != live {
            self.shared.bump_revision();
            self.shared.wake.notify_all();
        }
    }

    pub fn is_mix_live(&self) -> bool {
        self.shared.effective_mix_live()
    }

    pub fn snapshot(&self) -> Result<JobsSnapshot, JobError> {
        let db = self
            .shared
            .database
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Ok(db.jobs_snapshot(RECENT_JOBS_LIMIT)?)
    }

    pub fn enqueue(&self, new_job: &NewJob) -> Result<Job, JobError> {
        let job = {
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            db.enqueue_job(new_job)?
        };
        self.shared.bump_revision();
        self.shared.wake.notify_all();
        Ok(job)
    }

    pub fn enqueue_coalesced(&self, new_job: &NewJob) -> Result<(Job, bool), JobError> {
        let (job, inserted) = {
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            db.enqueue_or_coalesce_job(new_job)?
        };
        if inserted {
            self.shared.bump_revision();
        }
        self.shared.wake.notify_all();
        Ok((job, inserted))
    }

    /// Enqueues (or attaches to a matching queued job) and returns a channel that receives the
    /// terminal `Job` once it finishes, fails or is cancelled.
    pub fn enqueue_coalesced_with_waiter(
        &self,
        new_job: &NewJob,
    ) -> Result<(Job, Receiver<Job>), JobError> {
        let (tx, rx) = mpsc::channel();
        let (job, inserted) = {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let (job, inserted) = db.enqueue_or_coalesce_job(new_job)?;
            state.waiters.entry(job.id).or_default().push(tx);
            (job, inserted)
        };
        if inserted {
            self.shared.bump_revision();
        }
        self.shared.wake.notify_all();
        Ok((job, rx))
    }

    pub fn cancel(&self, job_id: i64) -> Result<Job, JobError> {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut db = self
            .shared
            .database
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let existing = db.job_by_id(job_id)?.ok_or(JobError::NotFound { job_id })?;
        if !existing.status.can_cancel() {
            return Err(JobError::NotCancellable {
                job_id,
                status: existing.status,
            });
        }

        if existing.status == JobStatus::Queued
            && let Some(cancelled) = db.cancel_queued_job(job_id)?
        {
            let senders = state.waiters.remove(&job_id).unwrap_or_default();
            drop(db);
            drop(state);
            for tx in senders {
                let _ = tx.send(cancelled.clone());
            }
            self.shared.bump_revision();
            self.shared.wake.notify_all();
            return Ok(cancelled);
        }

        let wait_rx = if let Some(handle) = state.active.get(&job_id) {
            handle.cancelled.store(true, Ordering::Release);
            handle.abort.store(true, Ordering::Release);
            let (tx, rx) = mpsc::channel();
            state.waiters.entry(job_id).or_default().push(tx);
            Some(rx)
        } else {
            None
        };
        drop(db);
        drop(state);
        self.shared.wake.notify_all();
        if let Some(rx) = wait_rx
            && let Ok(settled) = rx.recv_timeout(Duration::from_millis(500))
        {
            return Ok(settled);
        }
        Ok(existing)
    }

    /// Pauses a queued or running job. A queued one is not started, and a running one waits at
    /// its next call to `wait_if_paused`, keeping its progress.
    pub fn pause(&self, job_id: i64) -> Result<Job, JobError> {
        self.set_paused(job_id, true)
    }

    pub fn resume(&self, job_id: i64) -> Result<Job, JobError> {
        self.set_paused(job_id, false)
    }

    fn set_paused(&self, job_id: i64, paused: bool) -> Result<Job, JobError> {
        let updated = {
            let _state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let existing = db.job_by_id(job_id)?.ok_or(JobError::NotFound { job_id })?;
            if !existing.status.can_pause() {
                return Err(JobError::NotPausable {
                    job_id,
                    status: existing.status,
                });
            }
            db.set_job_paused(job_id, paused)?
                .ok_or(JobError::NotFound { job_id })?
        };
        self.shared.bump_revision();
        self.shared.wake.notify_all();
        Ok(updated)
    }

    pub fn retry(&self, job_id: i64) -> Result<Job, JobError> {
        let retried = {
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let existing = db.job_by_id(job_id)?.ok_or(JobError::NotFound { job_id })?;
            if !existing.status.can_retry() {
                return Err(JobError::NotRetryable {
                    job_id,
                    status: existing.status,
                });
            }
            db.retry_job(job_id)?.ok_or(JobError::NotFound { job_id })?
        };
        self.shared.bump_revision();
        self.shared.wake.notify_all();
        Ok(retried)
    }

    pub fn clear_finished(&self) -> Result<usize, JobError> {
        let removed = {
            let mut db = self
                .shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            db.clear_finished_jobs()?
        };
        if removed > 0 {
            self.shared.bump_revision();
        }
        Ok(removed)
    }
}

impl Drop for JobRunner {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state.shutdown = true;
            state.waiters.clear();
            for handle in state.active.values() {
                handle.abort.store(true, Ordering::Release);
            }
        }
        self.shared.wake.notify_all();
        let handles =
            std::mem::take(&mut *self.workers.lock().unwrap_or_else(PoisonError::into_inner));
        for handle in handles {
            let _ = handle.join();
        }
    }
}

fn run_lane(shared: Arc<Shared>, handler: JobHandler, interactive: bool) {
    loop {
        let claimed = {
            let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
            if state.shutdown {
                return;
            }
            if !interactive && shared.effective_mix_live() {
                let has_bg_work = shared
                    .database
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .has_queued_background_jobs()
                    .unwrap_or(false);
                if has_bg_work && shared.mix_probe.is_some() {
                    drop(shared.wake.wait_timeout(state, MIX_POLL_INTERVAL));
                } else {
                    drop(shared.wake.wait(state));
                }
                continue;
            }

            let mut db = shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if !interactive && db.has_active_interactive_jobs().unwrap_or(false) {
                drop(db);
                drop(shared.wake.wait(state));
                continue;
            }
            match db.claim_next_job(interactive) {
                Ok(Some(job)) => {
                    let cancelled = Arc::new(AtomicBool::new(false));
                    let abort = Arc::new(AtomicBool::new(false));
                    state.active.insert(
                        job.id,
                        ActiveHandle {
                            cancelled: Arc::clone(&cancelled),
                            abort: Arc::clone(&abort),
                        },
                    );
                    (job, cancelled, abort)
                }
                Ok(None) => {
                    drop(db);
                    drop(shared.wake.wait(state));
                    continue;
                }
                Err(_) => {
                    drop(db);
                    drop(shared.wake.wait_timeout(state, Duration::from_millis(100)));
                    continue;
                }
            }
        };

        let (job, cancelled, abort) = claimed;
        shared.bump_revision();
        if interactive {
            shared.wake.notify_all();
        }

        let exec = JobExecution {
            job: &job,
            shared: &shared,
            cancelled: Arc::clone(&cancelled),
            abort: Arc::clone(&abort),
        };
        let outcome = handler(&exec);

        let terminal = {
            let mut state = shared.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.active.remove(&job.id);
            let was_cancelled = cancelled.load(Ordering::Acquire);
            let shutting_down = state.shutdown;
            drop(state);

            if shutting_down && !was_cancelled && matches!(outcome, JobOutcome::Interrupted) {
                return;
            }

            let mut db = shared
                .database
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if was_cancelled || matches!(outcome, JobOutcome::Cancelled) {
                db.mark_running_job_cancelled(job.id).ok().flatten()
            } else {
                match outcome {
                    JobOutcome::Completed {
                        done_items,
                        total_items,
                    } => db
                        .complete_job(job.id, done_items, total_items)
                        .ok()
                        .flatten(),
                    JobOutcome::Failed(message) => db.fail_job(job.id, &message).ok().flatten(),
                    JobOutcome::Cancelled => db.mark_running_job_cancelled(job.id).ok().flatten(),
                    JobOutcome::Interrupted => return,
                }
            }
        };

        if let Some(finished) = terminal {
            shared.notify_waiters(&finished);
            shared.bump_revision();
        }
        shared.wake.notify_all();
    }
}
