//! Checks the free providers' real answers still read as the fixtures assume. Each test asks a
//! public service over the network, so they run only when asked for:
//! `cargo test -p resonance-identify --test live -- --ignored`.

#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use resonance_core::identify::{ReleaseUrl, Step, StepOutcome};
use resonance_core::library::TagField;
use resonance_db::{Database, IdentificationInput};
use resonance_identify::credentials::MemoryCredentialStore;
use resonance_identify::http::NetworkClient;
use resonance_identify::pipeline::Pipeline;

fn pipeline() -> Pipeline {
    Pipeline::with_free_providers(
        Arc::new(NetworkClient::new()),
        Arc::new(Mutex::new(Database::open_in_memory().unwrap())),
        Arc::new(MemoryCredentialStore::new()),
    )
}

fn input(artist: &str, title: &str, duration_ms: u64) -> IdentificationInput {
    IdentificationInput {
        track_id: 1,
        path: format!("{artist} - {title}.flac"),
        duration_ms: Some(duration_ms),
        fingerprint: None,
        fingerprint_duration_s: None,
        file_isrc: None,
        bandcamp_sellers: Vec::new(),
        tags: vec![
            (TagField::Artist, artist.to_owned()),
            (TagField::Title, title.to_owned()),
        ],
    }
}

#[test]
#[ignore = "asks MusicBrainz over the network"]
fn musicbrainz_search_finds_a_well_known_recording() {
    let found = pipeline()
        .identify(
            &[input("Daft Punk", "Around the World", 429_000)],
            &AtomicBool::new(false),
        )
        .remove(0);

    let searched = found
        .steps
        .iter()
        .find(|report| report.step == Step::TextSearch && report.provider == Some("musicbrainz"))
        .unwrap();
    assert!(matches!(searched.outcome, StepOutcome::Ran { candidates, .. } if candidates > 0));
    let best = found.best.unwrap();
    assert_eq!(best.candidate.artist.as_deref(), Some("Daft Punk"));
    assert!(best.candidate.recording_id.is_some());
}

#[test]
#[ignore = "asks Discogs over the network"]
fn a_discogs_release_reads_without_a_token() {
    // Discogs release 1 is the first release in its database.
    let found = pipeline().identify_with_url(
        &input("The Persuader", "Unknown", 0),
        &ReleaseUrl::DiscogsRelease(1),
    );
    let best = found.best.unwrap();
    assert_eq!(best.candidate.artist.as_deref(), Some("The Persuader"));
    assert!(best.candidate.catno.is_some());
}
