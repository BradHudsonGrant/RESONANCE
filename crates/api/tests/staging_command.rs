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

#[test]
fn prep_queue_and_batch_tools_fix_case_replace_text_and_apply_release_fields() {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("D:\\Club").unwrap();
    let file = |path: &str, tags: Vec<(TagField, String)>| ScannedFile {
        path: path.to_owned(),
        size_bytes: 1,
        modified_ns: 0,
        format: AudioFormat::Wav,
        audio: None,
        tags,
    };
    database
        .record_scan(
            source,
            &[
                file(
                    "[VEC001] Vector Recordings - Horizon EP (2024)/01 - dj kaden - echo chamber (uk vip mix).wav",
                    vec![],
                ),
                file(
                    "[VEC001] Vector Recordings - Horizon EP (2024)/02 - mc conrad - night drive [www.promo.test].wav",
                    vec![],
                ),
            ],
        )
        .unwrap();
    let api = Api::new_silent(database);

    let Response::PrepQueue(snapshot) = api
        .handle(request(json!({
            "command": "prep_queue",
            "params": { "proposals_only": true }
        })))
        .unwrap()
    else {
        panic!("expected a prep queue snapshot");
    };
    assert_eq!(snapshot.items.len(), 2);
    assert!(snapshot.items[0].pending_fields >= 6);
    assert_eq!(
        snapshot.items[0].release.as_ref().map(|r| r.title.as_str()),
        Some("Horizon EP")
    );
    let ids: Vec<i64> = snapshot.items.iter().map(|item| item.track.id).collect();

    let Response::AcceptPrepItems(accepted) = api
        .handle(request(json!({
            "command": "accept_prep_items",
            "params": { "track_ids": [ids[0]], "fields": [] }
        })))
        .unwrap()
    else {
        panic!("expected accepted proposals");
    };
    assert_eq!(accepted.tracks, 1);
    assert!(accepted.fields >= 6);

    let Response::ApplyBatchTag(case_report) = api
        .handle(request(json!({
            "command": "apply_batch_tag",
            "params": {
                "track_ids": [ids[0]],
                "tool": { "kind": "fix_case", "fields": ["title", "artist", "mix"], "text_case": "title" }
            }
        })))
        .unwrap()
    else {
        panic!("expected batch tag report");
    };
    assert_eq!(case_report.tracks, 1);

    let Response::ApplyBatchTag(release_report) = api
        .handle(request(json!({
            "command": "apply_batch_tag",
            "params": {
                "track_ids": [ids[0]],
                "tool": { "kind": "apply_release_fields", "source_track_id": ids[0], "fields": [] }
            }
        })))
        .unwrap()
    else {
        panic!("expected release batch report");
    };
    assert_eq!(release_report.tracks, 1, "copied release fields to track 2");
    assert_eq!(release_report.fields, 4);

    let after_first = diff(&api, ids[0]);
    fn value_of(d: &TrackStagingDiff, name: TagName) -> Option<&str> {
        d.fields
            .iter()
            .find(|f| f.field == name)
            .and_then(|f| f.current_value.as_deref())
    }
    assert_eq!(value_of(&after_first, TagName::Artist), Some("DJ Kaden"));
    assert_eq!(
        value_of(&after_first, TagName::Title),
        Some("Echo Chamber (UK VIP Mix)")
    );
    assert_eq!(value_of(&after_first, TagName::Mix), None);

    let after_second = diff(&api, ids[1]);
    assert_eq!(value_of(&after_second, TagName::Album), Some("Horizon EP"));
    assert_eq!(
        value_of(&after_second, TagName::Label),
        Some("Vector Recordings")
    );
    assert_eq!(value_of(&after_second, TagName::CatNo), Some("VEC001"));
    assert_eq!(value_of(&after_second, TagName::Year), Some("2024"));

    let bad = api
        .handle(Request::AcceptPrepItems {
            track_ids: vec![ids[0], ids[1]],
            fields: vec![],
            candidate_index: Some(0),
            skip_remaining: false,
            include_artwork: None,
        })
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::BadRequest);
}

#[test]
fn accept_prep_items_empty_fields_with_skip_remaining_does_not_accept_pending_tags() {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("D:\\Club").unwrap();
    let track_file = |path: &str, title: &str| ScannedFile {
        path: path.to_owned(),
        size_bytes: 1,
        modified_ns: 0,
        format: AudioFormat::Mp3,
        audio: None,
        tags: vec![(TagField::Title, title.to_owned())],
    };
    database
        .record_scan(
            source,
            &[
                track_file("a.mp3", "Track One"),
                track_file("b.mp3", "Track Two"),
            ],
        )
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
                field: TagField::Label,
                value: "Recordings A".to_owned(),
                provider: "discogs".to_owned(),
                confidence: 0.94,
                audio_matched: false,
            }],
        )
        .unwrap();
    database
        .store_proposals(
            ids[1],
            &[Proposal {
                field: TagField::Label,
                value: "Recordings B".to_owned(),
                provider: "discogs".to_owned(),
                confidence: 0.94,
                audio_matched: false,
            }],
        )
        .unwrap();
    let art = resonance_db::StoredArtwork {
        hash: "a".repeat(64),
        kind: resonance_core::artwork::ImageKind::Jpeg,
        byte_len: 1234,
    };
    database
        .store_proposed_artwork(ids[0], &art, "discogs", "https://i.discogs.com/one.jpg")
        .unwrap();
    database
        .store_proposed_artwork(ids[1], &art, "discogs", "https://i.discogs.com/two.jpg")
        .unwrap();

    let api = Api::new_silent(database);
    let revisions = api.watch_revisions();
    let rev0 = *revisions.borrow();

    // Case 1: cover-only Apply & skip rest (`fields: []`, `skip_remaining: true`, `include_artwork: Some(true)`).
    let Response::AcceptPrepItems(art_only) = api
        .handle(Request::AcceptPrepItems {
            track_ids: vec![ids[0]],
            fields: vec![],
            candidate_index: None,
            skip_remaining: true,
            include_artwork: Some(true),
        })
        .unwrap()
    else {
        panic!("expected AcceptPrepItems");
    };
    assert_eq!(art_only.tracks, 1);
    assert_eq!(art_only.fields, 1);
    assert!(*revisions.borrow() > rev0);
    let diff0 = diff(&api, ids[0]);
    let label0 = diff0
        .fields
        .iter()
        .find(|f| f.field == TagName::Label)
        .unwrap();
    assert_eq!(label0.current_value, None);
    assert_eq!(label0.status, Some(ProposalState::Skipped));
    let Response::TrackArtwork(art0) = api
        .handle(Request::TrackArtwork { track_id: ids[0] })
        .unwrap()
    else {
        panic!("expected TrackArtwork");
    };
    assert_eq!(art0.artwork.as_deref(), Some("a".repeat(64).as_str()));
    assert!(art0.proposed.is_none());

    // Case 2: skip all untouched proposals when skipping remaining with artwork unchecked.
    let rev1 = *revisions.borrow();
    let Response::AcceptPrepItems(none_accepted) = api
        .handle(Request::AcceptPrepItems {
            track_ids: vec![ids[1]],
            fields: vec![],
            candidate_index: None,
            skip_remaining: true,
            include_artwork: Some(false),
        })
        .unwrap()
    else {
        panic!("expected AcceptPrepItems");
    };
    assert_eq!(none_accepted.tracks, 0);
    assert_eq!(none_accepted.fields, 0);
    assert!(*revisions.borrow() > rev1);
    let diff1 = diff(&api, ids[1]);
    let label1 = diff1
        .fields
        .iter()
        .find(|f| f.field == TagName::Label)
        .unwrap();
    assert_eq!(label1.current_value, None);
    assert_eq!(label1.status, Some(ProposalState::Skipped));
    let Response::TrackArtwork(art1) = api
        .handle(Request::TrackArtwork { track_id: ids[1] })
        .unwrap()
    else {
        panic!("expected TrackArtwork");
    };
    assert_eq!(art1.artwork, None);
    assert!(art1.proposed.is_none());
}

#[test]
fn staging_queue_zero_limit_counts_pending_artwork_only_tracks() {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("D:\\Club").unwrap();
    database
        .record_scan(
            source,
            &[ScannedFile {
                path: "cover-only.mp3".to_owned(),
                size_bytes: 1,
                modified_ns: 0,
                format: AudioFormat::Mp3,
                audio: None,
                tags: vec![(TagField::Title, "Cover Only".to_owned())],
            }],
        )
        .unwrap();
    let id = database.tracks().unwrap()[0].track_id;
    let art = resonance_db::StoredArtwork {
        hash: "b".repeat(64),
        kind: resonance_core::artwork::ImageKind::Jpeg,
        byte_len: 512,
    };
    database
        .store_proposed_artwork(id, &art, "discogs", "https://i.discogs.com/cover.jpg")
        .unwrap();

    let api = Api::new_silent(database);
    let Response::StagingQueue(page) = api
        .handle(Request::StagingQueue {
            offset: 0,
            limit: 0,
        })
        .unwrap()
    else {
        panic!("expected StagingQueue");
    };
    assert_eq!(page.total, 1);
    assert!(page.items.is_empty());
}

#[test]
fn skip_prep_items_bumps_watch_revision() {
    let (api, ids) = library();
    let revisions = api.watch_revisions();
    let before = *revisions.borrow();

    let Response::SkipPrepItems(skipped) = api
        .handle(Request::SkipPrepItems {
            track_ids: vec![ids[0]],
            fields: vec![],
        })
        .unwrap()
    else {
        panic!("expected SkipPrepItems");
    };
    assert_eq!(skipped.fields, 1);
    assert!(*revisions.borrow() > before);
}
