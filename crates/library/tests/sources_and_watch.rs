use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::folders::list_subfolders;
use resonance_library::scan::scan;
use resonance_library::watch::SourceWatcher;

fn folder(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sources-and-watch")
        .join(format!("{name}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).expect("the folder can be made");
    root
}

fn tone(path: &Path) {
    fs::create_dir_all(path.parent().expect("the file has a parent"))
        .expect("the folder can be made");
    Signal::tone(440.0, 0.1, -6.0, 48_000)
        .write_wav(path)
        .expect("the fixture can be written");
}

#[test]
fn subfolder_toggles_and_editable_ignore_list_govern_scans_and_folder_trees() {
    let music = folder("toggles");
    tone(&music.join("root.wav"));
    tone(&music.join("House/Deep/blue.wav"));
    tone(&music.join("Samples/Loops/kick.wav"));
    tone(&music.join("_trash/discarded.wav"));
    // Windows strips trailing spaces unless opened via canonical path; create a clean nested tree
    // and check both top-level and nested subfolder listing.
    let mut db = Database::open_in_memory().expect("db opens");

    let first = scan(&mut db, &music, &[]).expect("initial scan runs");
    assert_eq!(first.found, 3);
    assert_eq!(db.track_count().expect("count reads"), 3);

    let source = db.sources().expect("sources read").remove(0);
    let ignored = db.ignored_folders().expect("ignored reads");

    let top = list_subfolders(&source, None, &ignored, &[]).expect("top folders list");
    let names: Vec<_> = top.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["House", "Samples"]);
    assert!(
        top.iter()
            .all(|entry| entry.enabled && entry.has_subfolders)
    );

    let nested = list_subfolders(&source, Some("House"), &ignored, &[]).expect("nested list");
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].path, "House/Deep");
    assert!(nested[0].enabled);

    // Switch off Samples and check both immediate asset hiding and subsequent scan behaviour.
    let updated = db
        .update_source(source.id, true, true, &["Samples"])
        .expect("source updates")
        .expect("source exists");
    assert_eq!(db.track_count().expect("Samples hidden"), 2);
    let top_after = list_subfolders(&updated, None, &ignored, &[]).expect("top folders list");
    let samples = top_after
        .iter()
        .find(|entry| entry.name == "Samples")
        .expect("Samples remains visible in tree so user can re-enable it");
    assert!(!samples.enabled);

    let second = scan(&mut db, &music, &[]).expect("rescan skips Samples");
    assert_eq!(second.found, 2);

    // Edit the ignore list: remove `_trash` and add `Deep`.
    let custom_ignored = db
        .set_ignored_folders(&["Deep"])
        .expect("ignore list updates");
    let third = scan(&mut db, &music, &[]).expect("rescan applies new ignore list");
    assert_eq!(third.added, 1); // `_trash/discarded.wav` enters the library
    let paths: Vec<_> = db
        .tracks()
        .expect("tracks read")
        .into_iter()
        .map(|row| row.file.path)
        .collect();
    assert_eq!(paths, ["_trash/discarded.wav", "root.wav"]);
    let house_children =
        list_subfolders(&updated, Some("House"), &custom_ignored, &[]).expect("nested list");
    assert!(house_children.is_empty());
}

#[test]
fn watcher_coalesces_new_audio_in_included_folders_and_ignores_excluded_ones() {
    let music = folder("watcher");
    fs::create_dir_all(music.join("House")).expect("House made");
    fs::create_dir_all(music.join("Samples")).expect("Samples made");
    tone(&music.join("initial.wav"));

    let mut db = Database::open_in_memory().expect("db opens");
    scan(&mut db, &music, &[]).expect("initial scan");
    let source = db.sources().expect("sources").remove(0);
    let updated = db
        .update_source(source.id, true, true, &["Samples"])
        .expect("exclude Samples")
        .expect("source exists");
    let ignored = db.ignored_folders().expect("ignored");

    let (tx, rx) = mpsc::channel();
    let watcher = SourceWatcher::with_quiet_window(Duration::from_millis(60), move |dirty| {
        let _ = tx.send(dirty.to_path_buf());
    });
    watcher.reconcile(std::slice::from_ref(&updated), &ignored, &[]);
    assert!(watcher.watching_ids().contains(&updated.id));

    // Files inside excluded subfolders or non-audio files should not trigger a rescan.
    tone(&music.join("Samples/ignored.wav"));
    fs::write(music.join("House/cover.jpg"), b"jpeg").expect("cover writes");
    assert!(
        rx.recv_timeout(Duration::from_millis(220)).is_err(),
        "excluded or non-audio files must not mark source dirty"
    );

    // Adding an audio file inside an included subfolder triggers the watcher.
    tone(&music.join("House/arrival.wav"));
    let dirty_folder = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("watcher reports new audio file");
    let report = scan(&mut db, &dirty_folder, &[]).expect("watcher scan runs");
    assert_eq!(report.added, 1);
    assert_eq!(db.track_count().expect("two tracks"), 2);

    // Turning watch off stops OS watching immediately.
    let unwatched = db
        .update_source(source.id, true, false, &["Samples"])
        .expect("watch off")
        .expect("source exists");
    watcher.reconcile(&[unwatched], &ignored, &[]);
    assert!(watcher.watching_ids().is_empty());
}
