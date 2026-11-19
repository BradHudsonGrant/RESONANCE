use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use resonance_api::Api;
use resonance_api::command::{ActivityJobStatus, ActivitySnapshot, MixDeck, Request, Response};
use resonance_core::jobs::{JOB_KIND_SCAN_SOURCE, JobTier, NewJob};
use resonance_db::Database;
use resonance_fixtures::signal::Signal;

fn snapshot(api: &Api) -> ActivitySnapshot {
    match api
        .handle(Request::JobsSnapshot)
        .expect("snapshot succeeds")
    {
        Response::JobsSnapshot(snapshot) => snapshot,
        other => panic!("expected JobsSnapshot, got {other:?}"),
    }
}

fn wait_until(timeout: Duration, mut predicate: impl FnMut() -> bool) {
    let started = Instant::now();
    while !predicate() {
        assert!(
            started.elapsed() < timeout,
            "condition did not settle within {timeout:?}"
        );
        thread::sleep(Duration::from_millis(15));
    }
}

#[test]
fn scan_queues_identity_pass_and_failed_scan_retries_cleanly_before_clearing_history() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_jobs_scan_retry");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Warehouse");
    fs::create_dir_all(&music).expect("music dir creates");
    Signal::tone(220.0, 1.5, -9.0, 44_100)
        .write_wav(&music.join("opener.wav"))
        .expect("wav writes");

    let api = Api::new_silent(Database::open_in_memory().expect("db opens"));
    let initial = snapshot(&api);
    assert!(initial.active.is_empty());
    assert!(initial.recent.is_empty());
    assert!(!initial.mix_backoff_active);

    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .expect("scan succeeds");

    wait_until(Duration::from_secs(5), || {
        let snap = snapshot(&api);
        snap.active.is_empty()
            && snap
                .recent
                .iter()
                .filter(|job| job.status == ActivityJobStatus::Completed)
                .count()
                >= 2
    });

    let after_scan = snapshot(&api);
    assert!(
        after_scan
            .recent
            .iter()
            .any(|job| job.kind == "scan_source" && job.done_items == 1)
    );
    assert!(
        after_scan
            .recent
            .iter()
            .any(|job| job.kind == "identify_library" && job.done_items == 1)
    );

    // Scanning a folder that does not exist yet records a Failed job with an error message.
    let unplugged = root.join("ExternalDrive");
    assert!(
        api.handle(Request::Scan {
            folder: unplugged.to_string_lossy().into_owned(),
        })
        .is_err()
    );

    let with_failure = snapshot(&api);
    let failed = with_failure
        .recent
        .iter()
        .find(|job| job.status == ActivityJobStatus::Failed)
        .expect("failed scan recorded");
    assert!(
        failed
            .error_message
            .as_deref()
            .is_some_and(|msg| msg.contains("ExternalDrive"))
    );
    let failed_id = failed.id;

    // Create the folder with a track and retry the failed job from Activity.
    fs::create_dir_all(&unplugged).expect("external dir creates");
    Signal::tone(330.0, 1.0, -9.0, 44_100)
        .write_wav(&unplugged.join("closer.wav"))
        .expect("closer writes");

    match api
        .handle(Request::RetryJob { job_id: failed_id })
        .expect("retry succeeds")
    {
        Response::RetryJob(_) => {}
        other => panic!("expected RetryJob, got {other:?}"),
    }

    wait_until(Duration::from_secs(5), || {
        let snap = snapshot(&api);
        snap.active.is_empty()
            && snap
                .recent
                .iter()
                .any(|job| job.id == failed_id && job.status == ActivityJobStatus::Completed)
    });

    match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 10,
            query: None,
        })
        .expect("tracks query succeeds")
    {
        Response::Tracks(page) => assert_eq!(page.total, 2),
        other => panic!("expected Tracks, got {other:?}"),
    }

    let cleared = match api
        .handle(Request::ClearFinishedJobs)
        .expect("clear succeeds")
    {
        Response::ClearFinishedJobs(snap) => snap,
        other => panic!("expected ClearFinishedJobs, got {other:?}"),
    };
    assert!(cleared.recent.is_empty());
}

#[test]
fn mix_deck_playback_pauses_tier1_identity_allows_cancel_and_resumes_when_stopped() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_jobs_mix_backoff");
    let _ = fs::remove_dir_all(&root);
    let deck_dir = root.join("Deck");
    let batch_dir = root.join("Incoming");
    fs::create_dir_all(&deck_dir).expect("deck dir creates");
    fs::create_dir_all(&batch_dir).expect("batch dir creates");

    let deck_track = deck_dir.join("playing_deck.wav");
    Signal::tone(220.0, 4.0, -6.0, 44_100)
        .write_wav(&deck_track)
        .expect("deck wav writes");
    Signal::tone(440.0, 1.5, -6.0, 44_100)
        .write_wav(&batch_dir.join("new_cut.wav"))
        .expect("batch wav writes");

    let api = Api::new_silent(Database::open_in_memory().expect("db opens"));
    api.handle(Request::MixLoad {
        deck: MixDeck::A,
        path: deck_track.to_string_lossy().into_owned(),
    })
    .expect("mix load succeeds");
    api.handle(Request::MixPlay {
        deck: MixDeck::A,
        playing: true,
    })
    .expect("mix play succeeds");

    // Tier 0 scan runs to completion even while Deck A is playing, while Tier 1 identify pauses.
    let scan_response = api
        .handle(Request::Scan {
            folder: batch_dir.to_string_lossy().into_owned(),
        })
        .expect("tier 0 scan completes during mix playback");
    match scan_response {
        Response::Scan(report) => assert_eq!(report.added, 1),
        other => panic!("expected Scan, got {other:?}"),
    }

    thread::sleep(Duration::from_millis(80));
    let during_mix = snapshot(&api);
    assert!(during_mix.mix_backoff_active);
    let paused_identify = during_mix
        .active
        .iter()
        .find(|job| job.kind == "identify_library")
        .expect("identify job stays queued/paused while mix plays");
    assert!(paused_identify.paused_for_mix);
    let identify_id = paused_identify.id;

    // Cancelling the paused background job works immediately, and retrying re-queues it under backoff.
    let after_cancel = match api
        .handle(Request::CancelJob {
            job_id: identify_id,
        })
        .expect("cancel succeeds")
    {
        Response::CancelJob(snap) => snap,
        other => panic!("expected CancelJob, got {other:?}"),
    };
    assert!(after_cancel.active.is_empty());
    assert!(
        after_cancel
            .recent
            .iter()
            .any(|job| job.id == identify_id && job.status == ActivityJobStatus::Cancelled)
    );

    api.handle(Request::RetryJob {
        job_id: identify_id,
    })
    .expect("retry succeeds");
    assert_eq!(snapshot(&api).active.len(), 1);

    // Stopping Deck A clears Mix backoff and lets Tier 1 identity finish automatically.
    api.handle(Request::MixPlay {
        deck: MixDeck::A,
        playing: false,
    })
    .expect("mix stop succeeds");

    wait_until(Duration::from_secs(5), || {
        let snap = snapshot(&api);
        !snap.mix_backoff_active
            && snap.active.is_empty()
            && snap
                .recent
                .iter()
                .any(|job| job.id == identify_id && job.status == ActivityJobStatus::Completed)
    });
}

#[test]
fn interrupted_running_scan_job_recovers_and_completes_on_api_startup() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_jobs_crash_recovery");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Recovered");
    fs::create_dir_all(&music).expect("music dir creates");
    Signal::tone(261.63, 1.0, -9.0, 44_100)
        .write_wav(&music.join("recovered_cut.wav"))
        .expect("wav writes");

    let db_path = root.join("resonance.db");
    let interrupted_id = {
        let mut db = Database::open(&db_path).expect("db opens");
        let payload_json = serde_json::json!({ "folder": music.to_string_lossy() }).to_string();
        let queued = db
            .enqueue_job(&NewJob {
                kind: JOB_KIND_SCAN_SOURCE.to_owned(),
                tier: JobTier::Scan,
                label: "Scan Recovered".to_owned(),
                payload_json,
                total_items: None,
            })
            .expect("enqueues");
        let claimed = db
            .claim_next_job(true)
            .expect("claims")
            .expect("job present");
        assert_eq!(claimed.id, queued.id);
        claimed.id
    };

    let api = Api::new_silent(Database::open(&db_path).expect("reopens db"));
    wait_until(Duration::from_secs(5), || {
        let snap = snapshot(&api);
        snap.active.is_empty()
            && snap
                .recent
                .iter()
                .any(|job| job.id == interrupted_id && job.status == ActivityJobStatus::Completed)
            && snap.recent.iter().any(|job| {
                job.kind == "identify_library" && job.status == ActivityJobStatus::Completed
            })
    });

    match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 10,
            query: None,
        })
        .expect("tracks query succeeds")
    {
        Response::Tracks(page) => assert_eq!(page.total, 1),
        other => panic!("expected Tracks, got {other:?}"),
    }
}

#[test]
fn a_job_paused_from_activity_waits_until_resumed() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_jobs_pause_resume");
    let _ = fs::remove_dir_all(&root);
    let deck_dir = root.join("Deck");
    let batch_dir = root.join("Incoming");
    fs::create_dir_all(&deck_dir).expect("deck dir creates");
    fs::create_dir_all(&batch_dir).expect("batch dir creates");
    let deck_track = deck_dir.join("playing_deck.wav");
    Signal::tone(220.0, 4.0, -6.0, 44_100)
        .write_wav(&deck_track)
        .expect("deck wav writes");
    Signal::tone(440.0, 1.5, -6.0, 44_100)
        .write_wav(&batch_dir.join("new_cut.wav"))
        .expect("batch wav writes");

    // Mix playback holds the identity job in the queue, so it can be paused before it starts.
    let api = Api::new_silent(Database::open_in_memory().expect("db opens"));
    api.handle(Request::MixLoad {
        deck: MixDeck::A,
        path: deck_track.to_string_lossy().into_owned(),
    })
    .expect("mix load succeeds");
    api.handle(Request::MixPlay {
        deck: MixDeck::A,
        playing: true,
    })
    .expect("mix play succeeds");
    api.handle(Request::Scan {
        folder: batch_dir.to_string_lossy().into_owned(),
    })
    .expect("scan succeeds");
    let identify_id = snapshot(&api)
        .active
        .iter()
        .find(|job| job.kind == "identify_library")
        .expect("identify job waits")
        .id;

    let Response::PauseJob(paused) = api
        .handle(Request::PauseJob {
            job_id: identify_id,
        })
        .expect("pause succeeds")
    else {
        panic!("expected PauseJob");
    };
    let job = paused
        .active
        .iter()
        .find(|job| job.id == identify_id)
        .expect("still active");
    assert!(job.paused);
    assert!(
        !job.paused_for_mix,
        "a job the user paused reads as paused by the user"
    );

    api.handle(Request::MixPlay {
        deck: MixDeck::A,
        playing: false,
    })
    .expect("mix stop succeeds");
    thread::sleep(Duration::from_millis(200));
    let held = snapshot(&api);
    let job = held
        .active
        .iter()
        .find(|job| job.id == identify_id)
        .expect("stays queued while paused");
    assert_eq!(job.status, ActivityJobStatus::Queued);

    let Response::ResumeJob(_) = api
        .handle(Request::ResumeJob {
            job_id: identify_id,
        })
        .expect("resume succeeds")
    else {
        panic!("expected ResumeJob");
    };
    wait_until(Duration::from_secs(5), || {
        snapshot(&api)
            .recent
            .iter()
            .any(|job| job.id == identify_id && job.status == ActivityJobStatus::Completed)
    });

    let refused = api
        .handle(Request::PauseJob {
            job_id: identify_id,
        })
        .expect_err("a finished job cannot be paused");
    assert!(refused.message.contains("cannot be paused"));
}
