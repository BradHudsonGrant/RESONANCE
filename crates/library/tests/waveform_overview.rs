use std::fs;
use std::path::PathBuf;

use resonance_core::columns::LibraryColumn;
use resonance_core::filter::{Condition, FieldFilter, FilterTerm};
use resonance_core::search::TrackQuery;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_library::identity::identify_pending;
use resonance_library::scan::scan;

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder created");
    folder
}

#[test]
fn identifying_a_file_keeps_its_overview_and_a_moved_copy_finds_it() {
    let source = test_folder("waveform_overview");
    let original = source.join("pad.flac");
    Signal::tone(700.0, 3.0, -6.0, 44_100)
        .write_flac(&original)
        .expect("flac written");

    let mut db = Database::open_in_memory().expect("db");
    scan(&mut db, &source, &[]).expect("scan");
    identify_pending(&mut db).expect("identify");
    let track_id = db.tracks().expect("tracks")[0].track_id;

    let overviews = db
        .waveform_overviews(&[track_id, track_id + 1_000])
        .expect("overviews");

    let overview = overviews[0].as_ref().expect("overview kept");
    assert!(overviews[1].is_none());
    // A 700 Hz tone at -6 dBFS peaks at half scale and sits in the mid band.
    assert!(overview.columns.iter().all(|column| column.peak == 128));
    let settled = &overview.columns[10..130];
    assert!(
        settled
            .iter()
            .all(|column| column.mid > 80 && column.low < 10)
    );
    assert!(
        db.pending_identity_assets(0, "", 10)
            .expect("pending")
            .is_empty()
    );

    let moved = source.join("Crate").join("pad-moved.flac");
    fs::create_dir_all(moved.parent().expect("parent")).expect("subfolder");
    fs::rename(&original, &moved).expect("moved");
    scan(&mut db, &source, &[]).expect("rescan");
    identify_pending(&mut db).expect("identify again");

    let tracks = db.tracks().expect("tracks");
    assert_eq!(tracks.len(), 1);
    let after = db
        .waveform_overviews(&[tracks[0].track_id])
        .expect("overviews after the move");
    assert_eq!(after[0].as_ref(), Some(overview));
}

#[test]
fn the_waveform_filter_finds_tracks_not_read_yet() {
    let source = test_folder("waveform_filter");
    Signal::tone(300.0, 1.0, -6.0, 48_000)
        .write_wav(&source.join("tone.wav"))
        .expect("wav written");
    let mut db = Database::open_in_memory().expect("db");
    scan(&mut db, &source, &[]).expect("scan");

    let count = |db: &Database, condition: Condition| {
        let query = TrackQuery {
            filters: vec![FilterTerm::one(
                FieldFilter::new(LibraryColumn::Waveform, condition).expect("filter"),
            )],
            ..TrackQuery::default()
        };
        db.query_tracks_page(&query, 0, 10).expect("page").len()
    };
    assert_eq!(count(&db, Condition::Missing), 1);
    assert_eq!(count(&db, Condition::Present), 0);

    identify_pending(&mut db).expect("identify");

    assert_eq!(count(&db, Condition::Missing), 0);
    assert_eq!(count(&db, Condition::Present), 1);
}
