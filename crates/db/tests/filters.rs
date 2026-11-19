#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::PathBuf;

use resonance_core::columns::LibraryColumn;
use resonance_core::filter::{Condition, FieldFilter, FilterTerm, parse_command};
use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_core::quick_filters::{QuickFilterBar, QuickFilterChange};
use resonance_core::search::{FilterPill, TrackQuery};
use resonance_db::{ArtworkStore, Database, DbError};

fn file(path: &str, format: AudioFormat, bitrate: u32, tags: &[(TagField, &str)]) -> ScannedFile {
    ScannedFile {
        path: path.to_owned(),
        size_bytes: u64::from(bitrate) * 30_000,
        modified_ns: 1_700_000_000_000_000_000,
        format,
        audio: Some(AudioFacts {
            duration_ms: 300_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: None,
            bitrate_kbps: Some(bitrate),
        }),
        tags: tags
            .iter()
            .map(|(field, value)| (*field, (*value).to_owned()))
            .collect(),
    }
}

/// Five tracks with gaps in different tags.
fn library() -> Database {
    let mut db = Database::open_in_memory().unwrap();
    let source = db.source("D:/Club").unwrap();
    db.record_scan(
        source,
        &[
            file(
                "House/one.flac",
                AudioFormat::Flac,
                900,
                &[
                    (TagField::Title, "One"),
                    (TagField::Artist, "Daft Punk"),
                    (TagField::Genre, "French House"),
                    (TagField::Label, "Virgin"),
                    (TagField::Bpm, "123"),
                    (TagField::Key, "Am"),
                    (TagField::Year, "2001"),
                    (TagField::TrackNumber, "9"),
                ],
            ),
            file(
                "House/two.mp3",
                AudioFormat::Mp3,
                320,
                &[
                    (TagField::Title, "Two"),
                    (TagField::Artist, "Bicep"),
                    (TagField::Genre, "Techno"),
                    (TagField::Bpm, "128"),
                    (TagField::Year, "2020"),
                    (TagField::TrackNumber, "10"),
                ],
            ),
            file(
                "Trance/three.mp3",
                AudioFormat::Mp3,
                192,
                &[
                    (TagField::Title, "Three"),
                    (TagField::Artist, "Tiësto"),
                    (TagField::Label, "Anjunadeep"),
                    (TagField::Bpm, "138"),
                ],
            ),
            file(
                "Trance/four_untitled.wav",
                AudioFormat::Wav,
                1411,
                &[(TagField::Genre, "Trance")],
            ),
            file(
                "Edits/five.aiff",
                AudioFormat::Aiff,
                1411,
                &[
                    (TagField::Title, "Five"),
                    (TagField::Genre, "techno"),
                    (TagField::Label, "Virgin"),
                ],
            ),
        ],
    )
    .unwrap();
    db
}

fn query(texts: &[&str]) -> TrackQuery {
    TrackQuery {
        filters: texts
            .iter()
            .map(|text| parse_command(text).unwrap())
            .collect(),
        ..TrackQuery::default()
    }
}

fn titles(db: &Database, query: &TrackQuery) -> Vec<String> {
    let rows: Vec<String> = db
        .query_tracks_page(query, 0, 50)
        .unwrap()
        .into_iter()
        .map(|row| row.file.title().to_owned())
        .collect();
    assert_eq!(
        db.query_track_count(query).unwrap() as usize,
        rows.len(),
        "the count agrees with the page"
    );
    rows
}

fn track_id(db: &Database, title: &str) -> i64 {
    db.tracks()
        .unwrap()
        .into_iter()
        .find(|row| row.file.title() == title)
        .unwrap()
        .track_id
}

fn shown(db: &Database, texts: &[&str]) -> Vec<String> {
    titles(db, &query(texts))
}

#[test]
fn text_conditions_match_ignoring_case_and_keep_untagged_tracks_on_is_not() {
    let db = library();
    assert_eq!(shown(&db, &["genre is techno"]), ["Five", "Two"]);
    assert_eq!(shown(&db, &["genre contains house"]), ["One"]);
    assert_eq!(shown(&db, &["artist starts with bi"]), ["Two"]);
    assert_eq!(
        shown(&db, &["genre not techno"]),
        ["One", "four_untitled", "Three"]
    );
    assert_eq!(
        shown(&db, &["genre is techno, trance"]),
        ["Five", "Two", "four_untitled"]
    );
    assert_eq!(
        shown(&db, &["label not virgin, anjunadeep"]),
        ["Two", "four_untitled"]
    );
    // Wildcards the user types are plain characters.
    assert!(shown(&db, &["artist contains %"]).is_empty());
    assert!(shown(&db, &["artist contains _"]).is_empty());
}

#[test]
fn missing_and_present_work_on_tags_titles_and_audio_facts() {
    let db = library();
    assert_eq!(shown(&db, &["missing genre"]), ["Three"]);
    assert_eq!(shown(&db, &["missing label"]), ["Two", "four_untitled"]);
    assert_eq!(shown(&db, &["missing title"]), ["four_untitled"]);
    assert_eq!(shown(&db, &["has title"]).len(), 4);
    assert_eq!(shown(&db, &["missing bit depth"]).len(), 5);
    assert_eq!(shown(&db, &["missing key"]).len(), 4);
    assert_eq!(shown(&db, &["has key"]), ["One"]);
    assert_eq!(shown(&db, &["missing track number"]).len(), 3);
    assert_eq!(shown(&db, &["has track number"]), ["One", "Two"]);
}

#[test]
fn number_choice_and_date_conditions() {
    let db = library();
    assert_eq!(shown(&db, &["bitrate < 320"]), ["Three"]);
    assert_eq!(shown(&db, &["bitrate <= 320"]), ["Two", "Three"]);
    assert_eq!(shown(&db, &["bpm 125-140"]), ["Two", "Three"]);
    assert_eq!(shown(&db, &["year >= 2010"]), ["Two"]);
    assert_eq!(shown(&db, &["time 5:00"]).len(), 5);
    assert_eq!(shown(&db, &["sample rate > 44.1k"]).len(), 0);
    assert_eq!(
        shown(&db, &["size > 20mb"]),
        ["Five", "One", "four_untitled"]
    );
    assert_eq!(shown(&db, &["format flac, wav"]), ["One", "four_untitled"]);
    assert_eq!(
        shown(&db, &["format not mp3"]),
        ["Five", "One", "four_untitled"]
    );
    assert_eq!(shown(&db, &["key 8A"]), ["One"]);
    assert_eq!(shown(&db, &["key not 8A"]).len(), 4);
    assert_eq!(shown(&db, &["modified before 2024-01-01"]).len(), 5);
    assert!(shown(&db, &["modified after 2024"]).is_empty());
    assert_eq!(shown(&db, &["added after 2020-01-01"]).len(), 5);
    assert_eq!(shown(&db, &["path contains trance/"]).len(), 2);
    assert_eq!(shown(&db, &["folder starts with house"]), ["One", "Two"]);
    assert_eq!(shown(&db, &["folder contains four"]).len(), 0);
}

#[test]
fn generic_tags_compare_the_way_they_sort() {
    let db = library();
    assert_eq!(shown(&db, &["track number is 9"]), ["One"]);
    assert_eq!(shown(&db, &["track number starts with 1"]), ["Two"]);
    assert_eq!(shown(&db, &["track number not 9"]).len(), 4);
}

#[test]
fn terms_combine_with_and_and_a_group_with_or() {
    let db = library();
    assert_eq!(shown(&db, &["genre is techno", "missing label"]), ["Two"]);
    let three = query(&["has genre", "bitrate > 320", "missing key"]);
    assert_eq!(titles(&db, &three), ["Five", "four_untitled"]);

    let either = TrackQuery {
        filters: vec![
            FilterTerm::any_of(vec![
                FieldFilter::missing(LibraryColumn::Genre).unwrap(),
                FieldFilter::missing(LibraryColumn::Label).unwrap(),
            ])
            .unwrap(),
        ],
        ..TrackQuery::default()
    };
    assert_eq!(titles(&db, &either), ["Two", "four_untitled", "Three"]);

    let counted = query(&["missing label", "format mp3"]);
    assert_eq!(db.query_filter_counts(&counted).unwrap(), [2, 2]);
    assert_eq!(db.query_track_count(&counted).unwrap(), 1);
}

#[test]
fn a_user_value_counts_as_the_tracks_value() {
    let mut db = library();
    let three = track_id(&db, "Three");
    db.set_user_tags(&[three], &[(TagField::Genre, Some("Trance".to_owned()))])
        .unwrap();
    assert!(shown(&db, &["missing genre"]).is_empty());
    db.set_user_tags(&[three], &[(TagField::Title, Some(String::new()))])
        .unwrap();
    assert_eq!(shown(&db, &["missing title"]).len(), 2);
}

#[test]
fn no_artwork_counts_the_users_picture() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("filters-artwork");
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0];
    jpeg.extend([7; 64]);
    let picture = ArtworkStore::new(&folder).put(&jpeg).unwrap();

    let mut db = library();
    assert_eq!(shown(&db, &["no artwork"]).len(), 5);
    let source = db.sources().unwrap()[0].id;
    db.record_scanned_artwork(
        source,
        &[("House/one.flac".to_owned(), Some(picture.clone()))],
    )
    .unwrap();
    let two = track_id(&db, "Two");
    db.set_user_artwork(two, &picture).unwrap();
    assert_eq!(shown(&db, &["has artwork"]), ["One", "Two"]);
    assert_eq!(shown(&db, &["no artwork"]).len(), 3);
}

#[test]
fn filters_never_read_file_tags() {
    let db = library();
    for text in [
        "missing genre",
        "genre contains house",
        "label is virgin",
        "bitrate < 320",
        "format flac",
        "track number is 9",
        "no artwork",
        "added after 2020",
    ] {
        let plan = db.explain_track_query(&query(&[text]), 0, 200).unwrap();
        assert!(
            !plan.iter().any(|step| step.contains("file_tag")),
            "{text}: {plan:?}"
        );
    }
}

#[test]
fn usage_quick_filters_and_the_setting_are_kept() {
    let mut db = library();
    let missing = parse_command("missing genre").unwrap();
    let flac = parse_command("format flac").unwrap();
    db.record_filter_use(&missing, 1_000).unwrap();
    db.record_filter_use(&missing, 2_000).unwrap();
    db.record_filter_use(&flac, 1_500).unwrap();
    let uses = db.filter_uses().unwrap();
    let of = |term: &FilterTerm| uses.iter().find(|used| used.term == *term).unwrap();
    assert_eq!((of(&missing).uses, of(&missing).last_used_ms), (2, 2_000));
    assert_eq!(of(&flac).uses, 1);

    let mut bar = db.quick_filter_bar().unwrap();
    assert_eq!(bar, QuickFilterBar::default());
    bar.apply(QuickFilterChange::Pin(flac.clone()), &[]);
    bar.apply(QuickFilterChange::SetRecommend(false), &[]);
    bar.dismissed.push(missing.key());
    db.save_quick_filter_bar(&bar).unwrap();
    assert_eq!(db.quick_filter_bar().unwrap(), bar);
}

#[test]
fn smart_views_keep_their_filters_search_and_pill() {
    let mut db = library();
    let terms = query(&["missing label", "bpm 120-130"]).filters;
    let view = db
        .create_smart_view(
            "  Needs a label ",
            Some(" techno "),
            FilterPill::Favourites,
            &terms,
        )
        .unwrap();
    assert_eq!(view.name, "Needs a label");
    assert_eq!(view.search.as_deref(), Some("techno"));
    assert!(matches!(
        db.create_smart_view("needs a LABEL", None, FilterPill::All, &terms),
        Err(DbError::DuplicateViewName { .. })
    ));
    let other = db
        .create_smart_view(
            "Lossless",
            None,
            FilterPill::All,
            &query(&["format flac"]).filters,
        )
        .unwrap();

    let views = db.smart_views().unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(views[0].terms, terms);
    assert_eq!(views[0].pill, FilterPill::Favourites);
    assert!(db.rename_smart_view(other.id, "Lossless only").unwrap());
    assert!(matches!(
        db.rename_smart_view(other.id, "Needs a label"),
        Err(DbError::DuplicateViewName { .. })
    ));
    assert!(db.delete_smart_view(view.id).unwrap());
    assert!(!db.delete_smart_view(view.id).unwrap());
    assert_eq!(db.smart_views().unwrap()[0].name, "Lossless only");
}

#[test]
fn a_condition_built_in_code_reads_the_same_as_its_command() {
    let db = library();
    let built = TrackQuery {
        filters: vec![FilterTerm::one(
            FieldFilter::new(LibraryColumn::Bitrate, Condition::Below(320.0)).unwrap(),
        )],
        ..TrackQuery::default()
    };
    assert_eq!(titles(&db, &built), shown(&db, &["bitrate < 320"]));
}
