use std::fs;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::prelude::{Accessor, AudioFile, TaggedFileExt};
use lofty::tag::{Tag, TagType};
use resonance_core::analysis::{
    AnalysisKey, AnalysisProvenance, AnalysisRun, AnalysisStatus, BeatGrid,
};
use resonance_core::identity::ContentHash;
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

fn retitle(path: &Path, title: &str) {
    let mut tagged = lofty::read_from_path(path).expect("flac opens for tagging");
    if tagged.primary_tag().is_none() {
        tagged.insert_tag(Tag::new(TagType::VorbisComments));
    }
    let tag = tagged.primary_tag_mut().expect("tag present");
    tag.set_title(title.to_owned());
    tagged
        .save_to_path(path, WriteOptions::default())
        .expect("tags saved");
}

fn audio_hash(db: &Database, track_id: i64) -> ContentHash {
    db.track_identity(track_id)
        .expect("query")
        .expect("identity")
        .identity
        .expect("identified")
        .audio
        .expect("decoded")
        .hash
}

fn grid(bpm: f64, first_beat_s: f64) -> BeatGrid {
    let step = 60.0 / bpm;
    BeatGrid {
        bpm,
        beats_s: (0..8)
            .map(|beat| first_beat_s + f64::from(beat) * step)
            .collect(),
        first_downbeat: 0,
    }
}

#[test]
fn a_renamed_and_retagged_file_finds_its_run_and_keeps_the_users_grid() {
    let source = test_folder("analysis_cache");
    let original = source.join("groove.flac");
    Signal::tone(220.0, 4.0, -6.0, 44_100)
        .write_flac(&original)
        .expect("flac written");
    retitle(&original, "Groove");

    let mut db = Database::open_in_memory().expect("db");
    scan(&mut db, &source, &[]).expect("scan");
    identify_pending(&mut db).expect("identify");
    let track_id = db.tracks().expect("tracks")[0].track_id;
    let hash = audio_hash(&db, track_id);

    let key = AnalysisKey {
        analyser: "beat_grid".to_owned(),
        version: "1".to_owned(),
        param_hash: [7; 32],
    };
    let run = AnalysisRun {
        key: key.clone(),
        status: AnalysisStatus::Done,
        confidence: Some(0.9),
        completed_ns: 1,
    };
    db.record_analysis_run(&hash, &run).expect("run recorded");
    db.store_beat_grid_auto(&hash, &grid(128.0, 0.0), 0.9)
        .expect("grid stored");

    let renamed = source.join("Crate").join("groove-renamed.flac");
    fs::create_dir_all(renamed.parent().expect("parent")).expect("subfolder");
    fs::rename(&original, &renamed).expect("renamed");
    retitle(&renamed, "Groove (club edit)");
    scan(&mut db, &source, &[]).expect("scan after rename and retag");
    identify_pending(&mut db).expect("identify after rename and retag");

    let tracks = db.tracks().expect("tracks");
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].track_id, track_id);
    assert_eq!(tracks[0].file.title(), "Groove (club edit)");
    let hash_after = audio_hash(&db, track_id);
    assert_eq!(hash_after, hash);
    assert_eq!(
        db.analysis_run(&hash_after, &key).expect("lookup"),
        Some(run)
    );

    let newer = AnalysisKey {
        version: "2".to_owned(),
        ..key.clone()
    };
    assert_eq!(db.analysis_run(&hash_after, &newer).expect("lookup"), None);
    let other_settings = AnalysisKey {
        param_hash: [8; 32],
        ..key
    };
    assert_eq!(
        db.analysis_run(&hash_after, &other_settings)
            .expect("lookup"),
        None
    );

    let estimated = db
        .effective_beat_grid(track_id)
        .expect("query")
        .expect("grid");
    assert_eq!(estimated.grid, grid(128.0, 0.0));
    assert_eq!(
        estimated.provenance,
        AnalysisProvenance::Estimated { confidence: 0.9 }
    );

    let corrected = grid(127.5, 0.012);
    db.set_beat_grid_user(track_id, &corrected)
        .expect("correction stored");
    db.store_beat_grid_auto(&hash, &grid(64.0, 0.5), 0.4)
        .expect("new estimate stored");
    let shown = db
        .effective_beat_grid(track_id)
        .expect("query")
        .expect("grid");
    assert_eq!(shown.grid, corrected);
    assert_eq!(shown.provenance, AnalysisProvenance::ConfirmedByUser);
}
