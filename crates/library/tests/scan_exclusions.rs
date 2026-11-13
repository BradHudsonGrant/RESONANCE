use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::scan::{ScanError, scan};

fn folder(name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("scan-exclusions")
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

fn contents(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(folder)
        .expect("the folder can be read")
        .map(|entry| {
            let entry = entry.expect("the entry can be read");
            (
                entry.file_name().to_string_lossy().into_owned(),
                fs::read(entry.path()).expect("the file can be read"),
            )
        })
        .collect();
    files.sort();
    files
}

#[test]
fn a_marked_checkout_is_skipped_when_the_program_is_installed_elsewhere() {
    let music = folder("installed-elsewhere");
    let checkout = music.join("Archives/another-checkout");
    tone(&music.join("track.wav"));
    tone(&checkout.join("fixtures-local/copy.wav"));
    fs::write(checkout.join(".resonance-ignore"), []).expect("the marker can be written");
    let mut database = Database::open_in_memory().expect("the library opens");

    let report = scan(&mut database, &music, &[]).expect("the source can be scanned");

    assert_eq!(report.found, 1);
    assert!(report.unreadable.is_empty());
    let tracks = database.tracks().expect("the tracks can be read");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].file.path, "track.wav");
}

#[test]
fn a_marked_source_is_rejected_before_any_library_write() {
    let root = folder("marked-source");
    let marked = root.join("checkout");
    tone(&marked.join("fixture.wav"));
    fs::write(marked.join(".resonance-ignore"), []).expect("the marker can be written");
    let data = root.join("data");
    let mut database = Database::open(&data.join("library.db")).expect("the library opens");
    let before = contents(&data);

    let error = scan(&mut database, &marked, &[]).expect_err("a marked source is excluded");

    assert!(matches!(&error, ScanError::ExcludedFolder { path } if path == &marked));
    assert!(error.to_string().contains(".resonance-ignore"));
    assert!(error.to_string().contains("Choose"));
    assert_eq!(contents(&data), before);
    assert_eq!(
        database.track_count().expect("the track count can be read"),
        0
    );
}

#[test]
fn fixtures_below_a_marked_checkout_can_be_scanned_directly() {
    let checkout = folder("explicit-fixtures");
    let fixtures = checkout.join("fixtures-local");
    fs::write(checkout.join(".resonance-ignore"), []).expect("the marker can be written");
    tone(&fixtures.join("fixture.wav"));
    let mut database = Database::open_in_memory().expect("the library opens");

    let report = scan(&mut database, &fixtures, &[]).expect("the explicit source can be scanned");

    assert_eq!(report.added, 1);
    assert!(report.unreadable.is_empty());
    assert_eq!(
        database.tracks().expect("the tracks can be read")[0]
            .file
            .path,
        "fixture.wav"
    );
}
