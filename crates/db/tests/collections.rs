use resonance_core::identity::{AssetIdentity, ContentHash};
use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_core::search::{FilterPill, SortDirection, TrackQuery, TrackSortField};
use resonance_db::{Database, DbError, TrackCollection};

fn scanned(path: &str, format: AudioFormat, tags: &[(TagField, &str)]) -> ScannedFile {
    ScannedFile {
        path: path.to_owned(),
        size_bytes: 5_000_000,
        modified_ns: 1_700_000_000_000_000_000,
        format,
        audio: Some(AudioFacts {
            duration_ms: 300_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: Some(16),
            bitrate_kbps: Some(900),
        }),
        tags: tags
            .iter()
            .map(|(field, value)| (*field, (*value).to_owned()))
            .collect(),
    }
}

fn seed_catalogue() -> Database {
    let mut db = Database::open_in_memory().expect("db opens");
    let source = db.source("D:\\Club").expect("source adds");
    let files = [
        scanned(
            "01/alpha.flac",
            AudioFormat::Flac,
            &[
                (TagField::Title, "Alpha Pulse"),
                (TagField::Artist, "Bicep"),
                (TagField::Bpm, "124"),
                (TagField::Key, "8A"),
            ],
        ),
        scanned(
            "02/bravo.mp3",
            AudioFormat::Mp3,
            &[
                (TagField::Title, "Bravo Dusk"),
                (TagField::Artist, "Four Tet"),
                (TagField::Bpm, "128"),
                (TagField::Key, "Am"),
            ],
        ),
        scanned(
            "sub/03/charlie.flac",
            AudioFormat::Flac,
            &[
                (TagField::Title, "Charlie Mist"),
                (TagField::Artist, "Bicep"),
                (TagField::Bpm, "132"),
                (TagField::Key, "5A"),
            ],
        ),
        scanned(
            "04/delta.aiff",
            AudioFormat::Aiff,
            &[
                (TagField::Title, "Delta Run"),
                (TagField::Artist, "Overmono"),
                (TagField::Bpm, "136"),
                (TagField::Key, "8A"),
            ],
        ),
    ];
    db.record_scan(source, &files).expect("scan writes");
    db
}

#[test]
fn collection_crud_validation_and_safe_delete() {
    let mut db = seed_catalogue();
    let tracks = db.tracks().expect("tracks list");
    assert_eq!(tracks.len(), 4);

    assert!(matches!(
        db.create_collection("   "),
        Err(DbError::InvalidCollectionName(_))
    ));
    assert!(matches!(
        db.create_collection("Warm\nUp"),
        Err(DbError::InvalidCollectionName(_))
    ));

    let warm = db
        .create_collection("  Warm Up  ")
        .expect("creates normalised collection");
    assert_eq!(warm.name, "Warm Up");
    assert_eq!(warm.position, 0);
    assert_eq!(warm.track_count, 0);

    assert!(matches!(
        db.create_collection("warm up"),
        Err(DbError::DuplicateCollectionName { .. })
    ));

    let peak = db.create_collection("Peak Hour").expect("second creates");
    assert_eq!(peak.position, 1);

    assert!(matches!(
        db.rename_collection(peak.id, "WARM UP"),
        Err(DbError::DuplicateCollectionName { .. })
    ));

    let renamed = db
        .rename_collection(warm.id, "  warm up  ")
        .expect("casing change succeeds")
        .expect("collection exists");
    assert_eq!(renamed.name, "warm up");

    assert_eq!(db.rename_collection(999_999, "Ghost").expect("ok"), None);

    db.add_tracks_to_collection(warm.id, &[tracks[0].track_id, tracks[1].track_id])
        .expect("adds tracks");
    assert!(db.delete_collection(warm.id).expect("deletes"));
    assert!(!db.delete_collection(warm.id).expect("repeat is false"));

    assert_eq!(db.collections().expect("lists"), vec![peak]);
    assert_eq!(db.track_count().expect("count unchanged"), 4);
    for track in db.tracks().expect("tracks remain") {
        assert!(track.collections.is_empty());
    }
}

#[test]
fn membership_order_idempotency_removal_and_row_hydration() {
    let mut db = seed_catalogue();
    let tracks = db.tracks().expect("tracks list");
    let col_a = db.create_collection("First").expect("first creates");
    let col_b = db.create_collection("Second").expect("second creates");

    let t3 = tracks[2].track_id;
    let t1 = tracks[0].track_id;
    let t2 = tracks[1].track_id;

    let updated = db
        .add_tracks_to_collection(col_a.id, &[t3, t1, t3, 999_999, t2, t1])
        .expect("adds tracks")
        .expect("collection exists");
    assert_eq!(updated.track_count, 3);

    db.add_tracks_to_collection(col_b.id, &[t1])
        .expect("adds to second");

    let page_1 = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col_a.id),
                sort: TrackSortField::Index,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            },
            0,
            2,
        )
        .expect("page 1 loads");
    let page_2 = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col_a.id),
                sort: TrackSortField::Index,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            },
            2,
            2,
        )
        .expect("page 2 loads");
    let ids: Vec<i64> = page_1
        .iter()
        .chain(page_2.iter())
        .map(|track| track.track_id)
        .collect();
    assert_eq!(ids, vec![t3, t1, t2]);

    assert_eq!(
        page_1[1].collections,
        vec![
            TrackCollection {
                id: col_a.id,
                name: "First".to_owned(),
            },
            TrackCollection {
                id: col_b.id,
                name: "Second".to_owned(),
            },
        ]
    );

    let all_rows = db.tracks().expect("tracks() loads");
    let paged_rows = db.tracks_page(0, 10).expect("tracks_page() loads");
    assert_eq!(all_rows[0].collections, page_1[1].collections);
    assert_eq!(paged_rows[0].collections, page_1[1].collections);

    let after_remove = db
        .remove_tracks_from_collection(col_a.id, &[t1])
        .expect("removes")
        .expect("exists");
    assert_eq!(after_remove.track_count, 2);

    let remaining: Vec<i64> = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col_a.id),
                sort: TrackSortField::Index,
                direction: SortDirection::Asc,
                ..TrackQuery::default()
            },
            0,
            10,
        )
        .expect("remaining query")
        .into_iter()
        .map(|track| track.track_id)
        .collect();
    assert_eq!(remaining, vec![t3, t2]);
    assert_eq!(db.track_count().expect("tracks untouched"), 4);

    assert_eq!(
        db.add_tracks_to_collection(999_999, &[t1]).expect("ok"),
        None
    );
    assert_eq!(
        db.remove_tracks_from_collection(999_999, &[t1])
            .expect("ok"),
        None
    );
}

fn track_id_for(tracks: &[resonance_db::TrackRow], path: &str) -> i64 {
    tracks
        .iter()
        .find(|track| track.file.path == path)
        .map(|track| track.track_id)
        .expect("seeded path exists")
}

#[test]
fn collection_queries_combine_search_filters_sorts_without_temp_btree() {
    let mut db = seed_catalogue();
    let tracks = db.tracks().expect("tracks");
    let alpha = track_id_for(&tracks, "01/alpha.flac");
    let bravo = track_id_for(&tracks, "02/bravo.mp3");
    let charlie = track_id_for(&tracks, "sub/03/charlie.flac");
    let delta = track_id_for(&tracks, "04/delta.aiff");

    db.set_track_favourite(charlie, true)
        .expect("favourites charlie");

    let col = db.create_collection("Bicep Box").expect("creates");
    db.add_tracks_to_collection(col.id, &[delta, alpha, charlie, bravo])
        .expect("adds all four");

    for direction in [SortDirection::Asc, SortDirection::Desc] {
        let plan = db
            .explain_track_query(
                &TrackQuery {
                    collection_id: Some(col.id),
                    sort: TrackSortField::Index,
                    direction,
                    ..TrackQuery::default()
                },
                0,
                50,
            )
            .expect("explain runs");
        assert!(
            plan.iter().all(|step| !step.contains("TEMP B-TREE")),
            "unexpected temp b-tree for collection index sort ({direction:?}): {plan:?}"
        );
    }

    let desc_paths: Vec<String> = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col.id),
                sort: TrackSortField::Index,
                direction: SortDirection::Desc,
                ..TrackQuery::default()
            },
            0,
            10,
        )
        .expect("desc query")
        .into_iter()
        .map(|t| t.file.path)
        .collect();
    assert_eq!(
        desc_paths,
        vec![
            "02/bravo.mp3",
            "sub/03/charlie.flac",
            "01/alpha.flac",
            "04/delta.aiff",
        ]
    );

    let search_q = TrackQuery {
        collection_id: Some(col.id),
        text: Some("bicep".to_owned()),
        filters: ["bpm 120-135", "format flac"]
            .map(|text| resonance_core::filter::parse_command(text).expect("filter reads"))
            .to_vec(),
        sort: TrackSortField::Bpm,
        direction: SortDirection::Desc,
        ..TrackQuery::default()
    };
    assert_eq!(db.query_track_count(&search_q).expect("count"), 2);
    let searched: Vec<String> = db
        .query_tracks_page(&search_q, 0, 10)
        .expect("search")
        .into_iter()
        .map(|t| t.file.path)
        .collect();
    assert_eq!(searched, vec!["sub/03/charlie.flac", "01/alpha.flac"]);

    let fav_q = TrackQuery {
        collection_id: Some(col.id),
        pill: FilterPill::Favourites,
        ..TrackQuery::default()
    };
    assert_eq!(db.query_track_count(&fav_q).expect("fav count"), 1);
    let fav_tracks = db.query_tracks_page(&fav_q, 0, 10).expect("fav tracks");
    assert_eq!(fav_tracks.len(), 1);
    assert_eq!(fav_tracks[0].file.path, "sub/03/charlie.flac");

    let empty_col = db.create_collection("Empty").expect("empty");
    assert_eq!(
        db.query_track_count(&TrackQuery {
            collection_id: Some(empty_col.id),
            ..TrackQuery::default()
        })
        .expect("count"),
        0
    );
}

#[test]
fn exclusion_source_removal_and_external_file_moves_keep_collections_consistent() {
    let mut db = seed_catalogue();
    let source = db.sources().expect("sources")[0].id;
    let tracks = db.tracks().expect("tracks");
    let t1 = track_id_for(&tracks, "01/alpha.flac");
    let t2 = track_id_for(&tracks, "02/bravo.mp3");
    let t3 = track_id_for(&tracks, "sub/03/charlie.flac");

    let col = db.create_collection("Moved Set").expect("creates");
    db.add_tracks_to_collection(col.id, &[t3, t1, t2])
        .expect("adds three");
    assert_eq!(db.collections().expect("lists")[0].track_count, 3);

    // Excluding "sub" hides t3 without discarding its membership.
    db.update_source(source, true, true, &["sub"])
        .expect("updates excluded folder");
    assert_eq!(db.collections().expect("lists")[0].track_count, 2);
    let visible: Vec<i64> = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col.id),
                ..TrackQuery::default()
            },
            0,
            10,
        )
        .expect("visible")
        .into_iter()
        .map(|t| t.track_id)
        .collect();
    assert_eq!(visible, vec![t1, t2]);

    // Restoring "sub" returns t3 at its original position 0.
    db.update_source(source, true, true, &[""])
        .expect("clears excluded folder");
    assert_eq!(db.collections().expect("lists")[0].track_count, 3);
    let restored: Vec<i64> = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col.id),
                ..TrackQuery::default()
            },
            0,
            10,
        )
        .expect("restored")
        .into_iter()
        .map(|t| t.track_id)
        .collect();
    assert_eq!(restored, vec![t3, t1, t2]);

    // Case 1: file renamed/moved on disk and reconciled via record_moved_scan_file.
    let asset_t1 = db.track_identity(t1).expect("id").expect("exists").asset_id;
    let moved_t1 = scanned(
        "01/alpha_renamed.flac",
        AudioFormat::Flac,
        &[
            (TagField::Title, "Alpha Pulse"),
            (TagField::Artist, "Bicep"),
            (TagField::Bpm, "124"),
            (TagField::Key, "8A"),
        ],
    );
    db.record_moved_scan_file(asset_t1, source, &moved_t1, None)
        .expect("moved scan file records");

    // Case 2: file scanned at new path then reconciled via reconcile_moved_asset.
    let asset_t2 = db.track_identity(t2).expect("id").expect("exists").asset_id;
    let new_bravo = scanned(
        "09/bravo_relocated.mp3",
        AudioFormat::Mp3,
        &[
            (TagField::Title, "Bravo Dusk"),
            (TagField::Artist, "Four Tet"),
            (TagField::Bpm, "128"),
            (TagField::Key, "Am"),
        ],
    );
    db.record_scan(source, &[new_bravo])
        .expect("records new asset");
    let duplicate_bravo_asset = db
        .tracks()
        .expect("tracks")
        .into_iter()
        .find(|t| t.file.path == "09/bravo_relocated.mp3")
        .and_then(|t| db.track_identity(t.track_id).expect("identity query"))
        .map(|stored| stored.asset_id)
        .expect("new asset id");
    let identity = AssetIdentity {
        byte_hash: ContentHash::from_bytes([7_u8; 32]),
        audio: None,
        chromaprint: None,
        decoded_facts: None,
        quality: None,
    };
    assert!(
        db.reconcile_moved_asset(asset_t2, duplicate_bravo_asset, &identity)
            .expect("reconciles")
    );

    let after_moves = db
        .query_tracks_page(
            &TrackQuery {
                collection_id: Some(col.id),
                ..TrackQuery::default()
            },
            0,
            10,
        )
        .expect("queries after move");
    let after_paths: Vec<&str> = after_moves.iter().map(|t| t.file.path.as_str()).collect();
    assert_eq!(
        after_paths,
        vec![
            "sub/03/charlie.flac",
            "01/alpha_renamed.flac",
            "09/bravo_relocated.mp3",
        ]
    );
    assert_eq!(db.collections().expect("lists")[0].track_count, 3);

    // Removing the source drops tracks and cascades collection_item entries cleanly.
    assert!(db.remove_source(source).expect("removes source"));
    assert_eq!(db.collections().expect("lists")[0].track_count, 0);
}
