use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use resonance_core::journal::OperationState;
use resonance_core::library::TagField;
use resonance_core::replace::{CueKind, ReplaceChoice, ReplaceVerdict};
use resonance_core::search::TrackQuery;
use resonance_core::template::Template;
use resonance_db::Database;
use resonance_fixtures::signal::Groove;
use resonance_library::file_tags;
use resonance_library::journal::{byte_hash_hex, undo};
use resonance_library::replace::{apply, compare, current_file};
use resonance_library::scan::scan;

const GROOVE: Groove = Groove {
    tempo_bpm: 124.0,
    seed: 11,
};
/// How much later the shop's FLAC starts than the MP3 the DJ bought years ago.
const FLAC_LATER_S: f64 = 0.0478;

struct Setup {
    music: PathBuf,
    incoming: PathBuf,
    database: Database,
    track_id: i64,
}

fn ffmpeg(args: &[&str]) {
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error"])
        .args(args)
        .status()
        .expect("FFmpeg is installed for the replace tests");
    assert!(status.success(), "FFmpeg ran: {args:?}");
}

fn text(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

/// A 128 kbps MP3 in a collection with cues on its beats, and a FLAC of the same master.
fn setup(test: &str) -> Setup {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&root);
    let music = root.join("music");
    let incoming = root.join("incoming");
    fs::create_dir_all(&music).expect("music folder");
    fs::create_dir_all(&incoming).expect("incoming folder");
    let master = incoming.join("master.wav");
    GROOVE
        .render(75.0, 44_100, 0.0, 1.0)
        .write_wav(&master)
        .expect("master written");
    ffmpeg(&[
        "-i",
        text(&master),
        "-c:a",
        "libmp3lame",
        "-b:a",
        "128k",
        "-metadata",
        "title=Night Drive",
        "-metadata",
        "artist=Test Artist",
        "-metadata",
        "genre=House",
        text(&music.join("night drive 128.mp3")),
    ]);
    GROOVE
        .render(75.0 + FLAC_LATER_S, 44_100, FLAC_LATER_S, 1.0)
        .write_flac(&incoming.join("Night Drive.flac"))
        .expect("FLAC written");
    fs::remove_file(&master).expect("master removed");

    let mut database = Database::open(&root.join("library.db")).expect("database");
    scan(&mut database, &music, &[]).expect("scan");
    let asset = database
        .query_asset_ids(&TrackQuery::default())
        .expect("assets")[0];
    let track_id = database
        .asset_file(asset)
        .expect("asset")
        .expect("known")
        .track_id;
    let crate_ = database.create_collection("Warm up").expect("collection");
    database
        .add_tracks_to_collection(crate_.id, &[track_id])
        .expect("added");
    database
        .set_user_tags(
            &[track_id],
            &[(TagField::Genre, Some("Deep house".to_owned()))],
        )
        .expect("user tag");
    for beat in [8, 40, 120] {
        database
            .add_cue(track_id, CueKind::Hot, beat_s(beat))
            .expect("cue");
    }
    Setup {
        music,
        incoming,
        database,
        track_id,
    }
}

fn beat_s(beat: u32) -> f64 {
    f64::from(beat) * 60.0 / GROOVE.tempo_bpm
}

fn names(folder: &Path) -> Vec<String> {
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

fn tag(tags: &[(TagField, String)], field: TagField) -> Option<&str> {
    tags.iter()
        .find(|(taken, _)| *taken == field)
        .map(|(_, value)| value.as_str())
}

#[test]
fn replacing_an_mp3_with_a_flac_keeps_the_track_and_lands_cues_then_undo_restores_it() {
    let mut setup = setup("replace-mp3-with-flac");
    let old_path = setup.music.join("night drive 128.mp3");
    let old_hash = byte_hash_hex(&old_path).expect("hash");
    let template = Template::parse("{artist} - {title}").expect("template");

    let current = current_file(&setup.database, setup.track_id).expect("current file");
    let comparison = compare(
        current,
        &setup.incoming.join("Night Drive.flac"),
        Some(&template),
    )
    .expect("compared");
    let offset_ms = comparison.offset_ms().expect("lined up");
    println!(
        "verdict {:?}, offset {offset_ms:.3} ms, similarity {:?}, old cutoff {:?}",
        comparison.verdict,
        comparison.fingerprint_similarity,
        comparison.old.measurements.spectral_cutoff_hz
    );
    assert_eq!(comparison.verdict, ReplaceVerdict::SameRecordingAndBetter);
    assert!((offset_ms - FLAC_LATER_S * 1000.0).abs() < 1.0);
    assert_eq!(comparison.replace_name, "Test Artist - Night Drive.flac");
    assert_eq!(comparison.cues_to_carry, 3);

    let report = apply(&mut setup.database, &comparison, ReplaceChoice::Replace).expect("applied");
    assert_eq!(report.operation.state, OperationState::Applied);
    assert_eq!(report.track_id, setup.track_id);
    assert_eq!(names(&setup.music), ["Test Artist - Night Drive.flac"]);

    let database = &setup.database;
    let shown = database
        .track_by_id(setup.track_id)
        .expect("track")
        .expect("still there");
    assert_eq!(shown.file.path, "Test Artist - Night Drive.flac");
    let collections = database
        .collections_for_track(setup.track_id)
        .expect("collections");
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].name, "Warm up");
    assert_eq!(database.track_count().expect("count"), 1);

    let written = file_tags::read(&report.placed_path).expect("tags read");
    assert_eq!(tag(&written.tags, TagField::Title), Some("Night Drive"));
    assert_eq!(tag(&written.tags, TagField::Artist), Some("Test Artist"));
    assert_eq!(tag(&written.tags, TagField::Genre), Some("Deep house"));
    assert!(
        database
            .unwritten_user_fields(setup.track_id)
            .expect("fields")
            .is_empty()
    );

    let alignment = database
        .track_identity(setup.track_id)
        .expect("identity")
        .expect("identified")
        .alignment();
    let cues = database.cues(setup.track_id).expect("cues");
    assert_eq!(cues.len(), 3);
    for (cue, beat) in cues.iter().zip([8, 40, 120]) {
        assert!(cue.carried_over_check);
        let landed = alignment.track_to_asset_seconds(cue.position_s, 44_100);
        let error_ms = (landed - (beat_s(beat) + FLAC_LATER_S)).abs() * 1000.0;
        println!("cue on beat {beat}: {error_ms:.4} ms out");
        assert!(error_ms < 5.0, "cue on beat {beat} lands {error_ms} ms out");
    }

    undo(&mut setup.database, report.operation.id).expect("undone");
    let database = &setup.database;
    assert_eq!(names(&setup.music), ["night drive 128.mp3"]);
    assert_eq!(byte_hash_hex(&old_path).expect("hash"), old_hash);
    let shown = database
        .track_by_id(setup.track_id)
        .expect("track")
        .expect("still there");
    assert_eq!(shown.file.path, "night drive 128.mp3");
    assert_eq!(tag(&shown.file.tags, TagField::Genre), Some("House"));
    assert_eq!(tag(&shown.user_tags, TagField::Genre), Some("Deep house"));
    assert_eq!(
        database
            .unwritten_user_fields(setup.track_id)
            .expect("fields"),
        ["genre"]
    );
    assert_eq!(
        database
            .collections_for_track(setup.track_id)
            .expect("c")
            .len(),
        1
    );
    assert_eq!(database.track_count().expect("count"), 1);
    let alignment = database
        .track_identity(setup.track_id)
        .expect("identity")
        .expect("identified")
        .alignment();
    assert_eq!(alignment.offset_samples(), 0);
    for (cue, beat) in database
        .cues(setup.track_id)
        .expect("cues")
        .iter()
        .zip([8, 40, 120])
    {
        assert!(!cue.carried_over_check);
        assert!((cue.position_s - beat_s(beat)).abs() < 1e-9);
    }
}

#[test]
fn a_different_version_is_added_as_a_track_of_its_own() {
    let mut setup = setup("replace-different-version");
    let edit = setup.incoming.join("Night Drive (Radio Edit).flac");
    Groove {
        tempo_bpm: 124.0,
        seed: 12,
    }
    .render(40.0, 44_100, 0.0, 1.0)
    .write_flac(&edit)
    .expect("edit written");

    let current = current_file(&setup.database, setup.track_id).expect("current file");
    let comparison = compare(current, &edit, None).expect("compared");
    assert_eq!(comparison.verdict, ReplaceVerdict::DifferentVersion);
    assert_eq!(
        comparison.verdict.default_choice(),
        ReplaceChoice::AddSeparateTrack
    );
    let report = apply(
        &mut setup.database,
        &comparison,
        ReplaceChoice::AddSeparateTrack,
    )
    .expect("added");
    assert_ne!(report.track_id, setup.track_id);
    assert_eq!(setup.database.track_count().expect("count"), 2);
    assert_eq!(
        names(&setup.music),
        ["Night Drive (Radio Edit).flac", "night drive 128.mp3"]
    );

    undo(&mut setup.database, report.operation.id).expect("undone");
    assert_eq!(setup.database.track_count().expect("count"), 1);
    assert_eq!(names(&setup.music), ["night drive 128.mp3"]);
}
