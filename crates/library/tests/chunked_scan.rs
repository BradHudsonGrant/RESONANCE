use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use resonance_core::search::TrackQuery;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::scan::{ScanError, scan, scan_chunked_with_batch_size};

fn write_tone(path: &std::path::Path, seconds: f64) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent dir creates");
    }
    Signal::tone_at_loudness(-12.0, seconds, 44_100)
        .write_wav(path)
        .expect("wav writes");
}

#[test]
fn cancelling_mid_scan_keeps_committed_batches_and_never_marks_unscanned_files_missing() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cancel_mid_scan");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("root creates");

    // Seed two existing tracks in the library first (including one at the end of sort order, and
    // one whose file was deleted before the cancelled rescan).
    let existing_a = root.join("00_existing_first.wav");
    let existing_deleted = root.join("98_deleted_before_cancel.wav");
    let existing_z = root.join("99_existing_last.wav");
    write_tone(&existing_a, 0.25);
    write_tone(&existing_deleted, 0.25);
    write_tone(&existing_z, 0.25);

    let mut initial_db = Database::open_in_memory().expect("db opens");
    let initial = scan(&mut initial_db, &root, &[]).expect("initial scan succeeds");
    assert_eq!(initial.added, 3);

    // Remove one existing file and add six new files so batch_size = 2 runs multiple batches.
    fs::remove_file(&existing_deleted).expect("deletes file");
    for index in 1..=6 {
        write_tone(&root.join(format!("10_new_{index:02}.wav")), 0.25);
    }

    let database = Mutex::new(initial_db);
    let cancel = AtomicBool::new(false);
    let mut batches_seen = 0usize;

    let outcome = scan_chunked_with_batch_size(&database, &root, &[], 2, &cancel, |progress| {
        if progress.done_files > 0 {
            batches_seen += 1;
            if batches_seen == 2 {
                cancel.store(true, Ordering::Release);
            }
        }
    });

    assert!(matches!(outcome, Err(ScanError::Cancelled)));
    assert_eq!(batches_seen, 2);

    let db = database.lock().unwrap_or_else(PoisonError::into_inner);
    let rows = db.tracks().expect("tracks read");
    // Initial 3 tracks + 3 newly committed tracks from batches 1..2 (00_existing_first + 3 new = 4
    // walked files across two 2-file batches) stay intact, and NONE are marked missing!
    assert_eq!(rows.len(), 6);
    assert!(
        rows.iter().all(|row| row.missing_since.is_none()),
        "cancelled scan must never mark unseen files missing: {rows:?}"
    );
}

#[test]
fn concurrent_tracks_page_reads_complete_promptly_during_multi_batch_scan() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("concurrent_scan_reads");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("root creates");

    for index in 0..24 {
        write_tone(&root.join(format!("track_{index:03}.wav")), 0.2);
    }

    let database = Arc::new(Mutex::new(Database::open_in_memory().expect("db opens")));
    let reader_db = Arc::clone(&database);
    let scan_started = Arc::new(Barrier::new(2));
    let reader_start = Arc::clone(&scan_started);
    let scan_finished = Arc::new(AtomicBool::new(false));
    let reader_done = Arc::clone(&scan_finished);
    let reads_completed = Arc::new(AtomicUsize::new(0));
    let reads_counter = Arc::clone(&reads_completed);

    let reader = thread::spawn(move || {
        reader_start.wait();
        let mut max_read = Duration::ZERO;
        let mut saw_partial_progress = false;
        while !reader_done.load(Ordering::Acquire) {
            let started = Instant::now();
            let count = {
                let db = reader_db.lock().unwrap_or_else(PoisonError::into_inner);
                let total = db.track_count().expect("count reads");
                let page = db
                    .query_tracks_page(&TrackQuery::default(), 0, 50)
                    .expect("page reads");
                assert_eq!(page.len() as u32, total);
                total
            };
            max_read = max_read.max(started.elapsed());
            if count > 0 && count < 24 {
                saw_partial_progress = true;
            }
            reads_counter.fetch_add(1, Ordering::Relaxed);
            thread::sleep(Duration::from_millis(1));
        }
        (max_read, saw_partial_progress)
    });

    let cancel = AtomicBool::new(false);
    let mut started_barrier = Some(scan_started);
    let report = scan_chunked_with_batch_size(&database, &root, &[], 4, &cancel, |progress| {
        if let Some(barrier) = started_barrier.take() {
            barrier.wait();
        }
        if progress.done_files > 0 && progress.done_files < progress.total_files {
            // Simulate realistic disk gap between batch commits without holding the DB mutex.
            thread::sleep(Duration::from_millis(5));
        }
    })
    .expect("chunked scan succeeds");
    scan_finished.store(true, Ordering::Release);

    let (max_read, saw_partial_progress) = reader.join().expect("reader joins");
    assert_eq!(report.found, 24);
    assert_eq!(report.added, 24);
    assert!(
        reads_completed.load(Ordering::Relaxed) >= 4,
        "reader should complete multiple reads while scan runs"
    );
    assert!(
        saw_partial_progress,
        "reader should observe incremental batch commits during scan"
    );
    assert!(
        max_read < Duration::from_millis(25),
        "expected sub-25ms concurrent page reads, got {max_read:?}"
    );
}
