use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use resonance_core::journal::OperationState;
use resonance_core::library::TagField;
use resonance_core::search::TrackQuery;
use resonance_core::template::{RenameStatus, Template};
use resonance_db::Database;
use resonance_fixtures::silence::write_silent_mp3;
use resonance_library::rename::{RenameError, apply_renames, preview_renames, undo_renames};
use resonance_library::scan::scan;

const TEMPLATE: &str = "{artist} - {title} ({mix})";

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(folder.join("music")).expect("test folder created");
    folder.join("music")
}

/// Silent files named `names`, scanned, with the user's tags set from `tags` in the same order.
fn library(folder: &Path, files: &[(&str, &[(TagField, &str)])]) -> Mutex<Database> {
    for (name, _) in files {
        write_silent_mp3(&folder.join(name), 0.5).expect("file written");
    }
    let mut database = Database::open_in_memory().expect("database");
    scan(&mut database, folder, &[]).expect("scan");
    for row in database.tracks().expect("tracks") {
        let tags = files
            .iter()
            .find(|(name, _)| *name == row.file.path)
            .map(|(_, tags)| *tags)
            .expect("a scanned file");
        let edits: Vec<_> = tags
            .iter()
            .map(|(field, value)| (*field, Some((*value).to_owned())))
            .collect();
        database
            .set_user_tags(&[row.track_id], &edits)
            .expect("tags set");
    }
    Mutex::new(database)
}

fn all_assets(database: &Mutex<Database>) -> Vec<i64> {
    database
        .lock()
        .expect("lock")
        .query_asset_ids(&TrackQuery::default())
        .expect("assets")
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

fn library_paths(database: &Mutex<Database>) -> Vec<String> {
    let mut paths: Vec<String> = database
        .lock()
        .expect("lock")
        .tracks()
        .expect("tracks")
        .into_iter()
        .map(|row| row.file.path)
        .collect();
    paths.sort();
    paths
}

fn never_stop(_: u64, _: u64) -> bool {
    false
}

const ARTIST: (TagField, &str) = (TagField::Artist, "Artist");

#[test]
fn renames_from_tags_and_undoes() {
    let folder = test_folder("rename_and_undo");
    let database = library(
        &folder,
        &[
            ("01 one.mp3", &[ARTIST, (TagField::Title, "One")]),
            (
                "02 two.mp3",
                &[ARTIST, (TagField::Title, "Two"), (TagField::Mix, "Dub")],
            ),
        ],
    );
    let template = Template::parse(TEMPLATE).expect("template");
    let assets = all_assets(&database);

    let preview =
        preview_renames(&database.lock().expect("lock"), &assets, &template).expect("preview");
    let mut names: Vec<_> = preview
        .iter()
        .map(|plan| (plan.status, plan.new_name.clone().unwrap_or_default()))
        .collect();
    names.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(
        names,
        vec![
            (RenameStatus::Ready, "Artist - One.mp3".to_owned()),
            (RenameStatus::Ready, "Artist - Two (Dub).mp3".to_owned()),
        ]
    );
    assert_eq!(names_on_disk(&folder), ["01 one.mp3", "02 two.mp3"]);

    let report =
        apply_renames(&database, &assets, &template, TEMPLATE, never_stop).expect("renamed");
    assert_eq!(report.renamed, 2);
    assert!(report.failed.is_empty());
    let expected = ["Artist - One.mp3", "Artist - Two (Dub).mp3"];
    assert_eq!(names_on_disk(&folder), expected);
    assert_eq!(library_paths(&database), expected);

    let again =
        preview_renames(&database.lock().expect("lock"), &assets, &template).expect("preview");
    assert!(
        again
            .iter()
            .all(|plan| plan.status == RenameStatus::Unchanged)
    );

    let undone = undo_renames(&database, report.operation_id.expect("operation")).expect("undone");
    assert_eq!(undone.restored, 2);
    assert_eq!(undone.operation.state, OperationState::Undone);
    assert_eq!(names_on_disk(&folder), ["01 one.mp3", "02 two.mp3"]);
    assert_eq!(library_paths(&database), ["01 one.mp3", "02 two.mp3"]);
}

#[test]
fn a_case_only_rename_changes_the_case_and_undoes() {
    let folder = test_folder("rename_case_only");
    let database = library(
        &folder,
        &[("artist - one.mp3", &[ARTIST, (TagField::Title, "One")])],
    );
    let template = Template::parse(TEMPLATE).expect("template");
    let assets = all_assets(&database);

    let report =
        apply_renames(&database, &assets, &template, TEMPLATE, never_stop).expect("renamed");
    assert_eq!(report.renamed, 1, "{:?}", report.failed);
    assert_eq!(names_on_disk(&folder), ["Artist - One.mp3"]);
    assert_eq!(library_paths(&database), ["Artist - One.mp3"]);

    undo_renames(&database, report.operation_id.expect("operation")).expect("undone");
    assert_eq!(names_on_disk(&folder), ["artist - one.mp3"]);
    assert_eq!(library_paths(&database), ["artist - one.mp3"]);
}

#[test]
fn a_collision_is_blocked_and_nothing_is_overwritten() {
    let folder = test_folder("rename_collision");
    let database = library(
        &folder,
        &[
            ("a.mp3", &[ARTIST, (TagField::Title, "Same")]),
            ("b.mp3", &[ARTIST, (TagField::Title, "Same")]),
            ("c.mp3", &[ARTIST, (TagField::Title, "Taken")]),
        ],
    );
    fs::write(folder.join("ARTIST - TAKEN.mp3"), b"someone else's file").expect("written");
    let template = Template::parse("{artist} - {title}").expect("template");
    let assets = all_assets(&database);

    let preview =
        preview_renames(&database.lock().expect("lock"), &assets, &template).expect("preview");
    let status = |name: &str| {
        preview
            .iter()
            .find(|plan| plan.old_name == name)
            .map(|plan| plan.status)
    };
    assert_eq!(status("a.mp3"), Some(RenameStatus::Ready));
    assert_eq!(status("b.mp3"), Some(RenameStatus::Collision));
    assert_eq!(status("c.mp3"), Some(RenameStatus::Collision));

    let report = apply_renames(
        &database,
        &assets,
        &template,
        "{artist} - {title}",
        never_stop,
    )
    .expect("renamed");
    assert_eq!(report.renamed, 1);
    assert_eq!(
        names_on_disk(&folder),
        ["ARTIST - TAKEN.mp3", "Artist - Same.mp3", "b.mp3", "c.mp3"]
    );
    assert_eq!(
        fs::read(folder.join("ARTIST - TAKEN.mp3")).expect("read"),
        b"someone else's file"
    );
}

#[test]
fn missing_tags_and_reserved_names_are_handled() {
    let folder = test_folder("rename_missing_reserved");
    let database = library(
        &folder,
        &[
            ("untagged.mp3", &[(TagField::Title, "Only a title")]),
            ("device.mp3", &[(TagField::Title, "con")]),
            ("dots.mp3", &[(TagField::Title, "Ends with dots. ..")]),
        ],
    );
    let assets = all_assets(&database);

    let both = Template::parse("{artist} - {title}").expect("template");
    let preview =
        preview_renames(&database.lock().expect("lock"), &assets, &both).expect("preview");
    assert!(preview.iter().all(
        |plan| plan.status == RenameStatus::MissingFields && plan.missing == [TagField::Artist]
    ));

    let title = Template::parse("{title}").expect("template");
    let report = apply_renames(&database, &assets, &title, "{title}", never_stop).expect("renamed");
    assert_eq!(report.renamed, 3);
    assert_eq!(
        names_on_disk(&folder),
        ["Ends with dots.mp3", "Only a title.mp3", "_con.mp3"]
    );
}

#[test]
fn a_rename_moved_again_outside_the_app_is_not_undone() {
    let folder = test_folder("rename_changed_since");
    let database = library(&folder, &[("x.mp3", &[ARTIST, (TagField::Title, "One")])]);
    let template = Template::parse(TEMPLATE).expect("template");
    let report = apply_renames(
        &database,
        &all_assets(&database),
        &template,
        TEMPLATE,
        never_stop,
    )
    .expect("renamed");
    fs::rename(folder.join("Artist - One.mp3"), folder.join("moved.mp3")).expect("moved");

    let refused = undo_renames(&database, report.operation_id.expect("operation"));
    assert!(matches!(refused, Err(RenameError::ChangedSinceRename(_))));
    assert_eq!(names_on_disk(&folder), ["moved.mp3"]);
}

#[test]
fn a_stopped_batch_is_one_operation_that_undoes() {
    let folder = test_folder("rename_stopped");
    let database = library(
        &folder,
        &[
            ("a.mp3", &[ARTIST, (TagField::Title, "A")]),
            ("b.mp3", &[ARTIST, (TagField::Title, "B")]),
            ("c.mp3", &[ARTIST, (TagField::Title, "C")]),
        ],
    );
    let template = Template::parse(TEMPLATE).expect("template");
    let report = apply_renames(
        &database,
        &all_assets(&database),
        &template,
        TEMPLATE,
        |done, total| done == 2 && done < total,
    )
    .expect("renamed");
    assert!(report.stopped);
    assert_eq!(report.renamed, 2);

    let undone = undo_renames(&database, report.operation_id.expect("operation")).expect("undone");
    assert_eq!(undone.restored, 2);
    assert_eq!(names_on_disk(&folder), ["a.mp3", "b.mp3", "c.mp3"]);
}
