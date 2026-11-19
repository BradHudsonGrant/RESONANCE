#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::path::PathBuf;
use std::time::Instant;

use resonance_core::filter::parse_command;
use resonance_core::search::TrackQuery;
use resonance_db::Database;

fn timed(mut run: impl FnMut()) -> (f64, f64, f64) {
    let mut times: Vec<f64> = (0..21)
        .map(|_| {
            let started = Instant::now();
            run();
            started.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    let cold = times.remove(0);
    times.sort_by(f64::total_cmp);
    (cold, times[times.len() / 2], times[times.len() - 1])
}

/// Times each kind of filter, and three together, on the library `just synthetic-library` makes.
/// A `tracks` call is the first page of 200 rows and the count, as the table reads them. The
/// token counts are the separate call the bar makes when more than one filter is active.
#[test]
#[ignore = "measures filters over target/bench/synthetic/library.db"]
fn filters_on_the_synthetic_library() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/bench/synthetic/library.db");
    let db = Database::open(&path).unwrap();
    let total = db.query_track_count(&TrackQuery::default()).unwrap();
    println!("{total} tracks");
    let plans = std::env::var_os("FILTER_PLANS").is_some();

    let cases: &[&[&str]] = &[
        &["genre contains house"],
        &["artist is Kai Vega"],
        &["genre not techno"],
        &["artist starts with ka"],
        &["label is any of Anjunadeep, Toolroom"],
        &["missing genre"],
        &["has label"],
        &["no artwork"],
        &["missing track number"],
        &["track number is 7"],
        &["bpm 124-128"],
        &["bitrate < 320"],
        &["year >= 2015"],
        &["added after 2026-01-01"],
        &["format flac, wav"],
        &["key 8A, 9A"],
        &["format not mp3"],
        &["path contains deep"],
        &["missing genre", "bitrate < 320", "format mp3"],
        &["bpm 120-130", "has label", "genre not techno"],
    ];
    println!("| Filter | Matches | Cold | Median | Slowest | Token counts, median |");
    for case in cases {
        let query = TrackQuery {
            filters: case
                .iter()
                .map(|text| parse_command(text).unwrap())
                .collect(),
            ..TrackQuery::default()
        };
        if plans {
            println!("count {:?}", db.explain_track_count(&query).unwrap());
            println!("page {:?}", db.explain_track_query(&query, 0, 200).unwrap());
        }
        let matched = db.query_track_count(&query).unwrap();
        let (cold, median, worst) = timed(|| {
            db.query_tracks_and_total(&query, 0, 200).unwrap();
        });
        let counts = if case.len() > 1 {
            let (_, median, _) = timed(|| {
                db.query_filter_counts(&query).unwrap();
            });
            format!("{median:.1} ms")
        } else {
            "–".to_owned()
        };
        println!(
            "| {} | {matched} | {cold:.1} ms | {median:.1} ms | {worst:.1} ms | {counts} |",
            case.join(", ")
        );
    }
}
