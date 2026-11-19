#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use resonance_api::Api;
use resonance_api::command::{
    DescribedFilter, FilterFieldKind, QuickFilters, Request, Response, SmartView, TrackPage,
};
use resonance_api::error::ErrorCode;
use resonance_core::library::{AudioFormat, ScannedFile, TagField};
use resonance_db::Database;
use serde_json::{Value, json};

fn library() -> Api {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("D:/Club").unwrap();
    let file = |path: &str, title: &str, genre: Option<&str>| ScannedFile {
        path: path.to_owned(),
        size_bytes: 1,
        modified_ns: 0,
        format: AudioFormat::Mp3,
        audio: None,
        tags: [(TagField::Title, Some(title)), (TagField::Genre, genre)]
            .into_iter()
            .filter_map(|(field, value)| value.map(|value| (field, value.to_owned())))
            .collect(),
    };
    database
        .record_scan(
            source,
            &[
                file("a.mp3", "Opener", Some("House")),
                file("b.mp3", "Closer", None),
                file("c.mp3", "Peak", Some("Techno")),
            ],
        )
        .unwrap();
    Api::new_silent(database)
}

fn call(api: &Api, value: Value) -> Response {
    api.handle(serde_json::from_value(value).unwrap()).unwrap()
}

fn parse(api: &Api, text: &str) -> DescribedFilter {
    match call(
        api,
        json!({ "command": "parse_filter", "params": { "text": text } }),
    ) {
        Response::ParseFilter(parsed) => parsed,
        other => panic!("unexpected {other:?}"),
    }
}

fn page(api: &Api, filters: Value) -> TrackPage {
    match call(
        api,
        json!({ "command": "tracks", "params": { "offset": 0, "limit": 50, "query": { "filters": filters } } }),
    ) {
        Response::Tracks(page) => page,
        other => panic!("unexpected {other:?}"),
    }
}

fn bar(response: Response) -> QuickFilters {
    match response {
        Response::QuickFilters(bar)
        | Response::ChangeQuickFilters(bar)
        | Response::RecordFilterUse(bar) => bar,
        other => panic!("unexpected {other:?}"),
    }
}

fn views(response: Response) -> Vec<SmartView> {
    match response {
        Response::SmartViews(views)
        | Response::SaveSmartView(views)
        | Response::RenameSmartView(views)
        | Response::DeleteSmartView(views) => views,
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_command_bar_filter_reads_into_a_term_the_tracks_call_takes() {
    let api = library();
    let missing = parse(&api, "/missing genre");
    assert_eq!(missing.label, "No genre");
    assert_eq!(missing.key, "genre:missing");

    let shown = page(&api, json!([missing.term]));
    assert_eq!(shown.total, 1);
    assert_eq!(shown.library_total, 3);
    assert_eq!(shown.tracks[0].title, "Closer");

    let hidden = page(
        &api,
        json!([{ "any_of": [{ "column": "genre", "condition": { "op": "is_not", "value": "techno" } }] }]),
    );
    assert_eq!(hidden.total, 2, "a track without a genre is not hidden");
}

#[test]
fn a_filter_that_does_not_suit_its_column_is_refused() {
    let api = library();
    let error = api
        .handle(
            serde_json::from_value(json!({
                "command": "tracks",
                "params": { "offset": 0, "limit": 50, "query": { "filters": [
                    { "any_of": [{ "column": "format", "condition": { "op": "missing" } }] }
                ] } }
            }))
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BadRequest);

    let error = api
        .handle(Request::ParseFilter {
            text: "/build set".to_owned(),
        })
        .unwrap_err();
    assert_eq!(error.message, "That is not a filter.");
}

#[test]
fn the_builder_lists_every_column_with_its_conditions() {
    let api = library();
    let Response::FilterFields(fields) = call(&api, json!({ "command": "filter_fields" })) else {
        panic!("expected fields");
    };
    let field = |id: &str| fields.iter().find(|field| field.column == id).unwrap();
    assert_eq!(field("artwork").kind, FilterFieldKind::Picture);
    assert_eq!(field("format").choices.len(), 5);
    assert_eq!(field("key").choices.len(), 24);
    assert_eq!(field("tag:album_artist").label, "Album artist");
    assert_eq!(field("catno").noun, "catalogue number");
}

#[test]
fn summaries_count_each_term_only_when_there_are_several() {
    let api = library();
    let missing = parse(&api, "missing genre").term;
    let opener = parse(&api, "title is opener").term;
    let Response::FilterSummaries(one) = call(
        &api,
        json!({ "command": "filter_summaries", "params": { "query": { "filters": [missing] } } }),
    ) else {
        panic!("expected summaries");
    };
    assert_eq!(one[0].count, None);
    let Response::FilterSummaries(two) = call(
        &api,
        json!({ "command": "filter_summaries", "params": { "query": { "filters": [missing, opener] } } }),
    ) else {
        panic!("expected summaries");
    };
    assert_eq!(
        two.iter().map(|summary| summary.count).collect::<Vec<_>>(),
        [Some(1), Some(1)]
    );
    assert_eq!(two[1].label, "Title is \u{201c}opener\u{201d}");
}

#[test]
fn use_pins_moves_and_the_setting_shape_the_bar() {
    let api = library();
    let start = bar(call(&api, json!({ "command": "quick_filters" })));
    assert!(start.recommend);
    assert!(start.pinned.is_empty());
    assert_eq!(start.recommended[0].label, "No genre");

    let techno = parse(&api, "genre is techno").term;
    let used = bar(call(
        &api,
        json!({ "command": "record_filter_use", "params": { "term": techno } }),
    ));
    assert_eq!(used.recommended[0].label, "Genre is \u{201c}techno\u{201d}");
    assert!(used.recommended.len() <= 5);

    let pinned = bar(call(
        &api,
        json!({ "command": "change_quick_filters", "params": { "change": { "kind": "move", "term": techno, "to": 0 } } }),
    ));
    assert_eq!(pinned.pinned[0].key, "genre:is:techno");
    assert!(
        pinned
            .recommended
            .iter()
            .all(|shown| shown.key != "genre:is:techno")
    );

    let unpinned = bar(call(
        &api,
        json!({ "command": "change_quick_filters", "params": { "change": { "kind": "remove", "term": techno } } }),
    ));
    assert!(unpinned.pinned.is_empty());

    let off = bar(call(
        &api,
        json!({ "command": "change_quick_filters", "params": { "change": { "kind": "set_recommend", "on": false } } }),
    ));
    assert!(!off.recommend);
    assert!(off.recommended.is_empty());
    assert!(!bar(call(&api, json!({ "command": "quick_filters" }))).recommend);
}

#[test]
fn smart_views_keep_a_query_and_reject_a_repeated_name() {
    let api = library();
    let missing = parse(&api, "missing genre").term;
    let saved = views(call(
        &api,
        json!({ "command": "save_smart_view", "params": { "name": "To tag", "query": { "text": "closer", "pill": "favourites", "filters": [missing] } } }),
    ));
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].query.text.as_deref(), Some("closer"));
    assert_eq!(saved[0].query.filters.as_ref().unwrap().len(), 1);

    let error = api
        .handle(
            serde_json::from_value(json!({ "command": "save_smart_view", "params": { "name": "to tag", "query": {} } }))
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        error.message,
        "A smart view named \"to tag\" already exists."
    );

    let id = saved[0].id;
    let renamed = views(call(
        &api,
        json!({ "command": "rename_smart_view", "params": { "view_id": id, "name": "Needs genre" } }),
    ));
    assert_eq!(renamed[0].name, "Needs genre");
    let gone = views(call(
        &api,
        json!({ "command": "delete_smart_view", "params": { "view_id": id } }),
    ));
    assert!(gone.is_empty());
}
