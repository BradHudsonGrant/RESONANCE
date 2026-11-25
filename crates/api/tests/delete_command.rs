#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::PathBuf;

use resonance_api::Api;
use resonance_api::command::{OperationKindName, OperationStateName, Request, Response};
use resonance_db::Database;
use resonance_fixtures::silence::write_silent_mp3;
use serde_json::{Value, json};

fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn track_ids(api: &Api) -> Vec<i64> {
    let Response::Tracks(page) = api
        .handle(Request::Tracks {
            offset: 0,
            limit: 100,
            query: None,
        })
        .unwrap()
    else {
        panic!("expected tracks");
    };
    page.tracks.into_iter().map(|track| track.id).collect()
}

#[test]
fn deletes_tracks_to_the_bin_lists_the_operation_and_undoes_it() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api-delete-to-bin");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    write_silent_mp3(&folder.join("first.mp3"), 0.3).unwrap();
    write_silent_mp3(&folder.join("second.mp3"), 0.6).unwrap();
    let api = Api::new_silent(Database::open_in_memory().unwrap());
    api.handle(Request::Scan {
        folder: folder.display().to_string(),
    })
    .unwrap();
    let ids = track_ids(&api);
    assert_eq!(ids.len(), 2);

    let Response::DeleteTracks(report) = api
        .handle(request(json!({
            "command": "delete_tracks",
            "params": { "track_ids": [ids[0]] }
        })))
        .unwrap()
    else {
        panic!("expected a delete report");
    };
    assert_eq!(report.no_bin, None);
    let operation = report.operation.unwrap();
    assert_eq!(operation.kind, OperationKindName::DeleteFiles);
    assert_eq!(operation.state, OperationStateName::Applied);
    assert!(operation.can_undo);
    assert_eq!(operation.summary, "Moved 1 file to the bin");
    assert_eq!(track_ids(&api), [ids[1]]);
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);

    let Response::RecentOperations(recent) =
        api.handle(Request::RecentOperations { limit: 5 }).unwrap()
    else {
        panic!("expected operations");
    };
    assert_eq!(recent[0].id, operation.id);

    let Response::UndoOperation(undone) = api
        .handle(Request::UndoOperation {
            operation_id: operation.id,
        })
        .unwrap()
    else {
        panic!("expected an undo report");
    };
    assert_eq!(undone.restored, 1);
    assert_eq!(undone.operation.state, OperationStateName::Undone);
    let mut back = track_ids(&api);
    back.sort_unstable();
    assert_eq!(back, ids);
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
}
