#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_core::search::{SortDirection, TrackQuery, TrackSortField};
use resonance_core::staging::{Proposal, ProposalStatus, Provenance, staging_diff};
use resonance_db::{AcceptReport, Database};

fn scanned(path: &str, tags: &[(TagField, &str)]) -> ScannedFile {
    ScannedFile {
        path: path.to_owned(),
        size_bytes: 5_000_000,
        modified_ns: 1_700_000_000_000_000_000,
        format: AudioFormat::Flac,
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

fn proposal(field: TagField, value: &str, confidence: f64) -> Proposal {
    Proposal {
        field,
        value: value.to_owned(),
        provider: "musicbrainz".to_owned(),
        confidence,
        audio_matched: true,
    }
}

fn searched(db: &Database, text: &str) -> Vec<i64> {
    let query = TrackQuery {
        text: Some(text.to_owned()),
        ..TrackQuery::default()
    };
    db.query_tracks_page(&query, 0, 50)
        .unwrap()
        .into_iter()
        .map(|row| row.track_id)
        .collect()
}

fn titles_sorted(db: &Database) -> Vec<String> {
    let query = TrackQuery {
        sort: TrackSortField::Title,
        direction: SortDirection::Asc,
        ..TrackQuery::default()
    };
    db.query_tracks_page(&query, 0, 50)
        .unwrap()
        .into_iter()
        .map(|row| {
            row.user_tags
                .iter()
                .chain(&row.file.tags)
                .find(|(field, _)| *field == TagField::Title)
                .map(|(_, value)| value.clone())
                .unwrap()
        })
        .collect()
}

fn track_ids(db: &Database) -> Vec<i64> {
    db.tracks()
        .unwrap()
        .iter()
        .map(|row| row.track_id)
        .collect()
}

fn library(files: &[ScannedFile]) -> Database {
    let mut db = Database::open_in_memory().unwrap();
    let source = db.source("D:\\Club").unwrap();
    db.record_scan(source, files).unwrap();
    db
}

#[test]
fn an_accepted_proposal_changes_search_sort_and_the_table_before_any_file_write() {
    let files = [
        scanned(
            "a.flac",
            &[(TagField::Title, "Bassline"), (TagField::Artist, "Someone")],
        ),
        scanned("b.flac", &[(TagField::Title, "Midnight Drive")]),
    ];
    let mut db = library(&files);
    let ids = track_ids(&db);
    let first = ids[0];

    db.store_proposals(first, &[proposal(TagField::Title, "Zenith", 0.95)])
        .unwrap();
    assert!(searched(&db, "zenith").is_empty());
    let layers = db.tag_layers(first).unwrap().unwrap();
    let diff = staging_diff(&layers.file, &layers.user, &layers.proposals);
    let title = diff
        .iter()
        .find(|entry| entry.field == TagField::Title)
        .unwrap();
    assert_eq!(title.current_value.as_deref(), Some("Bassline"));
    assert_eq!(title.current_provenance, Some(Provenance::File));
    assert!(title.changes());

    assert_eq!(db.accept_proposals(first, &[TagField::Title]).unwrap(), 1);

    assert_eq!(searched(&db, "zenith"), [first]);
    assert!(searched(&db, "bassline").is_empty());
    assert_eq!(titles_sorted(&db), ["Midnight Drive", "Zenith"]);
    let row = db.tracks_page(0, 10).unwrap().remove(0);
    assert_eq!(row.user_tags, [(TagField::Title, "Zenith".to_owned())]);
    assert_eq!(db.staging_queue_count().unwrap(), 0);

    // A rescan replaces the file's tags but the user's value still shows.
    let source = db.source("D:\\Club").unwrap();
    let mut retagged = files[0].clone();
    retagged.tags = vec![(TagField::Title, "Bassline (Remaster)".to_owned())];
    db.record_scan(source, &[retagged]).unwrap();
    assert_eq!(searched(&db, "zenith"), [first]);
    assert!(searched(&db, "remaster").is_empty());

    // Handing the field back to the file shows the file's value again.
    db.set_user_tags(&[first], &[(TagField::Title, None)])
        .unwrap();
    assert_eq!(searched(&db, "remaster"), [first]);
    assert!(searched(&db, "zenith").is_empty());
}

#[test]
fn user_values_set_on_many_tracks_show_at_once_and_a_blank_one_hides_the_file_value() {
    let mut db = library(&[
        scanned("a.flac", &[(TagField::Comment, "promo copy")]),
        scanned("b.flac", &[(TagField::Comment, "promo copy")]),
    ]);
    let ids = track_ids(&db);

    let changed = db
        .set_user_tags(
            &ids,
            &[
                (TagField::Label, Some("Night Shift".to_owned())),
                (TagField::Comment, Some(String::new())),
            ],
        )
        .unwrap();

    assert_eq!(changed, 2);
    assert_eq!(searched(&db, "shift").len(), 2);
    assert!(searched(&db, "promo").is_empty());
    assert_eq!(db.library_facets().unwrap().labels, ["Night Shift"]);
    assert_eq!(
        db.set_user_tags(&[999], &[(TagField::Label, None)])
            .unwrap(),
        0
    );
}

#[test]
fn proposals_never_touch_user_values_and_keep_decisions_on_the_same_value() {
    let mut db = library(&[scanned(
        "a.flac",
        &[(TagField::Title, "Song"), (TagField::Artist, "Artist")],
    )]);
    let track = track_ids(&db)[0];
    db.set_user_tags(
        &[track],
        &[(TagField::Label, Some("Typed Label".to_owned()))],
    )
    .unwrap();

    let run = [
        proposal(TagField::Title, "Song", 0.97),
        proposal(TagField::Label, "Online Label", 0.97),
        proposal(TagField::Year, "2019", 0.97),
        proposal(TagField::CatNo, "DEF023", 0.97),
    ];
    db.store_proposals(track, &run).unwrap();
    db.skip_proposals(track, &[TagField::Year]).unwrap();
    db.store_proposals(track, &run).unwrap();

    let layers = db.tag_layers(track).unwrap().unwrap();
    assert_eq!(layers.user, [(TagField::Label, "Typed Label".to_owned())]);
    let status_of = |field| {
        layers
            .proposals
            .iter()
            .find(|stored| stored.proposal.field == field)
            .map(|stored| stored.status)
    };
    assert_eq!(
        status_of(TagField::Title),
        None,
        "matches what the track shows"
    );
    assert_eq!(status_of(TagField::Year), Some(ProposalStatus::Skipped));
    assert_eq!(status_of(TagField::Label), Some(ProposalStatus::Pending));
    let diff = staging_diff(&layers.file, &layers.user, &layers.proposals);
    assert!(
        diff.iter()
            .find(|entry| entry.field == TagField::Label)
            .unwrap()
            .locked_by_user
    );

    // A different value is a new proposal, so the earlier skip no longer holds.
    db.store_proposals(track, &[proposal(TagField::Year, "2020", 0.97)])
        .unwrap();
    let layers = db.tag_layers(track).unwrap().unwrap();
    assert_eq!(
        layers
            .proposals
            .iter()
            .map(|stored| (stored.proposal.field, stored.status))
            .collect::<Vec<_>>(),
        [(TagField::Year, ProposalStatus::Pending)]
    );
}

#[test]
fn accepting_above_a_threshold_skips_user_held_and_low_confidence_fields() {
    let mut db = library(&[
        scanned("a.flac", &[(TagField::Title, "One")]),
        scanned("b.flac", &[(TagField::Title, "Two")]),
        scanned("c.flac", &[(TagField::Title, "Three")]),
    ]);
    let ids = track_ids(&db);
    db.set_user_tags(&[ids[0]], &[(TagField::Artist, Some("Typed".to_owned()))])
        .unwrap();
    db.store_proposals(
        ids[0],
        &[
            proposal(TagField::Artist, "Online", 0.99),
            proposal(TagField::Label, "Label A", 0.99),
        ],
    )
    .unwrap();
    db.store_proposals(ids[1], &[proposal(TagField::Label, "Label B", 0.5)])
        .unwrap();
    db.store_proposals(
        ids[2],
        &[
            proposal(TagField::Label, "Label C", 0.92),
            proposal(TagField::Year, "2001", 0.6),
        ],
    )
    .unwrap();
    assert_eq!(db.staging_queue_count().unwrap(), 3);

    let report = db.accept_above_threshold(0.9).unwrap();

    assert_eq!(
        report,
        AcceptReport {
            tracks: 2,
            fields: 2
        }
    );
    let user_of = |db: &Database, track| db.tag_layers(track).unwrap().unwrap().user;
    assert_eq!(
        user_of(&db, ids[0]),
        [
            (TagField::Artist, "Typed".to_owned()),
            (TagField::Label, "Label A".to_owned()),
        ]
    );
    assert!(user_of(&db, ids[1]).is_empty());
    assert_eq!(
        user_of(&db, ids[2]),
        [(TagField::Label, "Label C".to_owned())]
    );
    let queue = db.staging_queue(0, 10).unwrap();
    assert_eq!(
        queue
            .iter()
            .map(|entry| (entry.track_id, entry.pending_fields))
            .collect::<Vec<_>>(),
        [(ids[0], 1), (ids[1], 1), (ids[2], 1)]
    );
}
