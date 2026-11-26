#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::PathBuf;

use resonance_api::Api;
use resonance_api::command::{
    OperationKindName, OperationStateName, ReplaceChoiceName, ReplaceVerdictName, Request, Response,
};
use resonance_db::Database;
use resonance_fixtures::signal::Groove;
use serde_json::{Value, json};

fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn shown_formats(api: &Api) -> Vec<(i64, String)> {
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
    page.tracks
        .into_iter()
        .map(|track| (track.id, track.format))
        .collect()
}

#[test]
fn compares_replaces_and_undoes_through_the_commands() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api-replace");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("music");
    let incoming = root.join("incoming");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&incoming).unwrap();
    let groove = Groove {
        tempo_bpm: 122.0,
        seed: 5,
    };
    groove
        .render(40.0, 44_100, 0.0, 1.0)
        .write_wav(&music.join("old take.wav"))
        .unwrap();
    let better = incoming.join("better.flac");
    groove
        .render(40.01, 48_000, 0.010, 1.0)
        .write_flac(&better)
        .unwrap();
    let api = Api::new_silent(Database::open(&root.join("library.db")).unwrap());
    api.handle(Request::Scan {
        folder: music.display().to_string(),
    })
    .unwrap();
    let track_id = shown_formats(&api)[0].0;

    let Response::CompareReplacement(comparison) = api
        .handle(request(json!({
            "command": "compare_replacement",
            "params": { "track_id": track_id, "path": better.display().to_string() }
        })))
        .unwrap()
    else {
        panic!("expected a comparison");
    };
    assert_eq!(
        comparison.verdict,
        ReplaceVerdictName::SameRecordingAndBetter,
        "a higher sample rate ranks the FLAC above the WAV"
    );
    assert_eq!(comparison.default_choice, ReplaceChoiceName::Replace);
    assert!((comparison.offset_ms.unwrap() - 10.0).abs() < 1.0);
    assert_eq!(comparison.replace_name, "old take.flac");
    assert_eq!(comparison.new.sample_rate, Some(48_000));

    let Response::ApplyReplacement(result) = api
        .handle(request(json!({
            "command": "apply_replacement",
            "params": {
                "track_id": track_id,
                "path": better.display().to_string(),
                "choice": "replace"
            }
        })))
        .unwrap()
    else {
        panic!("expected a replace result");
    };
    assert_eq!(result.track_id, track_id);
    assert_eq!(result.operation.kind, OperationKindName::ReplaceFile);
    assert_eq!(result.operation.state, OperationStateName::Applied);
    assert_eq!(shown_formats(&api), [(track_id, "flac".to_owned())]);
    assert!(music.join("old take.flac").is_file());
    assert!(!music.join("old take.wav").exists());

    let Response::UndoOperation(undone) = api
        .handle(Request::UndoOperation {
            operation_id: result.operation.id,
        })
        .unwrap()
    else {
        panic!("expected an undo report");
    };
    assert_eq!(undone.operation.state, OperationStateName::Undone);
    assert_eq!(shown_formats(&api), [(track_id, "wav".to_owned())]);
    assert!(music.join("old take.wav").is_file());
    assert!(!music.join("old take.flac").exists());
}

#[test]
fn reports_ffmpeg_and_its_presets() {
    let api = Api::new_silent(Database::open_in_memory().unwrap());
    let Response::FfmpegStatus(status) = api.handle(Request::FfmpegStatus).unwrap() else {
        panic!("expected the FFmpeg status");
    };
    assert!(status.version.unwrap().starts_with("ffmpeg version"));
    assert_eq!(status.presets.len(), 3);
    assert_eq!(
        status.presets[0].label,
        "MP3 320 kbps CBR, true peak held under -1 dBTP"
    );
}
