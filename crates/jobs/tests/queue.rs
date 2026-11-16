use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use resonance_core::jobs::{
    JOB_KIND_IDENTIFY_LIBRARY, JOB_KIND_SCAN_SOURCE, JobStatus, JobTier, NewJob,
};
use resonance_db::Database;
use resonance_jobs::{JobError, JobInterrupt, JobOutcome, JobRunner};

#[test]
fn startup_recovers_interrupted_running_jobs_and_executes_them() {
    let mut raw_db = Database::open_in_memory().expect("db opens");
    let inserted = raw_db
        .enqueue_job(&NewJob {
            kind: JOB_KIND_SCAN_SOURCE.to_owned(),
            tier: JobTier::Scan,
            label: "Scan crashed folder".to_owned(),
            payload_json: r#"{"folder":"club"}"#.to_owned(),
            total_items: Some(10),
        })
        .expect("enqueues");
    let claimed = raw_db
        .claim_next_job(true)
        .expect("claims")
        .expect("job exists");
    assert_eq!(claimed.status, JobStatus::Running);

    let executed = Arc::new(AtomicUsize::new(0));
    let executed_clone = Arc::clone(&executed);
    let db = Arc::new(Mutex::new(raw_db));

    let runner = JobRunner::start(Arc::clone(&db), move |exec| {
        executed_clone.fetch_add(1, Ordering::SeqCst);
        let _ = exec.report_progress(10, Some(10));
        JobOutcome::Completed {
            done_items: 10,
            total_items: Some(10),
        }
    })
    .expect("runner starts");

    for _ in 0..100 {
        let snap = runner.snapshot().expect("snapshot reads");
        if snap.active.is_empty() && snap.recent.len() == 1 {
            assert_eq!(snap.recent[0].id, inserted.id);
            assert_eq!(snap.recent[0].status, JobStatus::Completed);
            assert_eq!(snap.recent[0].done_items, 10);
            assert_eq!(executed.load(Ordering::SeqCst), 1);
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("interrupted job did not recover and complete within timeout");
}

#[test]
fn coalesces_duplicate_queued_jobs_and_supports_cancel_retry_and_clear_finished() {
    let db = Arc::new(Mutex::new(Database::open_in_memory().expect("db opens")));
    let should_fail = Arc::new(AtomicUsize::new(1));
    let fail_flag = Arc::clone(&should_fail);

    let runner = JobRunner::start(Arc::clone(&db), move |exec| {
        if exec.job().kind == "slow_cancel" {
            for _ in 0..200 {
                if exec.check_interrupted().is_err() {
                    return JobOutcome::Cancelled;
                }
                thread::sleep(Duration::from_millis(5));
            }
            return JobOutcome::Completed {
                done_items: 1,
                total_items: Some(1),
            };
        }
        if fail_flag.fetch_sub(1, Ordering::SeqCst) > 0 {
            return JobOutcome::Failed("Folder disappeared".to_owned());
        }
        JobOutcome::Completed {
            done_items: 5,
            total_items: Some(5),
        }
    })
    .expect("runner starts");

    // Pause background lane via mix_live so queued background jobs coalesce deterministically.
    runner.set_mix_live(true);
    let identify_spec = NewJob {
        kind: JOB_KIND_IDENTIFY_LIBRARY.to_owned(),
        tier: JobTier::BackgroundCpu,
        label: "Identify tracks".to_owned(),
        payload_json: "{}".to_owned(),
        total_items: Some(4),
    };
    let (first_bg, first_inserted) = runner
        .enqueue_coalesced(&identify_spec)
        .expect("first enqueue");
    let (second_bg, second_inserted) = runner
        .enqueue_coalesced(&identify_spec)
        .expect("second enqueue coalesces");
    assert!(first_inserted);
    assert!(!second_inserted);
    assert_eq!(first_bg.id, second_bg.id);

    // Cancel the queued background job directly.
    let cancelled_queued = runner.cancel(first_bg.id).expect("queued cancel succeeds");
    assert_eq!(cancelled_queued.status, JobStatus::Cancelled);
    runner.set_mix_live(false);

    // Cancel an actively running job.
    let running_job = runner
        .enqueue(&NewJob {
            kind: "slow_cancel".to_owned(),
            tier: JobTier::Scan,
            label: "Slow scan".to_owned(),
            payload_json: "{}".to_owned(),
            total_items: Some(100),
        })
        .expect("slow job enqueues");
    for _ in 0..50 {
        let snap = runner.snapshot().expect("snapshot");
        if snap
            .active
            .iter()
            .any(|j| j.id == running_job.id && j.status == JobStatus::Running)
        {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let cancelled_running = runner
        .cancel(running_job.id)
        .expect("running cancel succeeds");
    assert_eq!(cancelled_running.status, JobStatus::Cancelled);

    // Fail a job, then retry it to completion.
    let (_, fail_rx) = runner
        .enqueue_coalesced_with_waiter(&NewJob {
            kind: JOB_KIND_SCAN_SOURCE.to_owned(),
            tier: JobTier::Scan,
            label: "Scan missing".to_owned(),
            payload_json: r#"{"folder":"missing"}"#.to_owned(),
            total_items: None,
        })
        .expect("fail job enqueues");
    let failed = fail_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("failed job settles");
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_message.as_deref(), Some("Folder disappeared"));
    assert!(matches!(
        runner.cancel(failed.id),
        Err(JobError::NotCancellable { .. })
    ));

    let retried = runner.retry(failed.id).expect("retry re-queues job");
    assert_eq!(retried.id, failed.id);
    for _ in 0..100 {
        let snap = runner.snapshot().expect("snapshot");
        if snap
            .recent
            .iter()
            .any(|j| j.id == failed.id && j.status == JobStatus::Completed)
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let snap_before_clear = runner.snapshot().expect("snapshot");
    assert_eq!(snap_before_clear.recent.len(), 3);
    let cleared = runner.clear_finished().expect("clears finished");
    assert_eq!(cleared, 3);
    assert!(runner.snapshot().expect("snapshot").recent.is_empty());
}

#[test]
fn mix_live_pauses_running_tier1_work_and_resumes_when_mix_stops_while_tier0_runs() {
    let db = Arc::new(Mutex::new(Database::open_in_memory().expect("db opens")));
    let batch1_done = Arc::new(Barrier::new(2));
    let batch1_worker = Arc::clone(&batch1_done);
    let tier1_batches = Arc::new(AtomicUsize::new(0));
    let tier1_batches_worker = Arc::clone(&tier1_batches);

    let runner = JobRunner::start(Arc::clone(&db), move |exec| {
        if exec.job().tier == JobTier::Scan {
            return JobOutcome::Completed {
                done_items: 3,
                total_items: Some(3),
            };
        }
        // Batch 1
        if exec.wait_if_paused() == Err(JobInterrupt::Cancelled) {
            return JobOutcome::Cancelled;
        }
        tier1_batches_worker.fetch_add(1, Ordering::SeqCst);
        batch1_worker.wait();
        // Give test thread a moment to flip mix_live = true before batch 2 check.
        thread::sleep(Duration::from_millis(30));
        match exec.wait_if_paused() {
            Ok(()) => {}
            Err(JobInterrupt::Cancelled) => return JobOutcome::Cancelled,
            Err(JobInterrupt::Shutdown) => return JobOutcome::Interrupted,
        }
        tier1_batches_worker.fetch_add(1, Ordering::SeqCst);
        JobOutcome::Completed {
            done_items: 2,
            total_items: Some(2),
        }
    })
    .expect("runner starts");

    let (_, bg_rx) = runner
        .enqueue_coalesced_with_waiter(&NewJob {
            kind: JOB_KIND_IDENTIFY_LIBRARY.to_owned(),
            tier: JobTier::BackgroundCpu,
            label: "Identify library".to_owned(),
            payload_json: "{}".to_owned(),
            total_items: Some(2),
        })
        .expect("enqueues tier 1");

    batch1_done.wait();
    runner.set_mix_live(true);

    // Even while Mix is live and Tier 1 is paused mid-job, a Tier 0 scan still runs immediately!
    let (_, scan_rx) = runner
        .enqueue_coalesced_with_waiter(&NewJob {
            kind: JOB_KIND_SCAN_SOURCE.to_owned(),
            tier: JobTier::Scan,
            label: "Scan folder".to_owned(),
            payload_json: r#"{"folder":"live"}"#.to_owned(),
            total_items: Some(3),
        })
        .expect("enqueues tier 0");
    let scan_done = scan_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("tier 0 scan completes while mix is live");
    assert_eq!(scan_done.status, JobStatus::Completed);

    // Tier 1 remains paused on batch 1 while mix_live is true.
    thread::sleep(Duration::from_millis(80));
    assert_eq!(tier1_batches.load(Ordering::SeqCst), 1);

    // Stopping Mix resumes Tier 1 automatically.
    runner.set_mix_live(false);
    let bg_done = bg_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("tier 1 resumes and completes");
    assert_eq!(bg_done.status, JobStatus::Completed);
    assert_eq!(tier1_batches.load(Ordering::SeqCst), 2);
}
