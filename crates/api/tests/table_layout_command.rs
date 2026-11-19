#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use resonance_api::Api;
use resonance_api::command::{LibraryTableLayout, Request, Response, TrackPage};
use resonance_api::error::ErrorCode;
use resonance_core::library::{AudioFormat, ScannedFile, TagField};
use resonance_db::Database;
use serde_json::json;

fn library() -> Api {
    let mut database = Database::open_in_memory().unwrap();
    let source = database.source("D:\\Club").unwrap();
    let file = |path: &str, title: &str, number: &str| ScannedFile {
        path: path.to_owned(),
        size_bytes: 1,
        modified_ns: 0,
        format: AudioFormat::Mp3,
        audio: None,
        tags: vec![
            (TagField::Title, title.to_owned()),
            (TagField::TrackNumber, number.to_owned()),
            (TagField::Composer, format!("{title} writer")),
        ],
    };
    database
        .record_scan(
            source,
            &[file("a.mp3", "Opener", "10"), file("b.mp3", "Closer", "2")],
        )
        .unwrap();
    Api::new_silent(database)
}

fn request(value: serde_json::Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn layout(api: &Api, value: serde_json::Value) -> LibraryTableLayout {
    match api.handle(request(value)).unwrap() {
        Response::TableLayout(layout) | Response::ChangeTableLayout(layout) => layout,
        other => panic!("unexpected response {other:?}"),
    }
}

fn ids(layout: &LibraryTableLayout) -> Vec<&str> {
    layout
        .columns
        .iter()
        .map(|column| column.id.as_str())
        .collect()
}

#[test]
fn the_chooser_offers_every_fixed_column_and_the_tags_the_library_has() {
    let api = library();
    let start = layout(&api, json!({ "command": "table_layout" }));
    assert_eq!(ids(&start), ["title", "artist", "bpm", "key", "time"]);
    assert!(!start.wrap);
    let offered: Vec<&str> = start
        .available
        .iter()
        .map(|column| column.id.as_str())
        .collect();
    assert!(offered.contains(&"composer"));
    assert!(offered.contains(&"artwork"));
    assert!(offered.contains(&"tag:track_number"));
    assert!(!offered.contains(&"tag:subgenre"));
    let title = &start.columns[0];
    assert!(!title.removable);
    assert_eq!(title.default_width_px, None);
}

#[test]
fn changes_persist_and_the_sort_reaches_the_track_query() {
    let api = library();
    layout(
        &api,
        json!({ "command": "change_table_layout", "params": { "change": { "kind": "toggle", "column": "tag:track_number" } } }),
    );
    layout(
        &api,
        json!({ "command": "change_table_layout", "params": { "change": { "kind": "resize", "column": "tag:track_number", "width_px": 90 } } }),
    );
    layout(
        &api,
        json!({ "command": "change_table_layout", "params": { "change": { "kind": "set_wrap", "wrap": true } } }),
    );
    let sorted = layout(
        &api,
        json!({ "command": "change_table_layout", "params": { "change": { "kind": "sort", "column": "tag:track_number", "add": false } } }),
    );
    assert_eq!(sorted.drop_order, ["tag:track_number"]);

    let again = layout(&api, json!({ "command": "table_layout" }));
    assert_eq!(again, sorted);
    assert!(again.wrap);
    assert_eq!(again.columns[5].width_px, Some(90));

    let field = serde_json::to_value(again.sort[0].field).unwrap();
    assert_eq!(field, json!({ "tag": "track_number" }));
    let page = match api
        .handle(request(json!({
            "command": "tracks",
            "params": { "offset": 0, "limit": 10, "query": { "sort": field, "direction": "asc" } }
        })))
        .unwrap()
    {
        Response::Tracks(page) => page,
        other => panic!("unexpected response {other:?}"),
    };
    let titles: Vec<&str> = page_titles(&page);
    assert_eq!(titles, ["Closer", "Opener"]);
    assert_eq!(page.tracks[0].composer.as_deref(), Some("Closer writer"));

    let reset = layout(
        &api,
        json!({ "command": "change_table_layout", "params": { "change": { "kind": "reset" } } }),
    );
    assert_eq!(ids(&reset), ["title", "artist", "bpm", "key", "time"]);
    assert!(reset.sort.is_empty());
}

fn page_titles(page: &TrackPage) -> Vec<&str> {
    page.tracks
        .iter()
        .map(|track| track.title.as_str())
        .collect()
}

#[test]
fn an_unknown_column_is_refused() {
    let api = library();
    let error = api
        .handle(request(json!({
            "command": "change_table_layout",
            "params": { "change": { "kind": "toggle", "column": "tension" } }
        })))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BadRequest);
}

/// The table draws its first frame before the core answers, so the default layout is written
/// into the bindings from the same rules the core applies.
#[test]
fn the_default_layout_for_the_ui_is_written_from_core() {
    let layout = resonance_api::default_table_layout();
    let json = serde_json::to_string_pretty(&layout).unwrap();
    let text = format!(
        "// This file is written by `cargo test -p resonance-api` from `TableLayout` in \
         `crates/core`. Do not edit this file manually.\n\n\
         import type {{ LibraryTableLayout }} from './LibraryTableLayout';\n\n\
         export const DEFAULT_TABLE_LAYOUT: LibraryTableLayout = {json};\n"
    );
    let folder = std::path::PathBuf::from(
        std::env::var("TS_RS_EXPORT_DIR").expect("cargo sets TS_RS_EXPORT_DIR for the bindings"),
    );
    let path = folder.join("DefaultTableLayout.ts");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(layout.columns.len(), 5);
}
