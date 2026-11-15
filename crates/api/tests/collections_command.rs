#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_api::Api;
use resonance_api::command::{
    LibraryCollection, Request, Response, TrackCollection, TrackPage, TrackQuery,
};
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
        .join("api-collections")
        .join(name)
        .join(unique);
    fs::create_dir_all(&folder).unwrap();
    folder
}

fn tone(path: &Path, freq: f64) {
    Signal::tone(freq, 0.1, -6.0, 48_000)
        .write_wav(path)
        .unwrap();
}

fn query_tracks(api: &Api, collection_id: Option<i64>) -> TrackPage {
    match api
        .handle(Request::Tracks {
            offset: 0,
            limit: 50,
            query: Some(TrackQuery {
                collection_id,
                ..TrackQuery::default()
            }),
        })
        .unwrap()
    {
        Response::Tracks(page) => page,
        other => panic!("expected tracks, got {other:?}"),
    }
}

fn create(api: &Api, name: &str) -> LibraryCollection {
    match api
        .handle(Request::CreateCollection {
            name: name.to_owned(),
        })
        .unwrap()
    {
        Response::CreateCollection(col) => col,
        other => panic!("expected CreateCollection, got {other:?}"),
    }
}

#[test]
fn api_collection_crud_filtering_and_validation() {
    let music = folder("crud");
    tone(&music.join("Opener.wav"), 440.0);
    tone(&music.join("Closer.wav"), 550.0);
    let api = Api::new(Database::open_in_memory().unwrap());

    api.handle(Request::Scan {
        folder: music.to_str().unwrap().to_owned(),
    })
    .unwrap();

    let all = query_tracks(&api, None);
    assert_eq!(all.total, 2);

    let blank_err = api
        .handle(Request::CreateCollection {
            name: "  ".to_owned(),
        })
        .unwrap_err();
    assert_eq!(blank_err.code, ErrorCode::BadRequest);

    let warm = create(&api, "Warm Up");
    let dup_err = api
        .handle(Request::CreateCollection {
            name: "warm up".to_owned(),
        })
        .unwrap_err();
    assert_eq!(dup_err.code, ErrorCode::BadRequest);

    let added = match api
        .handle(Request::AddTracksToCollection {
            collection_id: warm.id,
            track_ids: vec![all.tracks[1].id, all.tracks[0].id],
        })
        .unwrap()
    {
        Response::AddTracksToCollection(col) => col,
        other => panic!("expected AddTracksToCollection, got {other:?}"),
    };
    assert_eq!(added.track_count, 2);

    let renamed = match api
        .handle(Request::RenameCollection {
            collection_id: warm.id,
            name: "Late Night".to_owned(),
        })
        .unwrap()
    {
        Response::RenameCollection(col) => col,
        other => panic!("expected RenameCollection, got {other:?}"),
    };
    assert_eq!(renamed.name, "Late Night");

    let in_collection = query_tracks(&api, Some(warm.id));
    assert_eq!(in_collection.total, 2);
    assert_eq!(in_collection.tracks[0].title, "Opener");
    assert_eq!(
        in_collection.tracks[0].collections,
        vec![TrackCollection {
            id: warm.id,
            name: "Late Night".to_owned(),
        }]
    );

    let removed = match api
        .handle(Request::RemoveTracksFromCollection {
            collection_id: warm.id,
            track_ids: vec![all.tracks[1].id],
        })
        .unwrap()
    {
        Response::RemoveTracksFromCollection(col) => col,
        other => panic!("expected RemoveTracksFromCollection, got {other:?}"),
    };
    assert_eq!(removed.track_count, 1);

    let after_delete = match api
        .handle(Request::DeleteCollection {
            collection_id: warm.id,
        })
        .unwrap()
    {
        Response::DeleteCollection(list) => list,
        other => panic!("expected DeleteCollection, got {other:?}"),
    };
    assert!(after_delete.is_empty());
    assert_eq!(query_tracks(&api, None).total, 2);
    assert!(music.join("Opener.wav").exists());
}

#[test]
fn external_file_rename_through_api_scan_and_identify_preserves_collection() {
    let music = folder("external-rename");
    let before = music.join("Before Name.wav");
    let after = music.join("After Name.wav");
    tone(&before, 330.0);

    let api = Api::new(Database::open_in_memory().unwrap());
    api.handle(Request::Scan {
        folder: music.to_str().unwrap().to_owned(),
    })
    .unwrap();
    api.handle(Request::Identify).unwrap();

    let track = query_tracks(&api, None).tracks.pop().unwrap();
    let col = create(&api, "Renamed Test");
    api.handle(Request::AddTracksToCollection {
        collection_id: col.id,
        track_ids: vec![track.id],
    })
    .unwrap();

    fs::rename(&before, &after).unwrap();
    api.handle(Request::Scan {
        folder: music.to_str().unwrap().to_owned(),
    })
    .unwrap();
    api.handle(Request::Identify).unwrap();

    let collections = match api.handle(Request::Collections).unwrap() {
        Response::Collections(list) => list,
        other => panic!("expected Collections, got {other:?}"),
    };
    assert_eq!(collections[0].track_count, 1);

    let in_collection = query_tracks(&api, Some(col.id));
    assert_eq!(in_collection.total, 1);
    assert_eq!(in_collection.tracks[0].id, track.id);
    assert_eq!(in_collection.tracks[0].title, "After Name");
    assert_eq!(
        in_collection.tracks[0].collections,
        vec![TrackCollection {
            id: col.id,
            name: "Renamed Test".to_owned(),
        }]
    );
}
