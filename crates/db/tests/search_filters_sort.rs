use resonance_core::filter::{FilterTerm, parse_command};
use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_core::search::{FilterPill, SortDirection, TrackQuery, TrackSortField};
use resonance_db::Database;

struct TrackSpec<'a> {
    path: &'a str,
    format: AudioFormat,
    duration_ms: Option<u64>,
    tags: &'a [(TagField, &'a str)],
}

fn scanned(spec: TrackSpec<'_>) -> ScannedFile {
    ScannedFile {
        path: spec.path.to_owned(),
        size_bytes: 5_000_000,
        modified_ns: 1_700_000_000_000_000_000,
        format: spec.format,
        audio: spec.duration_ms.map(|duration_ms| AudioFacts {
            duration_ms,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: Some(16),
            bitrate_kbps: Some(900),
        }),
        tags: spec
            .tags
            .iter()
            .map(|(field, value)| (*field, (*value).to_owned()))
            .collect(),
    }
}

fn seed_catalogue() -> Database {
    let mut db = Database::open_in_memory().expect("db opens");
    let source = db.source("D:\\Club").expect("source adds");
    let files = [
        scanned(TrackSpec {
            path: "01/daft.flac",
            format: AudioFormat::Flac,
            duration_ms: Some(320_000),
            tags: &[
                (TagField::Title, "One More Time"),
                (TagField::Artist, "Daft Punk"),
                (TagField::Album, "Discovery"),
                (TagField::Genre, "French House"),
                (TagField::Label, "Virgin"),
                (TagField::Comment, "classic club weapon"),
                (TagField::Bpm, "122.7"),
                (TagField::Key, "10A"),
                (TagField::Year, "2001"),
            ],
        }),
        scanned(TrackSpec {
            path: "02/tiesto.mp3",
            format: AudioFormat::Mp3,
            duration_ms: Some(240_000),
            tags: &[
                (TagField::Title, "Adagio"),
                (TagField::Artist, "Tiësto"),
                (TagField::Album, "Just Be"),
                (TagField::Genre, "Trance"),
                (TagField::Label, "Anjunadeep"),
                (TagField::Bpm, "138,5"),
                (TagField::Key, "Am"),
                (TagField::Year, "2004-05-15"),
            ],
        }),
        scanned(TrackSpec {
            path: "03/no_bpm.wav",
            format: AudioFormat::Wav,
            duration_ms: None,
            tags: &[
                (TagField::Title, "alpha groove"),
                (TagField::Artist, ""),
                (TagField::Label, "Anjunadeep"),
                (TagField::Key, "2A"),
                (TagField::Year, "2022"),
            ],
        }),
        scanned(TrackSpec {
            path: "04/bravo_major.aiff",
            format: AudioFormat::Aiff,
            duration_ms: Some(410_000),
            tags: &[
                (TagField::Title, "Bravo Lift"),
                (TagField::Artist, "Bicep"),
                (TagField::Bpm, "128"),
                (TagField::Key, "AM"),
                (TagField::Year, "2020"),
            ],
        }),
        scanned(TrackSpec {
            path: "05/untitled_demo.m4a",
            format: AudioFormat::M4a,
            duration_ms: Some(180_000),
            tags: &[
                (TagField::Artist, "Zed"),
                (TagField::Bpm, "126"),
                (TagField::Key, "A minor"),
                (TagField::Year, "2024"),
            ],
        }),
    ];
    db.record_scan(source, &files).expect("scan writes");
    db
}

fn filters(texts: &[&str]) -> Vec<FilterTerm> {
    texts
        .iter()
        .map(|text| parse_command(text).expect("filter reads"))
        .collect()
}

fn titles(db: &Database, query: &TrackQuery) -> Vec<String> {
    db.query_tracks_page(query, 0, 50)
        .expect("page queries")
        .into_iter()
        .map(|row| row.file.title().to_owned())
        .collect()
}

#[test]
fn fts_prefix_diacritics_all_columns_and_punctuation_safety() {
    let mut db = seed_catalogue();

    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                text: Some("daf dis".to_owned()),
                ..TrackQuery::default()
            }
        ),
        vec!["One More Time"]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                text: Some("tiesto".to_owned()),
                ..TrackQuery::default()
            }
        ),
        vec!["Adagio"]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                text: Some("weapon".to_owned()),
                ..TrackQuery::default()
            }
        ),
        vec!["One More Time"]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                text: Some("untitled_demo".to_owned()),
                ..TrackQuery::default()
            }
        ),
        vec!["untitled_demo"]
    );

    // Reserved FTS5 tokens and punctuation must never trigger a syntax error.
    for tricky in [
        "AND OR NOT NEAR",
        "\"unbalanced (paren) title:boom * ^",
        "--- !!! ???",
    ] {
        db.query_track_count(&TrackQuery {
            text: Some(tricky.to_owned()),
            ..TrackQuery::default()
        })
        .expect("tricky FTS query runs cleanly");
    }

    // Exclusion hides matches; restoring brings them back; rescanning replaces old FTS tokens.
    let sources = db.sources().expect("sources read");
    let source_id = sources[0].id;
    db.update_source(source_id, false, true, &([] as [&str; 0]))
        .expect("exclude subfolders");
    assert_eq!(db.query_track_count(&TrackQuery::default()).unwrap(), 0);
    db.update_source(source_id, true, true, &([] as [&str; 0]))
        .expect("restore subfolders");
    assert_eq!(db.query_track_count(&TrackQuery::default()).unwrap(), 5);

    db.remove_source(source_id).expect("source removes");
    assert_eq!(
        db.query_track_count(&TrackQuery {
            text: Some("One".to_owned()),
            ..TrackQuery::default()
        })
        .unwrap(),
        0
    );
}

#[test]
fn filters_handle_bpm_key_aliases_year_label_format_favourites_and_recent() {
    let mut db = seed_catalogue();

    // 8A matches 'Am' and 'A minor', NOT 'AM' (11B).
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                filters: filters(&["key 8A"]),
                ..TrackQuery::default()
            }
        ),
        vec!["Adagio", "untitled_demo"]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                filters: filters(&["key AM"]),
                ..TrackQuery::default()
            }
        ),
        vec!["Bravo Lift"]
    );

    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                filters: filters(&["bpm 125-139"]),
                ..TrackQuery::default()
            }
        ),
        vec!["Adagio", "Bravo Lift", "untitled_demo"]
    );

    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                filters: filters(&["year 2004-2020"]),
                ..TrackQuery::default()
            }
        ),
        vec!["Adagio", "Bravo Lift"]
    );

    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                filters: filters(&["label is anjunadeep"]),
                ..TrackQuery::default()
            }
        ),
        vec!["Adagio", "alpha groove"]
    );

    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                filters: filters(&["format flac, wav"]),
                ..TrackQuery::default()
            }
        ),
        vec!["One More Time", "alpha groove"]
    );

    let all = db.tracks().expect("tracks read");
    let adagio_id = all[1].track_id;
    assert!(
        db.set_track_favourite(adagio_id, true)
            .expect("favourite sets")
    );
    assert!(
        !db.set_track_favourite(999_999, true)
            .expect("missing track ignored")
    );

    let favs = TrackQuery {
        pill: FilterPill::Favourites,
        ..TrackQuery::default()
    };
    assert_eq!(titles(&db, &favs), vec!["Adagio"]);
    assert_eq!(db.query_track_count(&favs).unwrap(), 1);
    assert!(db.query_tracks_page(&favs, 0, 10).unwrap()[0].favourite);

    // Recent orders newly scanned tracks newest first.
    let second_source = db.source("D:\\Fresh").expect("second source");
    db.record_scan(
        second_source,
        &[scanned(TrackSpec {
            path: "newest.flac",
            format: AudioFormat::Flac,
            duration_ms: Some(200_000),
            tags: &[(TagField::Title, "Fresh Arrival")],
        })],
    )
    .expect("second scan stores");

    let recent = TrackQuery {
        pill: FilterPill::Recent,
        ..TrackQuery::default()
    };
    let recent_titles = titles(&db, &recent);
    assert_eq!(
        recent_titles.first().map(String::as_str),
        Some("Fresh Arrival")
    );
    assert_eq!(recent_titles.len(), 6);
}

#[test]
fn all_sorts_put_nulls_last_follow_camelot_and_paginate_cleanly() {
    let db = seed_catalogue();

    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Title,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "Adagio",
            "alpha groove",
            "Bravo Lift",
            "One More Time",
            "untitled_demo"
        ]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Title,
                direction: SortDirection::Desc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "untitled_demo",
            "One More Time",
            "Bravo Lift",
            "alpha groove",
            "Adagio"
        ]
    );

    // Blank artist sorts LAST in both Asc and Desc.
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Artist,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "Bravo Lift",
            "One More Time",
            "Adagio",
            "untitled_demo",
            "alpha groove"
        ]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Artist,
                direction: SortDirection::Desc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "untitled_demo",
            "Adagio",
            "One More Time",
            "Bravo Lift",
            "alpha groove"
        ]
    );

    // Missing BPM sorts LAST in both Asc and Desc.
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Bpm,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "One More Time",
            "untitled_demo",
            "Bravo Lift",
            "Adagio",
            "alpha groove"
        ]
    );
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Bpm,
                direction: SortDirection::Desc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "Adagio",
            "Bravo Lift",
            "untitled_demo",
            "One More Time",
            "alpha groove"
        ]
    );

    // Key sort puts 2A before 10A and groups 8A ('Am' / 'A minor') together.
    assert_eq!(
        titles(
            &db,
            &TrackQuery {
                sort: TrackSortField::Key,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            }
        ),
        vec![
            "alpha groove",
            "Adagio",
            "untitled_demo",
            "One More Time",
            "Bravo Lift"
        ]
    );

    // Paginating BPM sort produces the same sequence without gaps or duplicates.
    let bpm_query = TrackQuery {
        sort: TrackSortField::Bpm,
        direction: SortDirection::Asc,
        ..TrackQuery::default()
    };
    let page1: Vec<String> = db
        .query_tracks_page(&bpm_query, 0, 2)
        .unwrap()
        .into_iter()
        .map(|row| row.file.title().to_owned())
        .collect();
    let page2: Vec<String> = db
        .query_tracks_page(&bpm_query, 2, 2)
        .unwrap()
        .into_iter()
        .map(|row| row.file.title().to_owned())
        .collect();
    let page3: Vec<String> = db
        .query_tracks_page(&bpm_query, 4, 2)
        .unwrap()
        .into_iter()
        .map(|row| row.file.title().to_owned())
        .collect();
    assert_eq!([page1, page2, page3].concat(), titles(&db, &bpm_query));

    let facets = db.library_facets().expect("facets read");
    assert_eq!(facets.labels, vec!["Anjunadeep", "Virgin"]);
    assert_eq!(facets.bpm_min, Some(122.7));
    assert_eq!(facets.bpm_max, Some(138.5));
    assert_eq!(facets.year_min, Some(2001));
    assert_eq!(facets.year_max, Some(2024));
}

#[test]
fn unfiltered_sorts_use_indices_without_temp_sort() {
    let db = seed_catalogue();
    for sort in [
        TrackSortField::Index,
        TrackSortField::Title,
        TrackSortField::Artist,
        TrackSortField::Bpm,
        TrackSortField::Key,
        TrackSortField::Duration,
        TrackSortField::Added,
    ] {
        for direction in [SortDirection::Asc, SortDirection::Desc] {
            let query = TrackQuery {
                sort,
                direction,
                ..TrackQuery::default()
            };
            let steps = db
                .explain_track_query(&query, 50_000, 200)
                .expect("plan explains");
            assert!(
                steps.iter().all(|step| !step.contains("TEMP B-TREE")),
                "unexpected temp sort for {sort:?} {direction:?}: {steps:?}"
            );
        }
    }

    let fts_steps = db
        .explain_track_query(
            &TrackQuery {
                text: Some("daft".to_owned()),
                ..TrackQuery::default()
            },
            0,
            200,
        )
        .expect("fts plan explains");
    assert!(
        fts_steps.iter().any(|step| step.contains("asset_fts")),
        "expected FTS5 index scan in plan: {fts_steps:?}"
    );

    let fav_steps = db
        .explain_track_query(
            &TrackQuery {
                pill: FilterPill::Favourites,
                ..TrackQuery::default()
            },
            0,
            200,
        )
        .expect("favourites plan explains");
    assert!(
        fav_steps
            .iter()
            .any(|step| step.contains("track_favourite")),
        "expected track_favourite partial index in plan: {fav_steps:?}"
    );
}
