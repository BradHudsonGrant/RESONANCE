#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::PathBuf;

use resonance_api::Api;
use resonance_api::command::{
    FieldChange, OperationStateName, Request, Response, TagName, TrackQuery,
};
use resonance_api::error::ErrorCode;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use serde_json::json;

fn request(value: serde_json::Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn titles(api: &Api) -> Vec<String> {
    match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 10,
            query: Some(TrackQuery::default()),
        })
        .unwrap()
    {
        Response::Tracks(page) => page.tracks.into_iter().map(|track| track.title).collect(),
        other => panic!("expected tracks, got {other:?}"),
    }
}

#[test]
fn previews_writes_and_undoes_through_the_journal() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tag_writes_command");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("tone.wav");
    Signal::tone(440.0, 1.0, -6.0, 44_100)
        .write_wav(&path)
        .unwrap();

    let api = Api::new_silent(Database::open_in_memory().unwrap());
    api.handle(Request::Scan {
        folder: folder.display().to_string(),
    })
    .unwrap();
    let Response::Tracks(page) = api
        .handle(Request::Tracks {
            offset: 0,
            limit: 1,
            query: None,
        })
        .unwrap()
    else {
        panic!("expected tracks");
    };
    let track_id = page.tracks[0].id;
    api.handle(request(json!({
        "command": "set_user_tag",
        "params": { "track_ids": [track_id], "edits": [{ "field": "title", "value": "Sunrise" }] }
    })))
    .unwrap();
    let bytes = fs::read(&path).unwrap();

    let Response::PreviewTagWrites(preview) = api
        .handle(request(
            json!({ "command": "preview_tag_writes", "params": {} }),
        ))
        .unwrap()
    else {
        panic!("expected a preview");
    };
    assert_eq!(preview.len(), 1);
    assert_eq!(preview[0].track_id, track_id);
    assert_eq!(preview[0].blocked, None);
    assert_eq!(
        preview[0].fields,
        [FieldChange {
            field: TagName::Title,
            from: None,
            to: Some("Sunrise".to_owned()),
        }]
    );
    assert_eq!(fs::read(&path).unwrap(), bytes, "a preview writes nothing");

    let Response::ApplyTagWrites(report) = api
        .handle(request(json!({
            "command": "apply_tag_writes",
            "params": { "track_ids": [track_id] }
        })))
        .unwrap()
    else {
        panic!("expected a report");
    };
    assert_eq!(report.written, 1);
    assert!(report.failed.is_empty());
    let operation_id = report.operation_id.unwrap();
    assert_ne!(fs::read(&path).unwrap(), bytes);
    assert_eq!(titles(&api), ["Sunrise"]);

    let Response::RecentOperations(recent) = api
        .handle(request(
            json!({ "command": "recent_operations", "params": { "limit": 10 } }),
        ))
        .unwrap()
    else {
        panic!("expected the journal");
    };
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, operation_id);
    assert_eq!(recent[0].state, OperationStateName::Applied);
    assert!(recent[0].can_undo);
    assert!(recent[0].applied_ms.is_some());

    let Response::UndoOperation(undo) = api
        .handle(request(json!({
            "command": "undo_operation",
            "params": { "operation_id": operation_id }
        })))
        .unwrap()
    else {
        panic!("expected an undo report");
    };
    assert_eq!(undo.restored, 1);
    assert_eq!(undo.operation.state, OperationStateName::Undone);
    assert!(!undo.operation.can_undo);
    assert_eq!(
        titles(&api),
        ["Sunrise"],
        "the user value still shows; only the file went back"
    );

    let error = api
        .handle(Request::UndoOperation { operation_id })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BadRequest);
}
