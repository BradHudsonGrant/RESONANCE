use resonance_core::columns::{LayoutChange, LibraryColumn, SortColumn, TableLayout};
use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_core::search::{FilterPill, SortDirection, TrackQuery, TrackSortField};
use resonance_db::Database;

fn file(
    path: &str,
    size_bytes: u64,
    bitrate: Option<u32>,
    tags: &[(TagField, &str)],
) -> ScannedFile {
    ScannedFile {
        path: path.to_owned(),
        size_bytes,
        modified_ns: 1_700_000_000_000_000_000 + size_bytes as i64,
        format: AudioFormat::Flac,
        audio: Some(AudioFacts {
            duration_ms: 300_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: Some(16),
            bitrate_kbps: bitrate,
        }),
        tags: tags
            .iter()
            .map(|(field, value)| (*field, (*value).to_owned()))
            .collect(),
    }
}

fn library() -> Database {
    let mut db = Database::open_in_memory().expect("db opens");
    let source = db.source("D:\\Club").expect("source adds");
    db.record_scan(
        source,
        &[
            file(
                "a.flac",
                3,
                Some(320),
                &[
                    (TagField::Title, "Alpha"),
                    (TagField::TrackNumber, "10"),
                    (TagField::Composer, "Zed"),
                    (TagField::CatNo, "LBL002"),
                ],
            ),
            file(
                "b.flac",
                1,
                None,
                &[
                    (TagField::Title, "Bravo"),
                    (TagField::TrackNumber, "9"),
                    (TagField::Remixer, "Ada"),
                ],
            ),
            file(
                "c.flac",
                2,
                Some(128),
                &[
                    (TagField::Title, "Charlie"),
                    (TagField::Composer, "Abe"),
                    (TagField::CatNo, "LBL001"),
                ],
            ),
            file(
                "d.flac",
                4,
                Some(320),
                &[(TagField::Title, "Delta"), (TagField::TrackNumber, "2")],
            ),
        ],
    )
    .expect("scan records");
    db
}

fn titles(db: &Database, query: &TrackQuery) -> Vec<String> {
    db.query_tracks_page(query, 0, 50)
        .expect("page reads")
        .iter()
        .map(|row| {
            row.file
                .tags
                .iter()
                .find(|(field, _)| *field == TagField::Title)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        })
        .collect()
}

fn sorted(sort: TrackSortField, direction: SortDirection) -> TrackQuery {
    TrackQuery {
        sort,
        direction,
        ..TrackQuery::default()
    }
}

#[test]
fn fixed_columns_sort_with_empty_values_last_both_ways() {
    let db = library();
    assert_eq!(
        titles(&db, &sorted(TrackSortField::Composer, SortDirection::Asc)),
        ["Charlie", "Alpha", "Bravo", "Delta"]
    );
    assert_eq!(
        titles(&db, &sorted(TrackSortField::Composer, SortDirection::Desc)),
        ["Alpha", "Charlie", "Delta", "Bravo"]
    );
    assert_eq!(
        titles(&db, &sorted(TrackSortField::CatNo, SortDirection::Asc)),
        ["Charlie", "Alpha", "Bravo", "Delta"]
    );
    assert_eq!(
        titles(&db, &sorted(TrackSortField::FileSize, SortDirection::Desc)),
        ["Delta", "Alpha", "Charlie", "Bravo"]
    );
    assert_eq!(
        titles(&db, &sorted(TrackSortField::Bitrate, SortDirection::Desc)),
        ["Delta", "Alpha", "Charlie", "Bravo"]
    );
}

#[test]
fn a_generic_tag_sorts_numbers_in_order_and_untagged_tracks_last() {
    let db = library();
    let tag = TrackSortField::Tag(TagField::TrackNumber);
    assert_eq!(
        titles(&db, &sorted(tag, SortDirection::Asc)),
        ["Delta", "Bravo", "Alpha", "Charlie"]
    );
    assert_eq!(
        titles(&db, &sorted(tag, SortDirection::Desc)),
        ["Alpha", "Bravo", "Delta", "Charlie"]
    );
    let page = db
        .query_tracks_page(&sorted(tag, SortDirection::Asc), 2, 2)
        .expect("page reads");
    assert_eq!(page.len(), 2);

    let favourites = TrackQuery {
        pill: FilterPill::Favourites,
        ..sorted(tag, SortDirection::Asc)
    };
    assert!(titles(&db, &favourites).is_empty());
}

#[test]
fn a_secondary_sort_orders_tracks_that_tie() {
    let db = library();
    let query = TrackQuery {
        then_sort: Some((TrackSortField::FileSize, SortDirection::Desc)),
        ..sorted(TrackSortField::Bitrate, SortDirection::Desc)
    };
    assert_eq!(titles(&db, &query), ["Delta", "Alpha", "Charlie", "Bravo"]);
    let query = TrackQuery {
        then_sort: Some((TrackSortField::FileSize, SortDirection::Asc)),
        ..sorted(TrackSortField::Bitrate, SortDirection::Desc)
    };
    assert_eq!(titles(&db, &query), ["Alpha", "Delta", "Charlie", "Bravo"]);
    let query = TrackQuery {
        then_sort: Some((TrackSortField::Title, SortDirection::Desc)),
        ..sorted(TrackSortField::Tag(TagField::Subgenre), SortDirection::Asc)
    };
    assert_eq!(titles(&db, &query), ["Delta", "Charlie", "Bravo", "Alpha"]);
}

#[test]
fn a_user_value_moves_a_track_in_the_tag_sort() {
    let mut db = library();
    let charlie = db
        .query_tracks_page(&sorted(TrackSortField::Title, SortDirection::Asc), 2, 1)
        .expect("page reads")[0]
        .track_id;
    db.set_user_tags(&[charlie], &[(TagField::TrackNumber, Some("1".to_owned()))])
        .expect("user value saves");
    let tag = TrackSortField::Tag(TagField::TrackNumber);
    assert_eq!(
        titles(&db, &sorted(tag, SortDirection::Asc))[0],
        "Charlie".to_owned()
    );
}

#[test]
fn every_sort_walks_an_index_without_sorting_the_library() {
    let db = library();
    let mut fields = vec![
        TrackSortField::Title,
        TrackSortField::Artist,
        TrackSortField::Bpm,
        TrackSortField::Key,
        TrackSortField::Duration,
        TrackSortField::Added,
        TrackSortField::Album,
        TrackSortField::Genre,
        TrackSortField::Label,
        TrackSortField::CatNo,
        TrackSortField::Year,
        TrackSortField::Comment,
        TrackSortField::Composer,
        TrackSortField::Remixer,
        TrackSortField::Mix,
        TrackSortField::Format,
        TrackSortField::Bitrate,
        TrackSortField::SampleRate,
        TrackSortField::BitDepth,
        TrackSortField::Channels,
        TrackSortField::FileSize,
        TrackSortField::Modified,
        TrackSortField::Path,
    ];
    fields.extend(resonance_core::columns::generic_tag_fields().map(TrackSortField::Tag));
    for sort in fields {
        for direction in [SortDirection::Asc, SortDirection::Desc] {
            let steps = db
                .explain_track_query(&sorted(sort, direction), 50_000, 200)
                .expect("plan explains");
            assert!(
                steps.iter().all(|step| !step.contains("TEMP B-TREE")),
                "temp sort for {sort:?} {direction:?}: {steps:?}"
            );
            assert!(
                steps.iter().all(|step| !step.contains("file_tag")),
                "file_tag read for {sort:?} {direction:?}: {steps:?}"
            );
        }
    }
}

#[test]
fn a_secondary_sort_only_sorts_within_runs_of_the_primary() {
    let db = library();
    let query = TrackQuery {
        then_sort: Some((TrackSortField::Title, SortDirection::Asc)),
        ..sorted(TrackSortField::Genre, SortDirection::Asc)
    };
    let steps = db
        .explain_track_query(&query, 0, 200)
        .expect("plan explains");
    assert!(
        steps
            .iter()
            .any(|step| step.contains("TEMP B-TREE FOR LAST") || step.contains("RIGHT PART")),
        "expected a partial sort: {steps:?}"
    );
}

#[test]
fn the_table_layout_persists_and_a_new_library_has_the_default() {
    let mut db = library();
    assert_eq!(
        db.table_layout().expect("layout reads"),
        TableLayout::default()
    );

    let mut layout = TableLayout::default();
    layout.apply(LayoutChange::Toggle(LibraryColumn::Tag(
        TagField::TrackNumber,
    )));
    layout.apply(LayoutChange::Toggle(LibraryColumn::Composer));
    layout.apply(LayoutChange::Move {
        column: LibraryColumn::Composer,
        to: 1,
    });
    layout.apply(LayoutChange::Resize {
        column: LibraryColumn::Title,
        width_px: Some(320),
    });
    layout.apply(LayoutChange::SetWrap(true));
    layout.apply(LayoutChange::Sort {
        column: SortColumn::Column(LibraryColumn::Composer),
        add: false,
    });
    layout.apply(LayoutChange::Sort {
        column: SortColumn::Column(LibraryColumn::Tag(TagField::TrackNumber)),
        add: true,
    });
    db.save_table_layout(&layout).expect("layout saves");
    assert_eq!(db.table_layout().expect("layout reads"), layout);

    db.save_table_layout(&TableLayout::default())
        .expect("layout resets");
    assert_eq!(
        db.table_layout().expect("layout reads"),
        TableLayout::default()
    );
}

#[test]
fn only_tags_some_track_has_are_offered_as_columns() {
    let db = library();
    assert_eq!(
        db.tags_in_library().expect("tags read"),
        [TagField::TrackNumber]
    );
}
