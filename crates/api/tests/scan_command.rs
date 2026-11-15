#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_api::Api;
use resonance_api::command::{Request, Response, ScanReport, TrackPage};
use resonance_api::error::ErrorCode;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;

fn folder(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let unique = format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("api-scan")
        .join(name)
        .join(unique);
    fs::create_dir_all(&folder).unwrap();
    folder
}

fn tone(path: &Path) {
    Signal::tone(440.0, 0.1, -6.0, 48_000)
        .write_wav(path)
        .unwrap();
}

fn scan(api: &Api, folder: &Path) -> ScanReport {
    match api
        .handle(Request::Scan {
            folder: folder.to_str().unwrap().to_owned(),
        })
        .unwrap()
    {
        Response::Scan(report) => report,
        other => panic!("expected a scan report, got {other:?}"),
    }
}

fn tracks(api: &Api) -> TrackPage {
    match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 200,
        })
        .unwrap()
    {
        Response::Tracks(page) => page,
        other => panic!("expected tracks, got {other:?}"),
    }
}

#[test]
fn a_scanned_file_appears_in_the_table_without_changing_the_file() {
    let music = folder("read-only");
    let path = music.join("First Light.wav");
    tone(&path);
    let before = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let api = Api::new(Database::open_in_memory().unwrap());

    let first = scan(&api, &music);
    let page = tracks(&api);
    let second = scan(&api, &music);

    assert_eq!(
        (first.found, first.added, first.changed, first.unchanged),
        (1, 1, 0, 0)
    );
    assert_eq!(first.unreadable_count, 0);
    assert!(first.unreadable.is_empty());
    assert_eq!(page.total, 1);
    assert_eq!(page.tracks[0].title, "First Light");
    assert_eq!(page.tracks[0].audio.as_ref().unwrap().duration_ms, 100);
    assert_eq!(
        (second.found, second.added, second.changed, second.unchanged),
        (1, 0, 0, 1)
    );
    assert_eq!(tracks(&api).total, 1);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
}

#[test]
fn an_empty_folder_returns_an_empty_report_and_library() {
    let music = folder("empty");
    let api = Api::new(Database::open_in_memory().unwrap());

    let report = scan(&api, &music);

    assert_eq!(
        (report.found, report.added, report.changed, report.unchanged),
        (0, 0, 0, 0)
    );
    assert_eq!(report.unreadable_count, 0);
    assert!(report.unreadable.is_empty());
    assert_eq!(tracks(&api).total, 0);
}

#[test]
fn a_missing_folder_names_the_problem_and_how_to_retry() {
    let missing = folder("missing").join("gone");
    let api = Api::new(Database::open_in_memory().unwrap());

    let error = api
        .handle(Request::Scan {
            folder: missing.to_str().unwrap().to_owned(),
        })
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::ScanFailed);
    assert!(error.message.contains("gone"));
    assert!(error.message.contains("readable"));
    assert!(error.message.contains("try again"));
    assert_eq!(tracks(&api).total, 0);
}

#[test]
fn unreadable_files_are_counted_but_the_report_stays_bounded() {
    let music = folder("unreadable");
    for index in 0..103 {
        fs::write(music.join(format!("broken-{index:03}.wav")), b"not audio").unwrap();
    }
    let api = Api::new(Database::open_in_memory().unwrap());

    let report = scan(&api, &music);
    let page = tracks(&api);

    assert_eq!(report.found, 103);
    assert_eq!(report.added, 103);
    assert_eq!(report.unreadable_count, 103);
    assert_eq!(report.unreadable.len(), 100);
    assert_eq!(report.unreadable[0].path, "broken-000.wav");
    assert!(report.unreadable.iter().all(|file| !file.reason.is_empty()));
    assert_eq!(page.total, 103);
    assert!(page.tracks.iter().all(|track| track.audio.is_none()));
}

#[test]
fn scanning_stays_out_of_the_folder_holding_the_app_data() {
    let music = folder("own-data");
    let app_data = music.join("Resonance/data");
    fs::create_dir_all(&app_data).unwrap();
    tone(&music.join("track.wav"));
    tone(&app_data.join("fixture.wav"));
    let api = Api::new(Database::open_in_memory().unwrap()).with_data_folder(app_data);

    let report = scan(&api, &music);

    assert_eq!(report.found, 1);
    assert_eq!(tracks(&api).tracks[0].title, "track");
}

#[tokio::test]
async fn source_commands_and_background_watcher_keep_the_library_in_step() {
    let music = folder("api-sources-watch");
    fs::create_dir_all(music.join("House")).unwrap();
    fs::create_dir_all(music.join("Samples")).unwrap();
    tone(&music.join("House/First.wav"));
    tone(&music.join("Samples/Loop.wav"));

    let api = Api::with_watch_quiet_window(
        Database::open_in_memory().unwrap(),
        std::time::Duration::from_millis(50),
    );
    let mut revisions = api.watch_revisions();

    scan(&api, &music);
    assert_eq!(tracks(&api).total, 2);

    let sources = match api.handle(Request::Sources).unwrap() {
        Response::Sources(list) => list,
        other => panic!("expected sources, got {other:?}"),
    };
    assert_eq!(sources.sources.len(), 1);
    let source_id = sources.sources[0].id;
    assert!(sources.sources[0].watching_active);

    // Exclude Samples via UpdateSource; track count drops to 1 immediately.
    let after_exclude = match api
        .handle(Request::UpdateSource {
            source_id,
            include_subfolders: true,
            watch: true,
            excluded_subfolders: vec!["Samples".to_owned()],
        })
        .unwrap()
    {
        Response::UpdateSource(list) => list,
        other => panic!("expected updated sources, got {other:?}"),
    };
    assert_eq!(after_exclude.sources[0].track_count, 1);
    assert_eq!(tracks(&api).total, 1);

    // Write a new file into House while watching is on; background watcher adds it and bumps revision.
    tone(&music.join("House/Second.wav"));
    tokio::time::timeout(std::time::Duration::from_secs(3), revisions.changed())
        .await
        .expect("watch revision arrives")
        .expect("sender stays open");
    assert_eq!(*revisions.borrow_and_update(), 1);
    assert_eq!(tracks(&api).total, 2);

    // Removing the source clears its tracks and stops watching.
    let after_remove = match api.handle(Request::RemoveSource { source_id }).unwrap() {
        Response::RemoveSource(list) => list,
        other => panic!("expected sources after remove, got {other:?}"),
    };
    assert!(after_remove.sources.is_empty());
    assert_eq!(tracks(&api).total, 0);
}
