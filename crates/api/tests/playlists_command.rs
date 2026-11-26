#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use resonance_api::Api;
use resonance_api::command::{
    ExternalChangePreview, OperationStateName, PlaylistMirror, Request, Response,
};
use resonance_db::Database;
use resonance_fixtures::signal::Groove;
use resonance_library::playlist_update::{
    self, ClearedChange, PlaylistUpdate, RewrittenFile, byte_hash,
};
use serde_json::{Value, json};

fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn changes(api: &Api) -> Vec<ExternalChangePreview> {
    let Response::ExternalChanges(previews) = api.handle(Request::ExternalChanges).unwrap() else {
        panic!("expected the waiting changes");
    };
    previews
}

fn apply(api: &Api, library_id: i64) -> Vec<String> {
    let Response::ApplyExternalChanges(applied) = api
        .handle(Request::ApplyExternalChanges { library_id })
        .unwrap()
    else {
        panic!("expected the applied changes");
    };
    applied.backups
}

#[test]
fn a_mirrored_playlist_follows_a_replace_through_a_backed_up_rewrite_and_exports() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api-playlists");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("music");
    let sets = root.join("sets");
    let incoming = root.join("incoming");
    for folder in [&music, &sets, &incoming] {
        fs::create_dir_all(folder).unwrap();
    }
    let groove = Groove {
        tempo_bpm: 124.0,
        seed: 9,
    };
    let master = incoming.join("master.wav");
    groove
        .render(40.0, 44_100, 0.0, 1.0)
        .write_wav(&master)
        .unwrap();
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .args([text(&master), "-c:a", "libmp3lame", "-b:a", "128k"])
        .arg(text(&music.join("night drive.mp3")))
        .status()
        .expect("FFmpeg is installed for the replace tests");
    assert!(status.success());
    fs::remove_file(&master).unwrap();
    groove
        .render(40.0, 44_100, 0.0, 1.0)
        .write_flac(&incoming.join("night drive.flac"))
        .unwrap();
    groove
        .render(20.0, 44_100, 0.0, 1.0)
        .write_wav(&music.join("other.wav"))
        .unwrap();
    let playlist = sets.join("Friday.m3u8");
    let original = "#EXTM3U\n#EXTINF:40,Night Drive\n../music/night drive.mp3\n\
                    ../music/other.wav\n../music/not here.mp3\nhttp://radio.example/live\n";
    fs::write(&playlist, original).unwrap();

    let api = Api::new_silent(Database::open(&root.join("library.db")).unwrap());
    api.handle(Request::Scan {
        folder: text(&music).to_owned(),
    })
    .unwrap();

    let Response::ImportPlaylist(PlaylistMirror {
        collection,
        matched,
        not_in_library,
        skipped,
    }) = api
        .handle(Request::ImportPlaylist {
            path: format!("\"{}\"", playlist.display()),
        })
        .unwrap()
    else {
        panic!("expected a mirror");
    };
    assert_eq!((matched, not_in_library, skipped), (2, 1, 1));
    assert_eq!(collection.name, "Friday");
    assert!(collection.mirror);
    assert_eq!(collection.track_count, 2);
    let refused = api
        .handle(Request::AddTracksToCollection {
            collection_id: collection.id,
            track_ids: vec![1],
        })
        .unwrap_err();
    assert!(refused.message.contains("follows a playlist file"));

    let Response::FindPlaylists(found) = api.handle(Request::FindPlaylists).unwrap() else {
        panic!("expected found playlists");
    };
    assert!(found.is_empty(), "the playlist sits outside the sources");

    let Response::Tracks(page) = api
        .handle(request(json!({
            "command": "tracks",
            "params": { "offset": 0, "limit": 10, "query": { "collection_id": collection.id } }
        })))
        .unwrap()
    else {
        panic!("expected tracks");
    };
    let night_drive = page
        .tracks
        .iter()
        .find(|track| track.format == "mp3")
        .unwrap()
        .id;

    let Response::ApplyReplacement(replaced) = api
        .handle(request(json!({
            "command": "apply_replacement",
            "params": {
                "track_id": night_drive,
                "path": text(&incoming.join("night drive.flac")),
                "choice": "replace"
            }
        })))
        .unwrap()
    else {
        panic!("expected a replace result");
    };
    assert_eq!(replaced.operation.state, OperationStateName::Applied);
    assert!(music.join("night drive.flac").is_file());

    let waiting = changes(&api);
    assert_eq!(waiting.len(), 1);
    let friday = &waiting[0];
    assert_eq!((friday.changes, friday.not_found), (1, 0));
    assert_eq!(friday.problem, None);
    let path_edit = friday.edits.iter().find(|edit| edit.line == 3).unwrap();
    assert_eq!(path_edit.before, "../music/night drive.mp3");
    assert_eq!(path_edit.after, "../music/night drive.flac");
    assert_eq!(
        fs::read_to_string(&playlist).unwrap(),
        original,
        "the dry run wrote nothing"
    );

    let backups = apply(&api, friday.library_id);
    assert_eq!(backups.len(), 1);
    assert!(backups[0].starts_with(text(&root.join("backups").join("playlists"))));
    assert_eq!(fs::read_to_string(&backups[0]).unwrap(), original);
    let rewritten = fs::read_to_string(&playlist).unwrap();
    assert!(rewritten.contains("\n../music/night drive.flac\n"));
    assert!(rewritten.contains("\n../music/other.wav\n../music/not here.mp3\nhttp://"));
    assert!(changes(&api).is_empty());

    let Response::ReadPlaylistAgain(again) = api
        .handle(Request::ReadPlaylistAgain {
            collection_id: collection.id,
        })
        .unwrap()
    else {
        panic!("expected a mirror");
    };
    assert_eq!((again.matched, again.collection.id), (2, collection.id));

    api.handle(Request::UndoOperation {
        operation_id: replaced.operation.id,
    })
    .unwrap();
    let back = changes(&api);
    assert_eq!(
        back[0].edits.last().unwrap().after,
        "../music/night drive.mp3"
    );
    apply(&api, back[0].library_id);
    assert!(
        fs::read_to_string(&playlist)
            .unwrap()
            .contains("\n../music/night drive.mp3\n")
    );

    let exported = sets.join("Export.m3u8");
    let Response::ExportCollection(written) = api
        .handle(Request::ExportCollection {
            collection_id: collection.id,
            path: text(&exported).to_owned(),
        })
        .unwrap()
    else {
        panic!("expected an export");
    };
    assert_eq!(written.tracks, 2);
    let export = fs::read_to_string(&exported).unwrap();
    assert!(export.starts_with("#EXTM3U\n#EXTINF:40,"));
    assert!(export.contains("night drive.mp3\n"));
    assert!(export.contains("other.wav\n"));
    assert!(
        api.handle(Request::ExportCollection {
            collection_id: collection.id,
            path: text(&exported).to_owned(),
        })
        .unwrap_err()
        .message
        .contains("already exists")
    );

    let other = page
        .tracks
        .iter()
        .find(|track| track.format == "wav")
        .unwrap()
        .id;
    api.handle(request(json!({
        "command": "set_user_tag",
        "params": {
            "track_ids": [other],
            "edits": [{ "field": "artist", "value": "Kofi Bell" }, { "field": "title", "value": "Third Rail" }]
        }
    })))
    .unwrap();
    let Response::ApplyRename(renamed) = api
        .handle(request(json!({
            "command": "apply_rename",
            "params": {
                "scope": { "kind": "tracks", "track_ids": [other] },
                "template": "{artist} - {title}"
            }
        })))
        .unwrap()
    else {
        panic!("expected a rename");
    };
    assert_eq!(renamed.renamed, 1);
    let renamed_edits = &changes(&api)[0].edits;
    assert_eq!(renamed_edits.len(), 1, "the entry has no EXTINF to refresh");
    assert_eq!(renamed_edits[0].line, 4);
    assert_eq!(
        renamed_edits[0].after,
        "../music/Kofi Bell - Third Rail.wav"
    );

    let Response::DeleteCollection(left) = api
        .handle(Request::DeleteCollection {
            collection_id: collection.id,
        })
        .unwrap()
    else {
        panic!("expected collections");
    };
    assert!(left.is_empty());
    assert!(playlist.is_file(), "removing the mirror keeps the playlist");
}

fn newest_operation(api: &Api) -> resonance_api::command::JournalOperation {
    let Response::RecentOperations(operations) =
        api.handle(Request::RecentOperations { limit: 1 }).unwrap()
    else {
        panic!("expected the history");
    };
    operations.into_iter().next().unwrap()
}

#[test]
fn a_playlist_update_is_undone_from_history_and_recovered_after_a_crash() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api-playlist-undo");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("music");
    let sets = root.join("sets");
    fs::create_dir_all(&music).unwrap();
    fs::create_dir_all(&sets).unwrap();
    Groove {
        tempo_bpm: 122.0,
        seed: 4,
    }
    .render(20.0, 44_100, 0.0, 1.0)
    .write_wav(&music.join("first cut.wav"))
    .unwrap();
    let playlist = sets.join("Saturday.m3u8");
    let original = "#EXTM3U\r\n../music/first cut.wav\r\n";
    fs::write(&playlist, original).unwrap();
    let database = root.join("library.db");

    let api = Api::new_silent(Database::open(&database).unwrap());
    api.handle(Request::Scan {
        folder: text(&music).to_owned(),
    })
    .unwrap();
    api.handle(Request::ImportPlaylist {
        path: text(&playlist).to_owned(),
    })
    .unwrap();
    api.handle(request(json!({
        "command": "set_user_tag",
        "params": {
            "track_ids": [1],
            "edits": [{ "field": "artist", "value": "Lena Brook" }, { "field": "title", "value": "Low Sun" }]
        }
    })))
    .unwrap();
    api.handle(request(json!({
        "command": "apply_rename",
        "params": { "scope": { "kind": "tracks", "track_ids": [1] }, "template": "{artist} - {title}" }
    })))
    .unwrap();
    let library_id = changes(&api)[0].library_id;

    let backups = apply(&api, library_id);
    let written = fs::read(&playlist).unwrap();
    assert_eq!(written, b"#EXTM3U\r\n../music/Lena Brook - Low Sun.wav\r\n");
    assert!(changes(&api).is_empty());
    let update = newest_operation(&api);
    assert_eq!(update.summary, "Updated 1 playlist");
    assert_eq!(update.state, OperationStateName::Applied);
    assert!(update.can_undo);

    let Response::UndoOperation(undone) = api
        .handle(Request::UndoOperation {
            operation_id: update.id,
        })
        .unwrap()
    else {
        panic!("expected an undo");
    };
    assert_eq!(undone.operation.state, OperationStateName::Undone);
    assert_eq!(undone.restored, 1);
    assert_eq!(fs::read_to_string(&playlist).unwrap(), original);
    assert!(Path::new(&backups[0]).is_file(), "the backup is kept");
    let queued = changes(&api);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].changes, 1);
    assert_eq!(
        queued[0].edits[0].after,
        "../music/Lena Brook - Low Sun.wav"
    );

    apply(&api, library_id);
    let update = newest_operation(&api);
    let mut changed = written.clone();
    changed.extend_from_slice(b"../music/added elsewhere.wav\r\n");
    fs::write(&playlist, &changed).unwrap();
    let refused = api
        .handle(Request::UndoOperation {
            operation_id: update.id,
        })
        .unwrap_err();
    assert!(refused.message.contains("changed after Resonance wrote it"));
    assert_eq!(fs::read(&playlist).unwrap(), changed);
    assert!(changes(&api).is_empty());
    assert_eq!(newest_operation(&api).state, OperationStateName::Applied);
    fs::write(&playlist, &written).unwrap();
    api.handle(Request::UndoOperation {
        operation_id: update.id,
    })
    .unwrap();
    assert_eq!(fs::read_to_string(&playlist).unwrap(), original);
    drop(api);

    // The app stops after the plan is journaled: once half way through writing the file, once
    // after the new file is in place.
    let crash = |on_disk: &[u8]| {
        let mut library = Database::open(&database).unwrap();
        let cleared = library
            .pending_external_changes()
            .unwrap()
            .into_iter()
            .map(|change| ClearedChange {
                id: change.id,
                library_id: change.library_id,
                track_id: change.track_id,
                old_path: change.old_path,
                new_path: change.new_path,
            })
            .collect();
        let backup = root.join("backups").join("crash.m3u8");
        fs::write(&backup, original).unwrap();
        let update = PlaylistUpdate {
            files: vec![RewrittenFile {
                path: playlist.clone(),
                backup,
                before_hash: byte_hash(original.as_bytes()),
                after_hash: byte_hash(&written),
            }],
            cleared,
        };
        playlist_update::begin(&mut library, &update).unwrap();
        fs::write(&playlist, on_disk).unwrap();
    };

    crash(&written[..20]);
    let api = Api::new_silent(Database::open(&database).unwrap());
    let recovered = newest_operation(&api);
    assert_eq!(recovered.state, OperationStateName::RolledBack);
    assert!(
        recovered
            .recovery
            .unwrap()
            .contains("put back from its backup")
    );
    assert_eq!(fs::read_to_string(&playlist).unwrap(), original);
    assert_eq!(changes(&api).len(), 1, "the change still waits");
    drop(api);

    crash(&written);
    let api = Api::new_silent(Database::open(&database).unwrap());
    let recovered = newest_operation(&api);
    assert_eq!(recovered.state, OperationStateName::Applied);
    assert!(recovered.can_undo);
    assert_eq!(fs::read(&playlist).unwrap(), written);
    assert!(changes(&api).is_empty());
    api.handle(Request::UndoOperation {
        operation_id: recovered.id,
    })
    .unwrap();
    assert_eq!(fs::read_to_string(&playlist).unwrap(), original);
    assert_eq!(changes(&api).len(), 1);
}
