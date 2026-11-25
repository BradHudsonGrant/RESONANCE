use std::fs;
use std::path::{Path, PathBuf};

use resonance_core::journal::{OperationKind, OperationState, StepState};
use resonance_core::search::TrackQuery;
use resonance_db::Database;
use resonance_fixtures::silence::write_silent_mp3;
use resonance_library::delete::{DeleteMode, delete_tracks};
use resonance_library::journal::{
    JournalError, Step, apply, begin, byte_hash_hex, recover, run_step, undo,
};
use resonance_library::scan::scan;

struct Library {
    music: PathBuf,
    database_path: PathBuf,
}

/// A library file beside a music folder holding `names`, each a different length so the bin
/// can tell them apart. The folder is named after the test so two runs never share a bin entry.
fn library(test: &str, names: &[&str]) -> Library {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&root);
    let music = root.join("music");
    fs::create_dir_all(&music).expect("test folder created");
    for (index, name) in names.iter().enumerate() {
        write_silent_mp3(&music.join(name), 0.5 + index as f64 * 0.25).expect("file written");
    }
    let database_path = root.join("library.db");
    let mut database = Database::open(&database_path).expect("database");
    scan(&mut database, &music, &[]).expect("scan");
    Library {
        music,
        database_path,
    }
}

impl Library {
    fn open(&self) -> Database {
        Database::open(&self.database_path).expect("database reopens")
    }
}

fn names_on_disk(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .expect("folder")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn shown_paths(database: &Database) -> Vec<String> {
    let mut paths: Vec<String> = database
        .query_asset_ids(&TrackQuery::default())
        .expect("assets")
        .into_iter()
        .map(|asset| {
            database
                .asset_file(asset)
                .expect("asset")
                .expect("known")
                .path
        })
        .collect();
    paths.sort();
    paths
}

fn track_of(database: &Database, path: &str) -> i64 {
    database
        .tracks()
        .expect("tracks")
        .into_iter()
        .find(|row| row.file.path == path)
        .map(|row| row.track_id)
        .expect("a scanned track")
}

fn asset_of(database: &Database, path: &str) -> i64 {
    database
        .query_asset_ids(&TrackQuery::default())
        .expect("assets")
        .into_iter()
        .find(|asset| {
            database
                .asset_file(*asset)
                .expect("asset")
                .expect("known")
                .path
                == path
        })
        .expect("a scanned file")
}

#[test]
fn deleting_to_the_bin_and_undoing_puts_files_and_tracks_back() {
    let library = library("journal-bin-undo", &["keep.mp3", "one.mp3", "two.mp3"]);
    let mut database = library.open();
    let one = track_of(&database, "one.mp3");
    let two = track_of(&database, "two.mp3");
    let before_one = byte_hash_hex(&library.music.join("one.mp3")).expect("hash");
    let before_two = byte_hash_hex(&library.music.join("two.mp3")).expect("hash");
    let crate_ = database.create_collection("Warm-up").expect("collection");
    database
        .add_tracks_to_collection(crate_.id, &[one, two])
        .expect("added");
    database.set_track_favourite(one, true).expect("favourite");

    let operation = delete_tracks(&mut database, &[one, two], DeleteMode::ToBin).expect("deleted");
    assert_eq!(operation.kind, OperationKind::DeleteFiles);
    assert_eq!(operation.state, OperationState::Applied);
    assert_eq!(operation.summary, "Moved 2 files to the bin");
    assert_eq!(names_on_disk(&library.music), ["keep.mp3"]);
    assert_eq!(shown_paths(&database), ["keep.mp3"]);
    assert_eq!(
        database.collections().expect("collections")[0].track_count,
        0
    );

    // A rescan while the files are in the bin neither brings them back nor marks them missing.
    scan(&mut database, &library.music, &[]).expect("rescan");
    assert_eq!(shown_paths(&database), ["keep.mp3"]);

    let undone = undo(&mut database, operation.id).expect("undone");
    assert_eq!(undone.state, OperationState::Undone);
    assert_eq!(
        names_on_disk(&library.music),
        ["keep.mp3", "one.mp3", "two.mp3"]
    );
    assert_eq!(
        byte_hash_hex(&library.music.join("one.mp3")).expect("hash"),
        before_one
    );
    assert_eq!(
        byte_hash_hex(&library.music.join("two.mp3")).expect("hash"),
        before_two
    );
    assert_eq!(shown_paths(&database), ["keep.mp3", "one.mp3", "two.mp3"]);
    assert_eq!(track_of(&database, "one.mp3"), one);
    assert_eq!(
        database.collections().expect("collections")[0].track_count,
        2
    );
    let favourite = database
        .tracks()
        .expect("tracks")
        .into_iter()
        .find(|row| row.track_id == one)
        .expect("track")
        .favourite;
    assert!(favourite);
    assert!(matches!(
        undo(&mut database, operation.id),
        Err(JournalError::CannotUndo { .. })
    ));
}

#[test]
fn a_file_that_cannot_go_puts_back_the_ones_already_in_the_bin() {
    let library = library("journal-bin-rollback", &["one.mp3", "two.mp3"]);
    let mut database = library.open();
    let one = library.music.join("one.mp3");
    let two = library.music.join("two.mp3");
    let steps = [
        Step::HideAsset {
            asset_id: asset_of(&database, "one.mp3"),
        },
        Step::TrashFile {
            size_bytes: fs::metadata(&one).expect("file").len(),
            path: one,
        },
        Step::HideAsset {
            asset_id: asset_of(&database, "two.mp3"),
        },
        // The size recorded in the plan no longer matches, as if the file changed.
        Step::TrashFile {
            size_bytes: 1,
            path: two,
        },
    ];

    let error = apply(
        &mut database,
        OperationKind::DeleteFiles,
        "Moved 2 files to the bin",
        "{}",
        &steps,
    )
    .expect_err("the second file is refused");
    let JournalError::RolledBack {
        operation_id,
        cause,
    } = error
    else {
        panic!("expected a roll back, got {error}");
    };
    assert!(matches!(*cause, JournalError::ChangedOnDisk(_)));
    let operation = database
        .operation(operation_id)
        .expect("read")
        .expect("known");
    assert_eq!(operation.state, OperationState::RolledBack);
    assert_eq!(names_on_disk(&library.music), ["one.mp3", "two.mp3"]);
    assert_eq!(shown_paths(&database), ["one.mp3", "two.mp3"]);
}

#[test]
fn deleting_for_good_removes_the_file_and_cannot_be_undone() {
    let library = library("journal-for-good", &["keep.mp3", "gone.mp3"]);
    let mut database = library.open();
    let gone = track_of(&database, "gone.mp3");

    let operation = delete_tracks(&mut database, &[gone], DeleteMode::ForGood).expect("deleted");
    assert_eq!(operation.kind, OperationKind::DeleteFilesForGood);
    assert!(!operation.kind.can_undo(operation.state));
    assert_eq!(names_on_disk(&library.music), ["keep.mp3"]);
    assert_eq!(shown_paths(&database), ["keep.mp3"]);
    assert!(matches!(
        undo(&mut database, operation.id),
        Err(JournalError::DeletedForGood(_))
    ));
}

/// Where a step boundary falls when the app stops.
#[derive(Debug, Clone, Copy)]
enum Crash {
    /// Half of the staging copy was written.
    DuringStagingCopy,
    AfterStagingCopy,
    AfterMoveIntoPlace,
    AfterDatabaseUpdate,
    /// The trash step was journaled as started but the file had not gone.
    BeforeTrash,
    /// The file went to the bin but the journal never heard.
    AfterTrashUnrecorded,
}

struct ReplaceShape {
    library: Library,
    old: PathBuf,
    incoming: PathBuf,
    staged: PathBuf,
    placed: PathBuf,
    old_hash: String,
    operation_id: i64,
}

/// The file side of replacing a track's file with a better one: copy the new file beside the
/// old one, move it to its final name, take the old file out of the library and move it to the
/// bin. The app stops at `crash`.
fn replace_until(test: &str, crash: Crash) -> ReplaceShape {
    let library = library(test, &["old.mp3"]);
    let incoming_folder = library.music.parent().expect("root").join("incoming");
    fs::create_dir_all(&incoming_folder).expect("incoming folder");
    let incoming = incoming_folder.join("better.mp3");
    write_silent_mp3(&incoming, 2.0).expect("new file");
    let old = library.music.join("old.mp3");
    let staged = library.music.join("old.mp3.resonance-incoming");
    let placed = library.music.join("old (better).mp3");
    let old_hash = byte_hash_hex(&old).expect("hash");

    let mut database = library.open();
    let steps = [
        Step::CopyFile {
            from: incoming.clone(),
            to: staged.clone(),
            byte_hash: byte_hash_hex(&incoming).expect("hash"),
        },
        Step::MoveFile {
            from: staged.clone(),
            to: placed.clone(),
        },
        Step::HideAsset {
            asset_id: asset_of(&database, "old.mp3"),
        },
        Step::TrashFile {
            path: old.clone(),
            size_bytes: fs::metadata(&old).expect("file").len(),
        },
    ];
    let operation_id = begin(
        &mut database,
        OperationKind::DeleteFiles,
        "Replaced 1 file",
        "{}",
        &steps,
    )
    .expect("journaled");
    let done = match crash {
        Crash::DuringStagingCopy => 0,
        Crash::AfterStagingCopy => 1,
        Crash::AfterMoveIntoPlace => 2,
        Crash::AfterDatabaseUpdate | Crash::BeforeTrash | Crash::AfterTrashUnrecorded => 3,
    };
    for position in 0..done {
        run_step(&mut database, operation_id, position).expect("step runs");
    }
    match crash {
        Crash::DuringStagingCopy => {
            database.start_step(operation_id, 0).expect("started");
            let bytes = fs::read(&incoming).expect("read");
            fs::write(&staged, &bytes[..bytes.len() / 2]).expect("half a copy");
        }
        Crash::BeforeTrash => database.start_step(operation_id, 3).expect("started"),
        Crash::AfterTrashUnrecorded => {
            database.start_step(operation_id, 3).expect("started");
            trash::delete(&old).expect("moved to the bin");
        }
        _ => {}
    }
    drop(database);
    ReplaceShape {
        library,
        old,
        incoming,
        staged,
        placed,
        old_hash,
        operation_id,
    }
}

fn recovers_cleanly(test: &str, crash: Crash) {
    let shape = replace_until(test, crash);
    let mut database = shape.library.open();
    let recovered = recover(&mut database).expect("recovered");
    assert_eq!(recovered.len(), 1, "{crash:?}");
    assert_eq!(recovered[0].state, OperationState::RolledBack, "{crash:?}");

    let operation = database
        .operation(shape.operation_id)
        .expect("read")
        .expect("known");
    assert_eq!(operation.state, OperationState::RolledBack, "{crash:?}");
    assert!(operation.recovery.is_some() && operation.recovered_ns.is_some());
    assert_eq!(operation.current_step, None);
    assert!(
        database
            .operation_steps(shape.operation_id)
            .expect("steps")
            .iter()
            .all(|step| step.state != StepState::Done),
        "{crash:?}"
    );
    assert_eq!(
        names_on_disk(&shape.library.music),
        ["old.mp3"],
        "{crash:?}"
    );
    assert!(!shape.staged.exists() && !shape.placed.exists());
    assert!(shape.incoming.exists());
    assert_eq!(byte_hash_hex(&shape.old).expect("hash"), shape.old_hash);
    assert_eq!(shown_paths(&database), ["old.mp3"], "{crash:?}");
    assert!(recover(&mut database).expect("nothing left").is_empty());
}

#[test]
fn a_crash_during_the_staging_copy_recovers_cleanly() {
    recovers_cleanly("journal-crash-during-copy", Crash::DuringStagingCopy);
}

#[test]
fn a_crash_after_the_staging_copy_recovers_cleanly() {
    recovers_cleanly("journal-crash-after-copy", Crash::AfterStagingCopy);
}

#[test]
fn a_crash_after_the_move_into_place_recovers_cleanly() {
    recovers_cleanly("journal-crash-after-move", Crash::AfterMoveIntoPlace);
}

#[test]
fn a_crash_after_the_database_update_recovers_cleanly() {
    recovers_cleanly("journal-crash-after-database", Crash::AfterDatabaseUpdate);
}

#[test]
fn a_crash_before_the_trash_recovers_cleanly() {
    recovers_cleanly("journal-crash-before-trash", Crash::BeforeTrash);
}

#[test]
fn a_crash_after_the_last_step_finishes_the_operation_and_undo_still_works() {
    let shape = replace_until("journal-crash-after-trash", Crash::AfterTrashUnrecorded);
    let mut database = shape.library.open();
    let recovered = recover(&mut database).expect("recovered");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].state, OperationState::Applied);
    assert_eq!(names_on_disk(&shape.library.music), ["old (better).mp3"]);
    assert!(shown_paths(&database).is_empty());

    undo(&mut database, shape.operation_id).expect("undone");
    assert_eq!(names_on_disk(&shape.library.music), ["old.mp3"]);
    assert_eq!(byte_hash_hex(&shape.old).expect("hash"), shape.old_hash);
    assert_eq!(shown_paths(&database), ["old.mp3"]);
}

#[test]
fn a_tag_write_that_stopped_part_way_loses_its_working_copy() {
    let library = library("journal-crash-tag-write", &["song.mp3"]);
    let mut database = library.open();
    let asset_id = asset_of(&database, "song.mp3");
    let track_id = track_of(&database, "song.mp3");
    let forward = serde_json::json!([{
        "track_id": track_id,
        "asset_id": asset_id,
        "path": "song.mp3",
        "fields": [["title", "Night"]],
    }])
    .to_string();
    let id = database
        .begin_operation(
            OperationKind::WriteTags,
            "Writing tags to 1 file",
            &forward,
            r#"{"files":[]}"#,
        )
        .expect("journaled");
    let copy = library.music.join("song.mp3.resonance-tmp");
    fs::copy(library.music.join("song.mp3"), &copy).expect("working copy");
    drop(database);

    let mut database = library.open();
    let recovered = recover(&mut database).expect("recovered");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].state, OperationState::Failed);
    assert_eq!(names_on_disk(&library.music), ["song.mp3"]);
    assert_eq!(
        database.operation(id).expect("read").expect("known").state,
        OperationState::Failed
    );
}

#[test]
fn a_rename_the_journal_never_recorded_gets_its_old_name_back() {
    let library = library("journal-crash-rename", &["a.mp3"]);
    let mut database = library.open();
    let asset_id = asset_of(&database, "a.mp3");
    let from = library.music.join("a.mp3");
    let to = library.music.join("Artist - Title.mp3");
    let forward = serde_json::json!({
        "template": "{artist} - {title}",
        "files": 1,
        "plan": [{ "asset_id": asset_id, "from": from, "to": to }],
    })
    .to_string();
    database
        .begin_operation(
            OperationKind::RenameFiles,
            "Renaming 1 file",
            &forward,
            "{}",
        )
        .expect("journaled");
    fs::rename(&from, &to).expect("renamed on disk only");
    drop(database);

    let mut database = library.open();
    let recovered = recover(&mut database).expect("recovered");
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].state, OperationState::Failed);
    assert_eq!(names_on_disk(&library.music), ["a.mp3"]);
    assert_eq!(shown_paths(&database), ["a.mp3"]);
}
