#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use resonance_api::Api;
use resonance_api::command::{
    ProposalState, Request, Response, TagName, TagProvenance, TrackQuery, TrackStagingDiff,
};
use resonance_api::error::ErrorCode;
use resonance_core::library::{AudioFormat, ScannedFile, TagField};
use resonance_core::staging::Proposal;
use resonance_db::Database;
use serde_json::json;

fn library() -> (Api, Vec<i64>) {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("D:\\Club").unwrap();
    let file = |path: &str, title: &str| ScannedFile {
        path: path.to_owned(),
        size_bytes: 1,
        modified_ns: 0,
        format: AudioFormat::Mp3,
        audio: None,
        tags: vec![
            (TagField::Title, title.to_owned()),
            (TagField::Key, "Am".to_owned()),
        ],
    };
    database
        .record_scan(source, &[file("a.mp3", "Opener"), file("b.mp3", "Closer")])
        .unwrap();
    let ids: Vec<i64> = database
        .tracks()
        .unwrap()
        .iter()
        .map(|row| row.track_id)
        .collect();
    database
        .store_proposals(
            ids[0],
            &[Proposal {
                field: TagField::CatNo,
                value: "DEF023".to_owned(),
                provider: "discogs".to_owned(),
                confidence: 0.93,
                audio_matched: false,
            }],
        )
        .unwrap();
    (Api::new_silent(database), ids)
}

fn request(value: serde_json::Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn diff(api: &Api, track_id: i64) -> TrackStagingDiff {
    match api.handle(Request::StagingDiff { track_id }).unwrap() {
        Response::StagingDiff(diff) => diff,
        other => panic!("expected a staging diff, got {other:?}"),
    }
}

fn titles(api: &Api, text: &str) -> Vec<String> {
    match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 10,
            query: Some(TrackQuery {
                text: Some(text.to_owned()),
                ..TrackQuery::default()
            }),
        })
        .unwrap()
    {
        Response::Tracks(page) => page.tracks.into_iter().map(|track| track.title).collect(),
        other => panic!("expected tracks, got {other:?}"),
    }
}

#[test]
fn the_queue_and_diff_show_a_proposal_and_accepting_it_makes_it_the_users() {
    let (api, ids) = library();

    match api
        .handle(request(
            json!({ "command": "staging_queue", "params": { "offset": 0, "limit": 20 } }),
        ))
        .unwrap()
    {
        Response::StagingQueue(page) => {
            assert_eq!(page.total, 1);
            assert_eq!(page.items[0].track_id, ids[0]);
            assert_eq!(page.items[0].pending_fields, 1);
        }
        other => panic!("expected the staging queue, got {other:?}"),
    }
    let before = diff(&api, ids[0]);
    assert_eq!(before.fields.len(), 13);
    let catno = before
        .fields
        .iter()
        .find(|field| field.field == TagName::CatNo)
        .unwrap();
    assert_eq!(catno.current_value, None);
    assert_eq!(catno.proposed_value.as_deref(), Some("DEF023"));
    assert_eq!(catno.provider.as_deref(), Some("discogs"));
    assert_eq!(catno.status, Some(ProposalState::Pending));
    assert!(catno.changes);
    let key = before
        .fields
        .iter()
        .find(|field| field.field == TagName::Key)
        .unwrap();
    assert_eq!(key.current_provenance, Some(TagProvenance::File));

    let after = match api
        .handle(request(json!({
            "command": "accept_proposed_fields",
            "params": { "track_id": ids[0], "fields": ["catno"] }
        })))
        .unwrap()
    {
        Response::AcceptProposedFields(diff) => diff,
        other => panic!("expected a staging diff, got {other:?}"),
    };
    let catno = after
        .fields
        .iter()
        .find(|field| field.field == TagName::CatNo)
        .unwrap();
    assert_eq!(catno.current_value.as_deref(), Some("DEF023"));
    assert_eq!(catno.current_provenance, Some(TagProvenance::User));
    assert_eq!(catno.status, Some(ProposalState::Accepted));
    assert!(catno.locked_by_user);
    assert!(!catno.changes);
}

#[test]
fn user_tags_on_many_tracks_show_in_the_table_and_search_at_once() {
    let (api, ids) = library();

    let report = api
        .handle(request(json!({
            "command": "set_user_tag",
            "params": {
                "track_ids": ids,
                "edits": [
                    { "field": "artist", "value": "Night Shift" },
                    { "field": "key", "value": "1d" }
                ]
            }
        })))
        .unwrap();
    assert!(matches!(report, Response::SetUserTag(report) if report.tracks == 2));

    assert_eq!(titles(&api, "shift"), ["Opener", "Closer"]);
    let tracks = match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 10,
            query: None,
        })
        .unwrap()
    {
        Response::Tracks(page) => page.tracks,
        other => panic!("expected tracks, got {other:?}"),
    };
    assert!(tracks.iter().all(|track| {
        track.artist.as_deref() == Some("Night Shift")
            && track.key.as_ref().map(|key| key.camelot.as_str()) == Some("8B")
    }));

    api.handle(Request::SetUserTag {
        track_ids: vec![ids[1]],
        edits: vec![resonance_api::command::UserTagEdit {
            field: TagName::Title,
            value: Some("Encore".to_owned()),
        }],
    })
    .unwrap();
    assert_eq!(titles(&api, "encore"), ["Encore"]);
    assert!(titles(&api, "closer").is_empty());
}

#[test]
fn a_value_that_does_not_fit_its_field_is_refused_and_nothing_changes() {
    let (api, ids) = library();

    let error = api
        .handle(request(json!({
            "command": "set_user_tag",
            "params": {
                "track_ids": ids,
                "edits": [
                    { "field": "title", "value": "Changed" },
                    { "field": "bpm", "value": "fast" }
                ]
            }
        })))
        .unwrap_err();

    assert_eq!(error.code, ErrorCode::BadRequest);
    assert!(error.message.contains("\"fast\" is not a BPM"));
    assert!(titles(&api, "changed").is_empty());
    let error = api
        .handle(Request::AcceptAllAboveThreshold {
            min_confidence: 1.5,
        })
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BadRequest);
}

#[test]
fn accepting_above_a_threshold_and_skipping_report_what_they_did() {
    let (api, ids) = library();

    let skipped = match api
        .handle(request(
            json!({ "command": "skip_proposal", "params": { "track_id": ids[0] } }),
        ))
        .unwrap()
    {
        Response::SkipProposal(diff) => diff,
        other => panic!("expected a staging diff, got {other:?}"),
    };
    assert!(skipped.fields.iter().all(|field| !field.changes));
    match api
        .handle(Request::AcceptAllAboveThreshold {
            min_confidence: 0.9,
        })
        .unwrap()
    {
        Response::AcceptAllAboveThreshold(report) => {
            assert_eq!((report.tracks, report.fields), (0, 0));
        }
        other => panic!("expected an accept report, got {other:?}"),
    }
}
