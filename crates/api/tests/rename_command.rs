#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use resonance_api::Api;
use resonance_api::command::{
    ActivityJobStatus, OperationKindName, OperationStateName, RenameStatusName, Request, Response,
};
use resonance_api::error::ErrorCode;
use resonance_db::Database;
use resonance_fixtures::silence::write_silent_mp3;
use serde_json::{Value, json};

fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn folder(name: &str, files: usize) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    for index in 0..files {
        write_silent_mp3(&folder.join(format!("{index:03}.mp3")), 0.2).unwrap();
    }
    folder
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Scans `folder` and gives each track the artist "Artist" and a title from its file name.
fn scanned(folder: &Path) -> (Api, Vec<i64>) {
    let api = Api::new_silent(Database::open_in_memory().unwrap());
    api.handle(Request::Scan {
        folder: folder.display().to_string(),
    })
    .unwrap();
    let Response::Tracks(page) = api
        .handle(Request::Tracks {
            offset: 0,
            limit: 1_000,
            query: None,
        })
        .unwrap()
    else {
        panic!("expected tracks");
    };
    let mut ids = Vec::new();
    for track in page.tracks {
        let title = format!("Title {}", track.title);
        api.handle(request(json!({
            "command": "set_user_tag",
            "params": {
                "track_ids": [track.id],
                "edits": [
                    { "field": "artist", "value": "Artist" },
                    { "field": "title", "value": title }
                ]
            }
        })))
        .unwrap();
        ids.push(track.id);
    }
    (api, ids)
}

#[test]
fn previews_renames_the_selection_and_undoes() {
    let folder = folder("rename_command", 2);
    let (api, ids) = scanned(&folder);

    let Response::PreviewRename(preview) = api
        .handle(request(json!({
            "command": "preview_rename",
            "params": {
                "scope": { "kind": "tracks", "track_ids": [ids[0]] },
                "template": "{artist} - {title} ({mix})",
                "limit": 10
            }
        })))
        .unwrap()
    else {
        panic!("expected a preview");
    };
    assert_eq!(preview.total, 1);
    assert_eq!(preview.ready, 1);
    assert_eq!(preview.files[0].status, RenameStatusName::Ready);
    assert_eq!(preview.files[0].old_name, "000.mp3");
    assert_eq!(
        preview.files[0].new_name.as_deref(),
        Some("Artist - Title 000.mp3")
    );
    assert_eq!(names(&folder), ["000.mp3", "001.mp3"], "a preview renames nothing");

    let Response::ApplyRename(report) = api
        .handle(request(json!({
            "command": "apply_rename",
            "params": {
                "scope": { "kind": "tracks", "track_ids": [ids[0]] },
                "template": "{artist} - {title} ({mix})"
            }
        })))
        .unwrap()
    else {
        panic!("expected a report");
    };
    assert_eq!(report.renamed, 1);
    assert!(!report.queued);
    assert_eq!(names(&folder), ["001.mp3", "Artist - Title 000.mp3"]);

    let Response::RecentOperations(recent) = api
        .handle(request(
            json!({ "command": "recent_operations", "params": { "limit": 5 } }),
        ))
        .unwrap()
    else {
        panic!("expected the journal");
    };
    assert_eq!(recent[0].kind, OperationKindName::RenameFiles);
    assert_eq!(recent[0].summary, "Renamed 1 file");

    let Response::UndoOperation(undo) = api
        .handle(Request::UndoOperation {
            operation_id: report.operation_id.unwrap(),
        })
        .unwrap()
    else {
        panic!("expected an undo report");
    };
    assert_eq!(undo.restored, 1);
    assert_eq!(undo.operation.state, OperationStateName::Undone);
    assert_eq!(names(&folder), ["000.mp3", "001.mp3"]);
}

#[test]
fn a_template_that_does_not_parse_is_refused() {
    let folder = folder("rename_command_bad_template", 1);
    let (api, _) = scanned(&folder);
    let error = api
        .handle(request(json!({
            "command": "preview_rename",
            "params": {
                "scope": { "kind": "query", "query": {} },
                "template": "{artist} - {genre}",
                "limit": 10
            }
        })))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(error.message.contains("{genre} is not a token"));
}

#[test]
fn the_whole_library_renames_as_one_activity_job() {
    let folder = folder("rename_command_job", 201);
    let (api, _) = scanned(&folder);

    let Response::ApplyRename(report) = api
        .handle(request(json!({
            "command": "apply_rename",
            "params": {
                "scope": { "kind": "query", "query": {} },
                "template": "{artist} - {title}"
            }
        })))
        .unwrap()
    else {
        panic!("expected a report");
    };
    assert!(report.queued);

    let started = Instant::now();
    let job = loop {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            panic!("expected a snapshot");
        };
        if let Some(job) = snapshot
            .recent
            .into_iter()
            .find(|job| job.kind == "rename_files")
        {
            break job;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "the job finishes");
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(job.status, ActivityJobStatus::Completed);
    assert_eq!(job.done_items, 201);
    assert_eq!(job.label, "Rename 201 files");
    assert!(names(&folder).iter().all(|name| name.starts_with("Artist - Title ")));

    let Response::RecentOperations(recent) = api
        .handle(request(
            json!({ "command": "recent_operations", "params": { "limit": 5 } }),
        ))
        .unwrap()
    else {
        panic!("expected the journal");
    };
    let Response::UndoOperation(undo) = api
        .handle(Request::UndoOperation {
            operation_id: recent[0].id,
        })
        .unwrap()
    else {
        panic!("expected an undo report");
    };
    assert_eq!(undo.restored, 201);
    assert_eq!(names(&folder)[0], "000.mp3");
}
