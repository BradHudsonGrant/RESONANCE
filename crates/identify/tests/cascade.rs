//! The cascade against fixture providers: every answer comes from this file, nothing goes to
//! the network.

#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use resonance_core::identify::{
    Candidate, ReleaseUrl, STOP_THRESHOLD, Step, StepOutcome, USER_URL_CONFIDENCE,
};
use resonance_core::library::{AudioFacts, AudioFormat, ScannedFile, TagField};
use resonance_core::providers::ProviderKey;
use resonance_core::staging::{ProposalStatus, staging_diff};
use resonance_db::{Database, IdentificationInput};
use resonance_identify::audio::{AudioDecodeError, AudioFingerprinter};
use resonance_identify::credentials::{CredentialStore, MemoryCredentialStore};
use resonance_identify::http::{BytesResponse, HttpClient, HttpRequest, HttpResponse, Unreachable};
use resonance_identify::library::{
    identify_track_with_url, run_online_pass, select_candidate, use_candidate_artwork,
};
use resonance_identify::pipeline::{Identification, Pipeline};
use resonance_identify::provider::{IdentifyError, Lookup, MetadataProvider, Query, QueryKind};
use resonance_identify::rate::RateLimit;
use serde_json::{Value, json};

const RECORDING: &str = "b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d";
const FINGERPRINT: &str = "AQAAT0mUaEkSRZEGAA";

type Matcher = Box<dyn Fn(&HttpRequest) -> bool + Send + Sync>;

struct Route {
    url: String,
    matches: Matcher,
    status: u16,
    body: String,
    bytes: Option<Vec<u8>>,
}

#[derive(Default)]
struct Fixture {
    routes: Vec<Route>,
    calls: Mutex<Vec<(HttpRequest, Instant)>>,
}

impl Fixture {
    fn route(mut self, url: &str, body: Value) -> Self {
        self.routes.push(Route {
            url: url.to_owned(),
            matches: Box::new(|_| true),
            status: 200,
            body: body.to_string(),
            bytes: None,
        });
        self
    }

    fn route_bytes(mut self, url: &str, bytes: &[u8]) -> Self {
        self.routes.push(Route {
            url: url.to_owned(),
            matches: Box::new(|_| true),
            status: 200,
            body: String::new(),
            bytes: Some(bytes.to_vec()),
        });
        self
    }

    fn route_when(
        mut self,
        url: &str,
        matches: impl Fn(&HttpRequest) -> bool + Send + Sync + 'static,
        body: impl Into<String>,
    ) -> Self {
        self.routes.push(Route {
            url: url.to_owned(),
            matches: Box::new(matches),
            status: 200,
            body: body.into(),
            bytes: None,
        });
        self
    }

    fn calls_to(&self, url: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(request, _)| request.url.starts_with(url))
            .count()
    }
}

impl HttpClient for Fixture {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
        self.calls
            .lock()
            .unwrap()
            .push((request.clone(), Instant::now()));
        let route = self
            .routes
            .iter()
            .find(|route| request.url.starts_with(&route.url) && (route.matches)(request));
        Ok(match route {
            Some(route) => HttpResponse {
                status: route.status,
                body: route.body.clone(),
                retry_after_s: None,
            },
            None => HttpResponse {
                status: 404,
                body: String::new(),
                retry_after_s: None,
            },
        })
    }

    fn fetch_bytes(
        &self,
        request: &HttpRequest,
        _limit: u64,
    ) -> Result<BytesResponse, Unreachable> {
        let answer = self.send(request)?;
        let bytes = self
            .routes
            .iter()
            .find(|route| request.url.starts_with(&route.url))
            .and_then(|route| route.bytes.clone());
        Ok(BytesResponse {
            status: answer.status,
            body: bytes.unwrap_or_else(|| answer.body.into_bytes()),
        })
    }
}

fn param<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .params
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn setup(fixture: Fixture, keys: &[ProviderKey]) -> (Arc<Fixture>, Pipeline, Arc<Mutex<Database>>) {
    let fixture = Arc::new(fixture);
    let database = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    let credentials = MemoryCredentialStore::new();
    for key in keys {
        credentials.write(*key, "fixture-key").unwrap();
    }
    let pipeline = Pipeline::with_free_providers(
        Arc::clone(&fixture) as Arc<dyn HttpClient>,
        Arc::clone(&database),
        Arc::new(credentials),
    );
    (fixture, pipeline, database)
}

fn input(track_id: i64, duration_ms: u64, fingerprint: bool) -> IdentificationInput {
    IdentificationInput {
        track_id,
        path: "Downloads/01 Some Artist - Night Drive.flac".to_owned(),
        duration_ms: Some(duration_ms),
        fingerprint: fingerprint.then(|| FINGERPRINT.to_owned()),
        fingerprint_duration_s: fingerprint.then_some((duration_ms / 1000) as u32),
        file_isrc: None,
        bandcamp_sellers: Vec::new(),
        tags: vec![
            (TagField::Artist, "Some Artist".to_owned()),
            (TagField::Title, "Night Drive".to_owned()),
        ],
    }
}

fn outcomes(
    identification: &Identification,
    step: Step,
) -> Vec<(Option<&'static str>, StepOutcome)> {
    identification
        .steps
        .iter()
        .filter(|report| report.step == step)
        .map(|report| (report.provider, report.outcome.clone()))
        .collect()
}

fn acoustid_answer(score: f64, recording: Value) -> Value {
    json!({ "status": "ok", "results": [{ "id": "a1", "score": score, "recordings": [recording] }] })
}

fn releases_of_recording() -> Value {
    json!({ "releases": [{
        "id": "c0a1b2c3-0000-4000-8000-000000000001",
        "title": "Night Drive EP",
        "date": "2019-05-03",
        "label-info": [{ "catalog-number": "DEF023", "label": { "name": "Some Label" } }]
    }]})
}

fn discogs_release() -> Value {
    json!({
        "id": 111,
        "title": "Night Drive",
        "year": 2019,
        "artists": [{ "name": "Some Artist (3)", "join": "" }],
        "labels": [{ "name": "Some Label (2)", "catno": "DEF023" }],
        "tracklist": [
            { "position": "A1", "type_": "track", "title": "Night Drive (Radio Edit)", "duration": "3:21" },
            { "position": "A2", "type_": "track", "title": "Night Drive (Extended Mix)", "duration": "6:52" }
        ]
    })
}

fn never() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn a_confident_fingerprint_stops_the_cascade_before_text_search() {
    let fixture = Fixture::default()
        .route(
            "https://api.acoustid.org/v2/lookup",
            acoustid_answer(
                0.97,
                json!({ "id": RECORDING, "title": "Night Drive", "duration": 412,
                        "artists": [{ "name": "Some Artist" }] }),
            ),
        )
        .route(
            "https://musicbrainz.org/ws/2/release",
            releases_of_recording(),
        );
    let (fixture, pipeline, _) = setup(fixture, &[ProviderKey::AcoustId, ProviderKey::Discogs]);

    let found = pipeline
        .identify(&[input(1, 412_300, true)], &never())
        .remove(0);

    let best = found.best.as_ref().unwrap();
    assert!(best.confidence >= STOP_THRESHOLD);
    assert!(best.audio_matched);
    assert_eq!(best.candidate.catno.as_deref(), Some("DEF023"));
    assert_eq!(best.candidate.label.as_deref(), Some("Some Label"));
    assert_eq!(best.candidate.year, Some(2019));
    assert_eq!(
        outcomes(&found, Step::ExistingTags),
        vec![(None, StepOutcome::Read { fields: 2 })]
    );
    assert!(matches!(
        outcomes(&found, Step::FileName)[0].1,
        StepOutcome::Read { fields: 3 }
    ));
    for step in [Step::TextSearch, Step::AudioConfirmation, Step::UserUrl] {
        assert!(
            outcomes(&found, step)
                .iter()
                .all(|(_, outcome)| *outcome == StepOutcome::SkippedConfident),
            "{step:?}"
        );
    }
    assert_eq!(
        fixture.calls_to("https://musicbrainz.org/ws/2/recording"),
        0
    );
    assert_eq!(fixture.calls_to("https://api.discogs.com"), 0);
    let lookup = &fixture.calls.lock().unwrap()[0].0;
    assert_eq!(param(lookup, "duration"), Some("412"));
    assert_eq!(param(lookup, "fingerprint"), Some(FINGERPRINT));
    assert!(found.is_complete());
}

#[test]
fn steps_without_their_key_are_skipped_and_reported_not_failed() {
    let fixture = Fixture::default().route(
        "https://musicbrainz.org/ws/2/recording",
        json!({ "recordings": [] }),
    );
    let (fixture, pipeline, _) = setup(fixture, &[]);

    let found = pipeline
        .identify(&[input(1, 300_000, true)], &never())
        .remove(0);

    assert_eq!(
        outcomes(&found, Step::Fingerprint),
        vec![(
            Some("acoustid"),
            StepOutcome::SkippedMissingKey(ProviderKey::AcoustId)
        )]
    );
    assert_eq!(
        outcomes(&found, Step::TextSearch),
        vec![
            (
                Some("deezer"),
                StepOutcome::Ran {
                    candidates: 0,
                    best: None
                }
            ),
            (
                Some("musicbrainz"),
                StepOutcome::Ran {
                    candidates: 0,
                    best: None
                }
            ),
            (
                Some("discogs"),
                StepOutcome::SkippedMissingKey(ProviderKey::Discogs)
            ),
            (
                Some("itunes"),
                StepOutcome::Ran {
                    candidates: 0,
                    best: None
                }
            ),
            (
                Some("bandcamp"),
                StepOutcome::Ran {
                    candidates: 0,
                    best: None
                }
            ),
        ]
    );
    assert_eq!(
        outcomes(&found, Step::UserUrl),
        vec![(None, StepOutcome::NeedsUrl)]
    );
    assert!(!found.is_complete());
    assert!(found.needs_url());
    assert_eq!(fixture.calls_to("https://api.acoustid.org"), 0);
    assert_eq!(fixture.calls_to("https://api.discogs.com"), 0);
}

#[test]
fn text_search_tells_the_radio_edit_from_the_extended_mix() {
    let fixture = Fixture::default()
        .route(
            "https://musicbrainz.org/ws/2/recording",
            json!({ "recordings": [] }),
        )
        .route_when(
            "https://api.discogs.com/database/search",
            |request| {
                param(request, "artist") == Some("Some Artist")
                    && param(request, "track") == Some("Night Drive")
            },
            json!({ "results": [{ "id": 111 }] }).to_string(),
        )
        .route("https://api.discogs.com/releases/111", discogs_release());
    let (fixture, pipeline, _) = setup(fixture, &[ProviderKey::Discogs]);

    let found = pipeline.identify(
        &[input(1, 412_000, false), input(2, 201_000, false)],
        &never(),
    );

    let extended = found[0].best.as_ref().unwrap();
    assert_eq!(extended.candidate.mix.as_deref(), Some("Extended Mix"));
    assert_eq!(extended.candidate.artist.as_deref(), Some("Some Artist"));
    assert_eq!(extended.candidate.label.as_deref(), Some("Some Label"));
    assert!(extended.confidence >= STOP_THRESHOLD);
    assert!(!extended.audio_matched);

    let radio = found[1].best.as_ref().unwrap();
    assert_eq!(radio.candidate.mix.as_deref(), Some("Radio Edit"));
    assert!(radio.confidence >= 0.80 && radio.confidence < STOP_THRESHOLD);

    // The second track asked the same questions, so the cache answered them.
    assert_eq!(
        fixture.calls_to("https://api.discogs.com/database/search"),
        1
    );
    assert_eq!(fixture.calls_to("https://api.discogs.com/releases/111"), 1);
    let authorised = fixture.calls.lock().unwrap().iter().any(|(request, _)| {
        request.url.starts_with("https://api.discogs.com") && request.secret.is_some()
    });
    assert!(authorised);
}

#[test]
fn audio_confirmation_marks_a_text_result_whose_recording_the_fingerprint_found() {
    let fixture = Fixture::default()
        .route(
            "https://api.acoustid.org/v2/lookup",
            acoustid_answer(0.9, json!({ "id": RECORDING })),
        )
        .route(
            "https://musicbrainz.org/ws/2/release",
            json!({ "releases": [] }),
        )
        .route(
            "https://musicbrainz.org/ws/2/recording",
            json!({ "recordings": [{
                "id": RECORDING, "score": 100, "title": "Night Drive", "length": 300_000,
                "artist-credit": [{ "name": "Some Artist", "joinphrase": "" }]
            }]}),
        );
    let (_, pipeline, _) = setup(fixture, &[ProviderKey::AcoustId]);

    let found = pipeline
        .identify(&[input(1, 300_400, true)], &never())
        .remove(0);

    let fingerprint = outcomes(&found, Step::Fingerprint);
    assert!(matches!(
        fingerprint[0].1,
        StepOutcome::Ran { candidates: 1, best: Some(best) } if best < STOP_THRESHOLD
    ));
    assert_eq!(
        outcomes(&found, Step::AudioConfirmation),
        vec![(
            None,
            StepOutcome::Ran {
                candidates: 1,
                best: Some(1.0)
            }
        )]
    );
    let best = found.best.as_ref().unwrap();
    assert!(best.audio_matched);
    assert_eq!(best.candidate.provider, "musicbrainz");
    assert_eq!(
        outcomes(&found, Step::TextSearch)[2],
        (Some("discogs"), StepOutcome::SkippedConfident)
    );
    assert_eq!(
        outcomes(&found, Step::TextSearch)[3],
        (Some("itunes"), StepOutcome::SkippedConfident)
    );
}

#[test]
fn weak_fingerprints_and_disabled_previews_never_count_as_audio_confirmations() {
    let fixture = Fixture::default()
        .route(
            "https://api.acoustid.org/v2/lookup",
            acoustid_answer(0.4, json!({ "id": RECORDING })),
        )
        .route(
            "https://musicbrainz.org/ws/2/recording",
            json!({ "recordings": [{
                "id": RECORDING, "score": 100, "title": "Night Drive", "length": 300_000,
                "artist-credit": [{ "name": "Some Artist" }]
            }]}),
        );
    let (_, pipeline, _) = setup(fixture, &[ProviderKey::AcoustId]);
    let weak = pipeline
        .identify(&[input(1, 300_000, true)], &never())
        .remove(0);
    assert!(
        outcomes(&weak, Step::AudioConfirmation)
            .iter()
            .any(|(id, outcome)| id.is_none()
                && matches!(outcome, StepOutcome::Ran { candidates: 0, .. }))
    );
    assert!(!weak.best.unwrap().audio_matched);

    let fixture = Fixture::default().route(
        "https://api.deezer.com/search/track",
        json!({
            "data":[{ "id":42,"title":"Night Drive","duration":300,"artist":{"name":"Some Artist"},
                      "preview":"https://example.test/preview.mp3" }]
        }),
    );
    let (fixture, pipeline, _) = setup(fixture, &[]);
    let disabled = pipeline
        .with_verify_previews(false)
        .identify(&[input(1, 300_000, false)], &never())
        .remove(0);
    assert!(
        disabled
            .candidates
            .iter()
            .any(|c| c.candidate.preview_url.is_some())
    );
    assert!(
        outcomes(&disabled, Step::AudioConfirmation).contains(&(None, StepOutcome::SkippedNoInput))
    );
    assert_eq!(fixture.calls_to("https://example.test/preview.mp3"), 0);
}

#[test]
fn a_pasted_discogs_release_or_bandcamp_page_is_a_confident_source() {
    let page = format!(
        "<html><head><script type=\"application/ld+json\">{}</script></head></html>",
        json!({
            "@type": "MusicAlbum", "name": "Night Drive EP",
            "byArtist": { "name": "Some Artist" }, "publisher": { "name": "Some Label" },
            "datePublished": "03 May 2019 00:00:00 GMT",
            "track": { "itemListElement": [
                { "position": 1, "item": { "name": "Night Drive", "duration": "P00H06M52S" } },
                { "position": 2, "item": { "name": "Other Tune", "duration": "P00H05M10S" } }
            ]}
        })
    );
    let fixture = Fixture::default()
        .route("https://api.discogs.com/releases/111", discogs_release())
        .route_when(
            "https://someone.bandcamp.com/album/night-drive",
            |_| true,
            page,
        );
    let (_, pipeline, _) = setup(fixture, &[]);

    let discogs =
        pipeline.identify_with_url(&input(1, 412_000, false), &ReleaseUrl::DiscogsRelease(111));
    let best = discogs.best.as_ref().unwrap();
    assert!(best.candidate.user_supplied);
    assert!(best.confidence >= USER_URL_CONFIDENCE);
    assert_eq!(best.candidate.mix.as_deref(), Some("Extended Mix"));

    let bandcamp = pipeline.identify_with_url(
        &input(1, 412_000, false),
        &ReleaseUrl::Bandcamp("https://someone.bandcamp.com/album/night-drive".to_owned()),
    );
    let best = bandcamp.best.as_ref().unwrap();
    assert_eq!(best.candidate.title.as_deref(), Some("Night Drive"));
    assert_eq!(best.candidate.album.as_deref(), Some("Night Drive EP"));
    assert_eq!(best.candidate.label.as_deref(), Some("Some Label"));
    assert!(best.confidence >= USER_URL_CONFIDENCE);
    assert_eq!(
        outcomes(&bandcamp, Step::UserUrl),
        vec![(
            Some("bandcamp"),
            StepOutcome::Ran {
                candidates: 2,
                best: Some(best.confidence)
            }
        )]
    );
}

#[test]
fn a_second_run_is_answered_from_the_cache() {
    let fixture = Fixture::default()
        .route(
            "https://api.acoustid.org/v2/lookup",
            acoustid_answer(
                0.97,
                json!({ "id": RECORDING, "title": "Night Drive", "duration": 412,
                        "artists": [{ "name": "Some Artist" }] }),
            ),
        )
        .route(
            "https://musicbrainz.org/ws/2/release",
            releases_of_recording(),
        );
    let (fixture, pipeline, database) = setup(fixture, &[ProviderKey::AcoustId]);

    let first = pipeline.identify(&[input(1, 412_300, true)], &never());
    let after_first = pipeline.fetch_counts();
    let second = pipeline.identify(&[input(1, 412_300, true)], &never());
    let after_second = pipeline.fetch_counts();

    assert_eq!(first, second);
    assert_eq!(after_first.network, 2);
    assert_eq!(after_second.network, 2);
    assert_eq!(after_second.cached - after_first.cached, 2);
    assert_eq!(fixture.calls.lock().unwrap().len(), 2);

    let stale = database
        .lock()
        .unwrap()
        .purge_stale_responses(i64::MAX)
        .unwrap();
    assert_eq!(stale, 2);
}

struct Paced(RateLimit);

impl MetadataProvider for Paced {
    fn id(&self) -> &'static str {
        "paced"
    }
    fn answers(&self, kind: QueryKind) -> bool {
        kind == QueryKind::Text
    }
    fn requires_key(&self, _kind: QueryKind) -> Option<ProviderKey> {
        None
    }
    fn key(&self) -> Option<ProviderKey> {
        None
    }
    fn rate_limit(&self) -> RateLimit {
        self.0
    }
    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let Query::Text(known) = query else {
            return Ok(Vec::new());
        };
        let title = known.title.clone().unwrap_or_default();
        lookup.fetch(&HttpRequest::get("https://paced.test/").param("q", title))?;
        Ok(Vec::new())
    }
}

#[test]
fn requests_to_a_provider_are_paced_to_its_declared_limit() {
    let fixture = Arc::new(Fixture::default().route("https://paced.test/", json!({})));
    let database = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    let pipeline = Pipeline::new(
        vec![Box::new(Paced(RateLimit::per_second(10)))],
        Arc::clone(&fixture) as Arc<dyn HttpClient>,
        database,
        Arc::new(MemoryCredentialStore::new()),
    );
    let inputs: Vec<_> = (0..4)
        .map(|index| {
            let mut track = input(index, 300_000, false);
            track.tags[1].1 = format!("Tune {index}");
            track
        })
        .collect();

    pipeline.identify(&inputs, &never());

    let times: Vec<Instant> = fixture
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(times.len(), 4);
    for pair in times.windows(2) {
        assert!(
            pair[1] - pair[0] >= Duration::from_millis(90),
            "{:?}",
            pair[1] - pair[0]
        );
    }
    assert!(times[3] - times[0] >= Duration::from_millis(290));
}

fn scanned(index: usize) -> ScannedFile {
    ScannedFile {
        path: format!("Some Artist - Tune {index}.flac"),
        size_bytes: 4_000_000,
        modified_ns: 1_700_000_000_000_000_000,
        format: AudioFormat::Flac,
        audio: Some(AudioFacts {
            duration_ms: 300_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: Some(16),
            bitrate_kbps: Some(900),
        }),
        tags: vec![
            (TagField::Title, format!("Tune {index}")),
            (TagField::Artist, "Some Artist".to_owned()),
        ],
    }
}

fn searched_title(request: &HttpRequest) -> Option<String> {
    param(request, "q").map(str::to_owned)
}

#[test]
fn the_online_pass_puts_the_selection_first_and_resumes_after_a_stop() {
    let fixture = Arc::new(Fixture::default().route("https://paced.test/", json!({})));
    let database = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    let pipeline = Pipeline::new(
        vec![Box::new(Paced(RateLimit::per_second(1000)))],
        Arc::clone(&fixture) as Arc<dyn HttpClient>,
        Arc::clone(&database),
        Arc::new(MemoryCredentialStore::new()),
    );
    {
        let mut database = database.lock().unwrap();
        let source = database.source("library").unwrap();
        let files: Vec<_> = (1..=12).map(scanned).collect();
        database.record_scan(source, &files).unwrap();
    }
    let selected = {
        let database = database.lock().unwrap();
        database
            .identification_inputs(resonance_db::IdentificationPass::Local, 0, 100)
            .unwrap()
            .into_iter()
            .find(|input| input.path.ends_with("Tune 12.flac"))
            .unwrap()
            .track_id
    };

    let stop = AtomicBool::new(false);
    let first = run_online_pass(&database, &pipeline, &[selected], &stop, |done, _| {
        if done > 0 {
            stop.store(true, Ordering::Release);
        }
    })
    .unwrap();

    let first_search = fixture
        .calls
        .lock()
        .unwrap()
        .iter()
        .find_map(|(request, _)| searched_title(request));
    assert_eq!(first_search.as_deref(), Some("Tune 12"));
    assert_eq!(first.looked_up, 4);
    assert_eq!(first.needs_url, 4);
    assert_eq!(first.incomplete, 0);
    let stored = database
        .lock()
        .unwrap()
        .identification(selected)
        .unwrap()
        .unwrap();
    assert!(stored.online);
    assert!(stored.steps_json.contains("\"needs_url\""));

    let again = AtomicBool::new(false);
    let second = run_online_pass(&database, &pipeline, &[], &again, |_, _| {}).unwrap();
    assert_eq!(second.looked_up, 8);
    let third = run_online_pass(&database, &pipeline, &[], &again, |_, _| {}).unwrap();
    assert_eq!(third.looked_up, 0);

    let pasted = identify_track_with_url(
        &database,
        &pipeline,
        selected,
        &ReleaseUrl::DiscogsRelease(999),
    )
    .unwrap()
    .unwrap();
    assert!(pasted.best.is_none());
    assert!(
        database
            .lock()
            .unwrap()
            .identification(selected)
            .unwrap()
            .unwrap()
            .online
    );
}

#[test]
fn user_values_survive_a_full_rerun_and_conflicting_proposals_show_as_locked() {
    let fixture = Fixture::default()
        .route(
            "https://musicbrainz.org/ws/2/recording",
            json!({ "recordings": [] }),
        )
        .route(
            "https://api.discogs.com/database/search",
            json!({ "results": [{ "id": 111 }] }),
        )
        .route("https://api.discogs.com/releases/111", discogs_release());
    // Without an AcoustID key every run is incomplete, so the next pass runs all six steps
    // again for the track.
    let (_, pipeline, database) = setup(fixture, &[ProviderKey::Discogs]);
    let track = {
        let mut database = database.lock().unwrap();
        let source = database.source("library").unwrap();
        let mut file = scanned(1);
        file.path = "Some Artist - Night Drive.flac".to_owned();
        file.audio = file.audio.map(|audio| AudioFacts {
            duration_ms: 412_000,
            ..audio
        });
        file.tags = vec![
            (TagField::Artist, "Some Artist".to_owned()),
            (TagField::Title, "Night Drive".to_owned()),
        ];
        database.record_scan(source, &[file]).unwrap();
        let identity = resonance_core::identity::AssetIdentity {
            byte_hash: resonance_core::identity::ContentHash::from_bytes([9; 32]),
            audio: Some(resonance_core::identity::AudioHash {
                hash: resonance_core::identity::ContentHash::from_bytes([10; 32]),
                decoder: "symphonia/0.6.1".to_owned(),
            }),
            chromaprint: Some(resonance_core::identity::Chromaprint {
                encoded: FINGERPRINT.to_owned(),
                raw: vec![1, 2, 3, 4],
                duration_s: 412,
            }),
            decoded_facts: None,
        };
        database
            .record_asset_identity(
                1,
                resonance_db::FileStamp {
                    size_bytes: 4_000_000,
                    modified_ns: 1_700_000_000_000_000_000,
                },
                &identity,
            )
            .unwrap();
        database.tracks().unwrap()[0].track_id
    };

    let first = run_online_pass(&database, &pipeline, &[], &never(), |_, _| {}).unwrap();
    assert_eq!(first.looked_up, 1);
    assert_eq!(first.incomplete, 1);
    {
        let mut database = database.lock().unwrap();
        let layers = database.tag_layers(track).unwrap().unwrap();
        let mut proposed: Vec<_> = layers
            .proposals
            .iter()
            .map(|stored| (stored.proposal.field, stored.proposal.value.as_str()))
            .collect();
        proposed.sort();
        assert_eq!(
            proposed,
            [
                (TagField::Title, "Night Drive (Extended Mix)"),
                (TagField::Album, "Night Drive"),
                (TagField::Year, "2019"),
                (TagField::TrackNumber, "A2"),
                (TagField::Label, "Some Label"),
                (TagField::CatNo, "DEF023"),
            ]
        );
        database
            .set_user_tags(
                &[track],
                &[(TagField::Label, Some("Typed Label".to_owned()))],
            )
            .unwrap();
        database
            .accept_proposals(track, &[TagField::Title])
            .unwrap();
    }

    let second = run_online_pass(&database, &pipeline, &[], &never(), |_, _| {}).unwrap();

    assert_eq!(second.looked_up, 1);
    let mut database = database.lock().unwrap();
    let layers = database.tag_layers(track).unwrap().unwrap();
    assert_eq!(layers.user.len(), 2);
    assert!(
        layers
            .user
            .contains(&(TagField::Label, "Typed Label".to_owned()))
    );
    assert!(
        layers
            .user
            .contains(&(TagField::Title, "Night Drive (Extended Mix)".to_owned()))
    );
    let diff = staging_diff(&layers.file, &layers.user, &layers.proposals);
    let label = diff
        .iter()
        .find(|entry| entry.field == TagField::Label)
        .unwrap();
    assert!(label.locked_by_user);
    assert_eq!(label.current_value.as_deref(), Some("Typed Label"));
    assert_eq!(
        label.proposal.as_ref().map(|stored| stored.status),
        Some(ProposalStatus::Pending)
    );

    let accepted = database.accept_above_threshold(STOP_THRESHOLD).unwrap();
    assert_eq!(
        accepted.fields, 4,
        "album, year, catalogue number and track number"
    );
    let user = database.tag_layers(track).unwrap().unwrap().user;
    assert!(user.contains(&(TagField::Label, "Typed Label".to_owned())));
}

const COVER: [u8; 8] = [0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F'];

#[test]
fn a_musicbrainz_release_with_a_front_cover_names_its_cover_art_archive_picture() {
    let release = |front: bool| {
        json!({ "releases": [{
            "id": "c0a1b2c3-0000-4000-8000-000000000001",
            "title": "Night Drive EP",
            "cover-art-archive": { "artwork": front, "front": front, "count": 1 }
        }]})
    };
    let recording = json!({ "id": RECORDING, "title": "Night Drive", "duration": 412,
                            "artists": [{ "name": "Some Artist" }] });
    for front in [true, false] {
        let fixture = Fixture::default()
            .route(
                "https://api.acoustid.org/v2/lookup",
                acoustid_answer(0.97, recording.clone()),
            )
            .route("https://musicbrainz.org/ws/2/release", release(front));
        let (_, pipeline, _) = setup(fixture, &[ProviderKey::AcoustId]);

        let found = pipeline
            .identify(&[input(1, 412_300, true)], &never())
            .remove(0);

        assert_eq!(
            found.best.unwrap().candidate.artwork_url.as_deref(),
            front.then_some(
                "https://coverartarchive.org/release/c0a1b2c3-0000-4000-8000-000000000001/front-500"
            )
        );
    }
}

#[test]
fn a_release_cover_is_fetched_once_and_staged_for_tracks_without_a_picture() {
    let image_url = "https://i.discogs.com/night-drive.jpg";
    let mut release = discogs_release();
    release["images"] = json!([
        { "type": "secondary", "uri": "https://i.discogs.com/back.jpg" },
        { "type": "primary", "uri": image_url }
    ]);
    let fixture = Fixture::default()
        .route("https://api.discogs.com/releases/111", release)
        .route_bytes(image_url, &COVER);
    let (fixture, pipeline, database) = setup(fixture, &[ProviderKey::Discogs]);
    let folder = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cascade-artwork");
    let _ = std::fs::remove_dir_all(&folder);
    let (tracks, held) = {
        let mut database = database.lock().unwrap();
        database.set_artwork_folder(&folder);
        let source = database.source("library").unwrap();
        database
            .record_scan(source, &[scanned(1), scanned(2), scanned(3)])
            .unwrap();
        let tracks: Vec<i64> = database
            .tracks()
            .unwrap()
            .iter()
            .map(|track| track.track_id)
            .collect();
        let held = database
            .artwork_store()
            .unwrap()
            .put(&[0xff, 0xd8, 0xff, 0xe0, 9])
            .unwrap();
        database.set_user_artwork(tracks[2], &held).unwrap();
        (tracks, held)
    };

    for track_id in &tracks {
        identify_track_with_url(
            &database,
            &pipeline,
            *track_id,
            &ReleaseUrl::DiscogsRelease(111),
        )
        .unwrap();
    }

    assert_eq!(fixture.calls_to(image_url), 1);
    assert_eq!(fixture.calls_to("https://i.discogs.com/back.jpg"), 0);
    let database = database.lock().unwrap();
    let staged = database.proposed_artwork(tracks[0]).unwrap().unwrap();
    assert!(staged.pending);
    assert_eq!(staged.provider, "discogs");
    assert_eq!(staged.source_url, image_url);
    assert_eq!(
        database
            .artwork_store()
            .unwrap()
            .read(&staged.artwork)
            .unwrap(),
        COVER
    );
    assert_eq!(
        database
            .proposed_artwork(tracks[1])
            .unwrap()
            .unwrap()
            .artwork,
        staged.artwork
    );
    assert_eq!(
        database.proposed_artwork(tracks[2]).unwrap(),
        None,
        "a track that shows a picture is left alone"
    );
    assert_eq!(database.track_artwork(tracks[2]).unwrap(), Some(held));
    assert_eq!(
        database.track_artwork(tracks[0]).unwrap(),
        None,
        "a proposal is not shown until it is accepted"
    );
}

#[test]
fn deezer_isrc_bridge_gets_musicbrainz_caa_cover_and_excludes_deezer_art() {
    let fixture = Fixture::default()
        .route_when(
            "https://api.deezer.com/search/track",
            |_| true,
            json!({ "data": [{
                "id": 9001,
                "title": "Night Drive (Extended Mix)",
                "duration": 412,
                "artist": { "name": "Some Artist" },
                "album": { "id": 77, "title": "Night Drive EP", "cover_xl": "https://cdn-images.dzcdn.net/cover.jpg" },
                "preview": "https://cdns-preview.dzcdn.net/p.mp3"
            }]})
            .to_string(),
        )
        .route(
            "https://api.deezer.com/track/9001",
            json!({
                "id": 9001,
                "title": "Night Drive (Extended Mix)",
                "duration": 412,
                "isrc": "GBXYZ2100099",
                "release_date": "2021-05-01",
                "track_position": 2,
                "artist": { "name": "Some Artist" },
                "album": { "id": 77, "title": "Night Drive EP" },
                "link": "https://www.deezer.com/track/9001"
            }),
        )
        .route(
            "https://api.deezer.com/album/77",
            json!({
                "id": 77,
                "title": "Night Drive EP",
                "label": "Defected",
                "release_date": "2021-05-01",
                "record_type": "ep",
                "genres": { "data": [{ "name": "House" }] }
            }),
        )
        .route(
            "https://musicbrainz.org/ws/2/isrc/GBXYZ2100099",
            json!({ "recordings": [{
                "id": RECORDING,
                "title": "Night Drive (Extended Mix)",
                "length": 412_000,
                "artist-credit": [{ "name": "Some Artist" }],
                "releases": [{
                    "id": "rel-caa-1",
                    "title": "Night Drive EP",
                    "status": "Official",
                    "date": "2021-05-01",
                    "cover-art-archive": { "front": true },
                    "label-info": [{ "catalog-number": "DFTD099", "label": { "name": "Defected" } }],
                    "release-group": { "primary-type": "EP" }
                }]
            }]}),
        );
    let (fixture, pipeline, _) = setup(fixture, &[]);

    let found = pipeline
        .identify(&[input(1, 412_000, false)], &never())
        .remove(0);

    let best = found.best.as_ref().unwrap();
    assert!(best.confidence >= STOP_THRESHOLD);
    assert_eq!(best.candidate.isrc.as_deref(), Some("GBXYZ2100099"));
    assert_eq!(
        best.candidate.artwork_provider.as_deref(),
        Some("musicbrainz")
    );
    assert_eq!(
        best.candidate.artwork_url.as_deref(),
        Some("https://coverartarchive.org/release/rel-caa-1/front-500")
    );
    assert_eq!(fixture.calls_to("https://itunes.apple.com"), 0);
    assert!(found.is_complete());
}

#[test]
fn deezer_quota_error_is_retried_and_never_cached() {
    let attempts = Arc::new(Mutex::new(0u32));
    struct QuotaHttp {
        attempts: Arc<Mutex<u32>>,
    }
    impl HttpClient for QuotaHttp {
        fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            let mut count = self.attempts.lock().unwrap();
            *count += 1;
            Ok(HttpResponse {
                status: 200,
                body: json!({ "error": { "type": "Exception", "message": "Quota limit exceeded", "code": 4 } }).to_string(),
                retry_after_s: Some(1),
            })
        }
        fn fetch_bytes(
            &self,
            _request: &HttpRequest,
            _limit: u64,
        ) -> Result<BytesResponse, Unreachable> {
            Ok(BytesResponse {
                status: 404,
                body: Vec::new(),
            })
        }
    }

    let attempts_clone = Arc::clone(&attempts);
    let database = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    let pipeline = Pipeline::new(
        vec![Box::new(resonance_identify::providers::Deezer)],
        Arc::new(QuotaHttp { attempts }),
        Arc::clone(&database),
        Arc::new(MemoryCredentialStore::new()),
    );

    let result = pipeline
        .identify(&[input(1, 300_000, false)], &never())
        .remove(0);
    assert!(!result.is_complete());
    assert!(*attempts_clone.lock().unwrap() >= 2);
    let cached_rows: i64 = database
        .lock()
        .unwrap()
        .purge_stale_responses(i64::MAX)
        .unwrap() as i64;
    assert_eq!(cached_rows, 0, "Deezer quota errors must never be cached");
}

struct MockFingerprinter {
    track_full: Vec<u32>,
    preview: Vec<u32>,
    excerpt_wav: Vec<u8>,
}

impl AudioFingerprinter for MockFingerprinter {
    fn fingerprint_bytes(
        &self,
        _bytes: &[u8],
        _ext_hint: Option<&str>,
    ) -> Result<Vec<u32>, AudioDecodeError> {
        Ok(self.preview.clone())
    }
    fn fingerprint_file_full(&self, _path: &Path) -> Result<Vec<u32>, AudioDecodeError> {
        Ok(self.track_full.clone())
    }
    fn extract_wav_excerpt(
        &self,
        _path: &Path,
        _ratio: f32,
        _s: u32,
    ) -> Result<Vec<u8>, AudioDecodeError> {
        Ok(self.excerpt_wav.clone())
    }
}

#[test]
fn step_5b_preview_verification_confirms_track_and_never_copies_audio_match_across_edits() {
    let track_words: Vec<u32> = (0..1800)
        .map(|i: u32| i.wrapping_mul(0x9e37_79b9).rotate_left((i % 17) + 1))
        .collect();
    let mut preview_words = vec![0x1111_2222; 12];
    preview_words.extend(track_words[700..920].iter().copied());
    preview_words.extend(std::iter::repeat_n(0x3333_4444, 16));

    // File has missing artist tag ("Unknown Artist"), so text-only caps at 0.80 until preview matches.
    let fixture = Fixture::default()
        .route_when(
            "https://api.deezer.com/search/track",
            |_| true,
            json!({ "data": [{
                "id": 42,
                "title": "Deep Space Anthem",
                "duration": 360,
                "artist": { "name": "Club Producer" },
                "album": { "id": 88, "title": "Deep Space EP" },
                "preview": "https://cdns-preview.dzcdn.net/deep.mp3"
            }]})
            .to_string(),
        )
        .route_bytes("https://cdns-preview.dzcdn.net/deep.mp3", b"fake-mp3-bytes");

    let database = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    let track_id = {
        let mut db = database.lock().unwrap();
        let source = db.source("D:\\House").unwrap();
        let mut file = scanned(1);
        file.path = "Deep Space Anthem.mp3".to_owned();
        file.audio = Some(AudioFacts {
            duration_ms: 360_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: None,
            bitrate_kbps: Some(320),
        });
        file.tags = vec![(TagField::Title, "Deep Space Anthem".to_owned())];
        db.record_scan(source, &[file]).unwrap();
        let track_id = db.tracks().unwrap()[0].track_id;
        let fp = resonance_core::identity::Chromaprint {
            encoded: "AQA".to_owned(),
            raw: track_words[..950].to_vec(),
            duration_s: 360,
        };
        let identity = resonance_core::identity::AssetIdentity {
            byte_hash: resonance_core::identity::ContentHash::from_bytes([1; 32]),
            audio: Some(resonance_core::identity::AudioHash {
                hash: resonance_core::identity::ContentHash::from_bytes([2; 32]),
                decoder: "symphonia/0.6.1".to_owned(),
            }),
            chromaprint: Some(fp),
            decoded_facts: None,
        };
        db.record_asset_identity(
            1,
            resonance_db::FileStamp {
                size_bytes: 4_000_000,
                modified_ns: 1_700_000_000_000_000_000,
            },
            &identity,
        )
        .unwrap();
        track_id
    };

    let pipeline = Pipeline::with_free_providers(
        Arc::new(fixture),
        Arc::clone(&database),
        Arc::new(MemoryCredentialStore::new()),
    )
    .with_fingerprinter(Arc::new(MockFingerprinter {
        track_full: track_words.clone(),
        preview: preview_words.clone(),
        excerpt_wav: b"RIFF".to_vec(),
    }));

    let report = run_online_pass(&database, &pipeline, &[track_id], &never(), |_, _| {}).unwrap();
    assert_eq!(report.confident, 1);

    let stored = database
        .lock()
        .unwrap()
        .identification(track_id)
        .unwrap()
        .unwrap();
    assert!(stored.audio_matched);
    assert!(stored.confidence.unwrap() >= STOP_THRESHOLD);
    let stored_cands =
        resonance_identify::library::parse_ranked_candidates(&stored.candidates_json);
    assert!(!stored_cands.is_empty());

    // A radio edit must not match the extended file, and the user can switch to the runner-up.
    let mut radio_input = input(track_id, 412_000, false);
    radio_input.tags = vec![(TagField::Title, "Deep Space Anthem".to_owned())];
    let fixture_radio = Fixture::default()
        .route_when(
            "https://api.deezer.com/search/track",
            |_| true,
            json!({ "data": [{
                "id": 99,
                "title": "Deep Space Anthem (Radio Edit)",
                "duration": 195,
                "artist": { "name": "Club Producer" },
                "album": { "id": 89, "title": "Single" },
                "preview": "https://cdns-preview.dzcdn.net/radio.mp3"
            }]})
            .to_string(),
        )
        .route_bytes(
            "https://cdns-preview.dzcdn.net/radio.mp3",
            b"fake-mp3-bytes",
        );
    let db_radio = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    {
        let mut db = db_radio.lock().unwrap();
        let src = db.source("D:\\House").unwrap();
        let mut f = scanned(1);
        f.audio = Some(AudioFacts {
            duration_ms: 412_000,
            sample_rate: Some(44_100),
            channels: Some(2),
            bit_depth: None,
            bitrate_kbps: Some(320),
        });
        db.record_scan(src, &[f]).unwrap();
        let identity = resonance_core::identity::AssetIdentity {
            byte_hash: resonance_core::identity::ContentHash::from_bytes([1; 32]),
            audio: Some(resonance_core::identity::AudioHash {
                hash: resonance_core::identity::ContentHash::from_bytes([2; 32]),
                decoder: "symphonia/0.6.1".to_owned(),
            }),
            chromaprint: Some(resonance_core::identity::Chromaprint {
                encoded: "AQA".to_owned(),
                raw: track_words[..950].to_vec(),
                duration_s: 412,
            }),
            decoded_facts: None,
        };
        db.record_asset_identity(
            1,
            resonance_db::FileStamp {
                size_bytes: 4_000_000,
                modified_ns: 1_700_000_000_000_000_000,
            },
            &identity,
        )
        .unwrap();
    }
    let pipeline_radio = Pipeline::with_free_providers(
        Arc::new(fixture_radio),
        db_radio,
        Arc::new(MemoryCredentialStore::new()),
    )
    .with_fingerprinter(Arc::new(MockFingerprinter {
        track_full: track_words,
        preview: preview_words,
        excerpt_wav: b"RIFF".to_vec(),
    }));
    let radio_out = pipeline_radio.identify(&[radio_input], &never()).remove(0);
    assert!(
        !radio_out.best.as_ref().is_some_and(|b| b.audio_matched),
        "Radio Edit preview match must NOT mark a 6:52 Extended Mix track as audio_matched"
    );
}

#[test]
fn select_candidate_switches_proposals_without_touching_user_locked_fields() {
    let fixture = Fixture::default()
        .route_when(
            "https://api.deezer.com/search/track",
            |_| true,
            json!({ "data": [
                { "id": 1, "title": "Night Drive", "duration": 300, "artist": { "name": "Some Artist" }, "album": { "id": 10, "title": "First EP" } },
                { "id": 2, "title": "Night Drive", "duration": 305, "artist": { "name": "Some Artist" }, "album": { "id": 11, "title": "Club Compilation" } }
            ]})
            .to_string(),
        );
    let (_, pipeline, database) = setup(fixture, &[]);
    let track = {
        let mut db = database.lock().unwrap();
        let src = db.source("library").unwrap();
        let mut f = scanned(1);
        f.tags = vec![
            (TagField::Artist, "Some Artist".to_owned()),
            (TagField::Title, "Night Drive".to_owned()),
        ];
        db.record_scan(src, &[f]).unwrap();
        let id = db.tracks().unwrap()[0].track_id;
        db.set_user_tags(
            &[id],
            &[(TagField::Album, Some("User Locked Album".to_owned()))],
        )
        .unwrap();
        id
    };

    let _ = run_online_pass(&database, &pipeline, &[track], &never(), |_, _| {}).unwrap();
    let stored = database
        .lock()
        .unwrap()
        .identification(track)
        .unwrap()
        .unwrap();
    let cands = resonance_identify::library::parse_ranked_candidates(&stored.candidates_json);
    assert_eq!(cands.len(), 2);

    let switched = select_candidate(&database, &pipeline, track, 1)
        .unwrap()
        .unwrap();
    assert_eq!(
        switched.candidate.album.as_deref(),
        Some("Club Compilation")
    );
    let db = database.lock().unwrap();
    assert_eq!(
        db.identification(track)
            .unwrap()
            .unwrap()
            .selected_candidate,
        1
    );
    let layers = db.tag_layers(track).unwrap().unwrap();
    assert_eq!(
        layers.user,
        [(TagField::Album, "User Locked Album".to_owned())]
    );
}

#[test]
fn step_6_supports_soundcloud_and_youtube_oembed() {
    let fixture = Fixture::default()
        .route(
            "https://soundcloud.com/oembed",
            json!({
                "title": "Some Artist - Night Drive (Club Mix)",
                "author_name": "House Label",
                "thumbnail_url": "https://i1.sndcdn.com/cover.jpg"
            }),
        )
        .route(
            "https://www.youtube.com/oembed",
            json!({
                "title": "Some Artist - Night Drive (Extended Mix)",
                "author_name": "Some Artist - Topic",
                "thumbnail_url": "https://i.ytimg.com/vi/abc/hqdefault.jpg"
            }),
        );
    let (_, pipeline, _) = setup(fixture, &[]);

    let sc = pipeline.identify_with_url(
        &input(1, 300_000, false),
        &ReleaseUrl::SoundCloud("https://soundcloud.com/label/night-drive".to_owned()),
    );
    let sc_best = sc.best.as_ref().unwrap();
    assert_eq!(sc_best.candidate.mix.as_deref(), Some("Club Mix"));
    assert_eq!(
        sc_best.candidate.artwork_provider.as_deref(),
        Some("soundcloud")
    );

    let yt = pipeline.identify_with_url(
        &input(1, 412_000, false),
        &ReleaseUrl::YouTube("dQw4w9WgXcQ".to_owned()),
    );
    let yt_best = yt.best.as_ref().unwrap();
    assert_eq!(yt_best.candidate.mix.as_deref(), Some("Extended Mix"));
    assert_eq!(yt_best.candidate.artwork_url, None);
}

#[test]
fn itunes_mix_hint_fallback_finds_extended_mix_without_storing_apple_art() {
    let fixture = Fixture::default()
        .route_when(
            "https://itunes.apple.com/search",
            |req| param(req, "term") == Some("Some Artist Night Drive"),
            json!({ "results": [{
                "wrapperType": "track",
                "trackId": 1001u64,
                "collectionId": 5001u64,
                "artistName": "Some Artist",
                "trackName": "Night Drive (Radio Edit)",
                "collectionName": "Night Drive - Single",
                "trackTimeMillis": 195_000u64,
                "trackViewUrl": "https://music.apple.com/album/1001",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb.jpg"
            }]})
            .to_string(),
        )
        .route_when(
            "https://itunes.apple.com/search",
            |req| param(req, "term") == Some("Some Artist Night Drive extended"),
            json!({ "results": [{
                "wrapperType": "track",
                "trackId": 1002u64,
                "collectionId": 5001u64,
                "artistName": "Some Artist",
                "trackName": "Night Drive (Extended Mix)",
                "collectionName": "Night Drive - EP",
                "primaryGenreName": "House",
                "releaseDate": "2022-04-01T07:00:00Z",
                "trackTimeMillis": 412_000u64,
                "trackNumber": 2,
                "trackViewUrl": "https://music.apple.com/album/1002"
            }]})
            .to_string(),
        );
    let (fixture, pipeline, _) = setup(fixture, &[]);

    let mut track = input(1, 412_000, false);
    track.path = "Downloads/Some Artist - Night Drive (Extended Mix).flac".to_owned();
    let found = pipeline.identify(&[track], &never()).remove(0);

    let best = found.best.as_ref().unwrap();
    assert_eq!(best.candidate.provider, "itunes");
    assert_eq!(best.candidate.mix.as_deref(), Some("Extended Mix"));
    assert_eq!(best.candidate.artwork_url, None);
    assert!(best.confidence >= STOP_THRESHOLD);
    assert_eq!(fixture.calls_to("https://itunes.apple.com/search"), 2);
}

#[test]
fn deezer_mix_hint_fallback_prioritises_matching_remix_enrichment() {
    let fixture = Fixture::default()
        .route_when(
            "https://api.deezer.com/search/track",
            |req| param(req, "q").is_some_and(|q| q.contains("artist:\"Some Artist\"")),
            json!({ "data": [{
                "id": 11,
                "title": "Night Drive",
                "duration": 195,
                "artist": { "name": "Some Artist" },
                "album": { "id": 70, "title": "Night Drive" }
            }]})
            .to_string(),
        )
        .route_when(
            "https://api.deezer.com/search/track",
            |req| param(req, "q") == Some("Some Artist Night Drive koze"),
            json!({ "data": [{
                "id": 12,
                "title": "Night Drive (DJ Koze Remix)",
                "duration": 420,
                "artist": { "name": "Some Artist" },
                "album": { "id": 71, "title": "Night Drive Remixes" }
            }]})
            .to_string(),
        )
        .route(
            "https://api.deezer.com/track/12",
            json!({
                "id": 12,
                "title": "Night Drive (DJ Koze Remix)",
                "duration": 420,
                "isrc": "DEABC2200012",
                "artist": { "name": "Some Artist" },
                "album": { "id": 71, "title": "Night Drive Remixes" }
            }),
        )
        .route(
            "https://api.deezer.com/album/71",
            json!({ "id": 71, "title": "Night Drive Remixes", "label": "Pampa", "record_type": "ep" }),
        );
    let (_, pipeline, _) = setup(fixture, &[]);

    let mut track = input(1, 420_000, false);
    track.path = "Downloads/Some Artist - Night Drive (DJ Koze Remix).mp3".to_owned();
    let found = pipeline.identify(&[track], &never()).remove(0);

    let best = found.best.as_ref().unwrap();
    assert_eq!(best.candidate.label.as_deref(), Some("Pampa"));
    assert_eq!(best.candidate.isrc.as_deref(), Some("DEABC2200012"));
    assert_eq!(best.candidate.mix.as_deref(), Some("DJ Koze Remix"));
}

#[test]
fn discogs_reaches_tier_3_for_remix_split_va_tracks_and_resists_bare_catno() {
    let fixture = Fixture::default()
        .route_when(
            "https://api.discogs.com/database/search",
            |req| param(req, "artist").is_some() && param(req, "track").is_some(),
            json!({ "results": [{ "id": 201 }] }).to_string(),
        )
        .route(
            "https://api.discogs.com/releases/201",
            json!({
                "id": 201,
                "title": "Night Drive",
                "artists": [{ "name": "Some Artist" }],
                "labels": [{ "name": "Some Label", "catno": "CAT01" }],
                "year": 2020,
                "tracklist": [{ "position": "A1", "title": "Night Drive (Original Mix)", "duration": "5:00" }]
            }),
        )
        .route_when(
            "https://api.discogs.com/database/search",
            |req| param(req, "q").is_some_and(|q| q.contains("solomun")),
            json!({ "results": [{ "id": 202 }] }).to_string(),
        )
        .route(
            "https://api.discogs.com/releases/202",
            json!({
                "id": 202,
                "title": "Club Tools Vol. 4",
                "artists": [{ "name": "Various" }],
                "labels": [{ "name": "Diynamic", "catno": "DIY099" }],
                "genres": ["Electronic"],
                "styles": ["House", "Deep House"],
                "year": 2021,
                "formats": [{ "name": "Vinyl", "descriptions": ["12\"", "EP"] }],
                "tracklist": [{
                    "position": "B1",
                    "title": "Some Artist - Night Drive (Solomun Remix)",
                    "duration": "6:52"
                }]
            }),
        );
    let (_, pipeline, _) = setup(fixture, &[ProviderKey::Discogs]);

    let mut track = input(1, 412_000, false);
    track.path = "Downloads/Some Artist - Night Drive (Solomun Remix).mp3".to_owned();
    let found = pipeline.identify(&[track], &never()).remove(0);

    let best = found.best.as_ref().unwrap();
    assert_eq!(best.candidate.provider, "discogs");
    assert_eq!(best.candidate.artist.as_deref(), Some("Some Artist"));
    assert_eq!(best.candidate.mix.as_deref(), Some("Solomun Remix"));
    assert_eq!(best.candidate.catno.as_deref(), Some("DIY099"));
    assert_eq!(
        best.candidate.subgenre.as_deref(),
        Some("House, Deep House")
    );
    assert!(best.confidence >= STOP_THRESHOLD);
}

#[test]
fn audd_runs_only_with_key_on_uncertain_tracks_and_respects_edit_guard() {
    let fixture_uncertain = Fixture::default().route_when(
        "https://api.audd.io/",
        |_| true,
        json!({
            "status": "success",
            "result": {
                "artist": "Identified Artist",
                "title": "White Label Anthem (Radio Edit)",
                "album": "White Label EP",
                "release_date": "2023-03-01",
                "label": "Club Cuts",
                "apple_music": { "durationInMillis": 195_000u64 }
            }
        })
        .to_string(),
    );
    let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    {
        let mut guard = db.lock().unwrap();
        let src = guard.source("D:\\House").unwrap();
        let mut f = scanned(1);
        f.path = "Track01.mp3".to_owned();
        f.audio.as_mut().unwrap().duration_ms = 410_000;
        guard.record_scan(src, &[f]).unwrap();
        let identity = resonance_core::identity::AssetIdentity {
            byte_hash: resonance_core::identity::ContentHash::from_bytes([3; 32]),
            audio: Some(resonance_core::identity::AudioHash {
                hash: resonance_core::identity::ContentHash::from_bytes([4; 32]),
                decoder: "symphonia/0.6.1".to_owned(),
            }),
            chromaprint: None,
            decoded_facts: None,
        };
        guard
            .record_asset_identity(
                1,
                resonance_db::FileStamp {
                    size_bytes: 4_000_000,
                    modified_ns: 1_700_000_000_000_000_000,
                },
                &identity,
            )
            .unwrap();
    }
    let creds = MemoryCredentialStore::new();
    creds.write(ProviderKey::AudD, "audd-token").unwrap();
    let pipeline = Pipeline::with_free_providers(
        Arc::new(fixture_uncertain),
        Arc::clone(&db),
        Arc::new(creds),
    )
    .with_fingerprinter(Arc::new(MockFingerprinter {
        track_full: Vec::new(),
        preview: Vec::new(),
        excerpt_wav: b"RIFF".to_vec(),
    }));

    let mut track = input(1, 410_000, false);
    track.path = "Downloads/Track01.mp3".to_owned();
    track.tags = vec![(TagField::Title, "White Label Anthem".to_owned())];
    let found = pipeline.identify(&[track], &never()).remove(0);

    // AudD returned a 195 s Radio Edit for our 410 s Extended file: duration > 25 s mismatch,
    // so edit guard MUST block marking the 410 s track as `audio_matched`.
    assert!(!found.best.as_ref().is_some_and(|b| b.audio_matched));
}

#[test]
fn switching_candidate_zero_to_one_and_back_restores_pending_proposed_artwork() {
    use resonance_core::identify::{Candidate, RankedCandidate, rank};
    use resonance_identify::library::parse_ranked_candidates;
    let folder = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cand-art-roundtrip");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let fixture = Fixture::default().route_bytes(
        "https://coverartarchive.org/release/rel-1/front-500",
        &COVER,
    );
    let mut db = Database::open(&folder.join("library.db")).unwrap();
    db.set_artwork_folder(folder.join("artwork"));
    let src = db.source("D:\\House").unwrap();
    db.record_scan(src, &[scanned(1)]).unwrap();

    let known = resonance_core::identify::KnownTags {
        artist: Some("Artist".to_owned()),
        title: Some("Track One".to_owned()),
        ..Default::default()
    };
    let c0 = Candidate {
        provider: "musicbrainz".to_owned(),
        artist: Some("Artist".to_owned()),
        title: Some("Track One".to_owned()),
        album: Some("EP With Art".to_owned()),
        duration_ms: Some(250_000),
        artwork_url: Some("https://coverartarchive.org/release/rel-1/front-500".to_owned()),
        artwork_provider: Some("musicbrainz".to_owned()),
        ..Candidate::default()
    };
    let c1 = Candidate {
        provider: "deezer".to_owned(),
        artist: Some("Artist".to_owned()),
        title: Some("Track One".to_owned()),
        album: Some("EP Without Art".to_owned()),
        duration_ms: Some(250_000),
        ..Candidate::default()
    };
    let dead_cover = Candidate {
        provider: "musicbrainz".to_owned(),
        artist: Some("Artist".to_owned()),
        title: Some("Track One".to_owned()),
        album: Some("EP With A Dead Cover".to_owned()),
        duration_ms: Some(200_000),
        artwork_url: Some("https://coverartarchive.org/release/rel-gone/front-500".to_owned()),
        artwork_provider: Some("musicbrainz".to_owned()),
        ..Candidate::default()
    };
    let ranked: Vec<RankedCandidate> = rank(&known, Some(250_000), vec![c0, c1, dead_cover]);
    let dead_index = ranked
        .iter()
        .position(|c| c.candidate.album.as_deref() == Some("EP With A Dead Cover"))
        .unwrap() as u32;
    assert_eq!(
        dead_index, 2,
        "the candidate with the wrong length ranks last"
    );
    let db = Arc::new(Mutex::new(db));
    let pipeline = Pipeline::with_free_providers(
        Arc::new(fixture),
        Arc::clone(&db),
        Arc::new(MemoryCredentialStore::new()),
    );
    let id = Identification {
        track_id: 1,
        known,
        best: ranked.first().cloned(),
        candidates: ranked,
        steps: Vec::new(),
    };
    {
        let mut guard = db.lock().unwrap();
        let best_json = serde_json::json!({
            "provider": id.best.as_ref().unwrap().candidate.provider,
            "confidence": id.best.as_ref().unwrap().confidence,
            "audio_matched": false,
            "evidence": { "basis": "tags_and_duration", "providers": ["musicbrainz"], "primary_provider": "musicbrainz", "duration_delta_ms": 0, "catno_matched": false, "isrc_matched": false },
        });
        let stored_row = resonance_db::StoredIdentification {
            track_id: 1,
            online: true,
            confidence: id.best.as_ref().map(|b| b.confidence),
            audio_matched: false,
            known_json: "{}".to_owned(),
            best_json: Some(best_json.to_string()),
            steps_json: "[]".to_owned(),
            candidates_json: serde_json::to_string(
                &id.candidates
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "provider": c.candidate.provider,
                            "providers": c.providers,
                            "confidence": c.confidence,
                            "audio_matched": false,
                            "artist": c.candidate.artist,
                            "title": c.candidate.title,
                            "album": c.candidate.album,
                            "duration_ms": c.candidate.duration_ms,
                            "duration_delta_ms": c.duration_delta_ms,
                            "artwork_url": c.candidate.artwork_url,
                            "artwork_provider": c.candidate.artwork_provider,
                            "evidence": { "basis": "tags_and_duration", "providers": [c.candidate.provider], "primary_provider": c.candidate.provider, "duration_delta_ms": 0, "catno_matched": false, "isrc_matched": false }
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
            selected_candidate: 0,
            updated_ns: 0,
        };
        guard.record_identification(&stored_row).unwrap();
        assert_eq!(
            parse_ranked_candidates(&guard.identification(1).unwrap().unwrap().candidates_json)
                .len(),
            3
        );
    }

    select_candidate(&db, &pipeline, 1, 0).unwrap().unwrap();
    assert!(
        db.lock().unwrap().proposed_artwork(1).unwrap().is_some(),
        "candidate 0 must stage pending artwork"
    );

    select_candidate(&db, &pipeline, 1, 1).unwrap().unwrap();
    assert!(
        db.lock().unwrap().proposed_artwork(1).unwrap().is_none(),
        "switching to artless candidate clears pending proposal without marking skipped"
    );

    select_candidate(&db, &pipeline, 1, 0).unwrap().unwrap();
    assert!(
        db.lock().unwrap().proposed_artwork(1).unwrap().is_some(),
        "switching back 0 -> 1 -> 0 must restore pending artwork proposal"
    );

    select_candidate(&db, &pipeline, 1, dead_index)
        .unwrap()
        .unwrap();
    assert!(
        db.lock().unwrap().proposed_artwork(1).unwrap().is_none(),
        "a match whose cover cannot be fetched still drops the last match's cover"
    );
}

#[test]
fn use_candidate_artwork_stages_and_accepts_alt_cover_even_when_file_has_embedded_art() {
    use resonance_core::identify::{Candidate, RankedCandidate, rank};
    use resonance_db::StoredIdentification;
    use resonance_identify::library::parse_ranked_candidates;

    let folder = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("use-cand-art");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let alt_url = "https://i.discogs.com/alt-cover.jpg";
    let fixture = Fixture::default().route_bytes(alt_url, &COVER);
    let mut db = Database::open(&folder.join("library.db")).unwrap();
    db.set_artwork_folder(folder.join("artwork"));
    let src = db.source("D:\\House").unwrap();
    let scanned_file = scanned(1);
    db.record_scan(src, std::slice::from_ref(&scanned_file))
        .unwrap();
    let existing = db
        .artwork_store()
        .unwrap()
        .put(&[0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 4])
        .unwrap();
    db.record_scanned_artwork(src, &[(scanned_file.path.clone(), Some(existing))])
        .unwrap();

    let known = resonance_core::identify::KnownTags {
        artist: Some("Some Artist".to_owned()),
        title: Some("Tune 1".to_owned()),
        ..Default::default()
    };
    let c0 = Candidate {
        provider: "deezer".to_owned(),
        artist: Some("Some Artist".to_owned()),
        title: Some("Tune 1".to_owned()),
        album: Some("First EP".to_owned()),
        duration_ms: Some(300_000),
        ..Candidate::default()
    };
    let c1 = Candidate {
        provider: "discogs".to_owned(),
        artist: Some("Some Artist".to_owned()),
        title: Some("Tune 1".to_owned()),
        album: Some("Second Pressing".to_owned()),
        artwork_url: Some(alt_url.to_owned()),
        artwork_provider: Some("discogs".to_owned()),
        ..Candidate::default()
    };
    let ranked: Vec<RankedCandidate> = rank(&known, Some(300_000), vec![c0, c1]);
    let db = Arc::new(Mutex::new(db));
    let pipeline = Pipeline::with_free_providers(
        Arc::new(fixture),
        Arc::clone(&db),
        Arc::new(MemoryCredentialStore::new()),
    );
    {
        let mut guard = db.lock().unwrap();
        let best_json = serde_json::json!({
            "provider": ranked[0].candidate.provider,
            "confidence": ranked[0].confidence,
            "audio_matched": false,
            "evidence": { "basis": "tags_and_duration", "providers": ["deezer"], "primary_provider": "deezer", "duration_delta_ms": 0, "catno_matched": false, "isrc_matched": false }
        });
        let stored_row = StoredIdentification {
            track_id: 1,
            online: true,
            confidence: Some(ranked[0].confidence),
            audio_matched: false,
            known_json: "{}".to_owned(),
            best_json: Some(best_json.to_string()),
            steps_json: "[]".to_owned(),
            candidates_json: serde_json::to_string(
                &ranked
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "provider": c.candidate.provider,
                            "providers": c.providers,
                            "confidence": c.confidence,
                            "audio_matched": false,
                            "artist": c.candidate.artist,
                            "title": c.candidate.title,
                            "album": c.candidate.album,
                            "duration_ms": c.candidate.duration_ms,
                            "duration_delta_ms": c.duration_delta_ms,
                            "artwork_url": c.candidate.artwork_url,
                            "artwork_provider": c.candidate.artwork_provider,
                            "evidence": { "basis": "tags_only", "providers": [c.candidate.provider], "primary_provider": c.candidate.provider, "catno_matched": false, "isrc_matched": false }
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap(),
            selected_candidate: 0,
            updated_ns: 0,
        };
        guard.record_identification(&stored_row).unwrap();
        guard
            .store_proposals(1, &resonance_core::staging::proposals_from(&ranked[0]))
            .unwrap();
        assert_eq!(
            parse_ranked_candidates(&guard.identification(1).unwrap().unwrap().candidates_json)
                .len(),
            2
        );
    }

    let staged_hash = use_candidate_artwork(&db, &pipeline, 1, 1, false)
        .unwrap()
        .expect("alt candidate artwork staged");
    {
        let guard = db.lock().unwrap();
        assert_eq!(
            guard.identification(1).unwrap().unwrap().selected_candidate,
            0
        );
        let staged = guard.proposed_artwork(1).unwrap().unwrap();
        assert!(staged.pending);
        assert_eq!(staged.artwork.hash, staged_hash);
    }
    // Keeping the file's cover and then picking the same match again brings it back.
    db.lock().unwrap().skip_proposed_artwork(1).unwrap();
    assert!(
        !db.lock()
            .unwrap()
            .proposed_artwork(1)
            .unwrap()
            .unwrap()
            .pending
    );
    use_candidate_artwork(&db, &pipeline, 1, 1, false)
        .unwrap()
        .expect("picked again");
    {
        let guard = db.lock().unwrap();
        let staged = guard.proposed_artwork(1).unwrap().unwrap();
        assert!(staged.pending, "a cover the user picks is suggested again");
        let layers = guard.tag_layers(1).unwrap().unwrap();
        assert!(
            layers
                .proposals
                .iter()
                .any(|p| p.proposal.field == TagField::Album && p.proposal.value == "First EP")
        );
    }

    let accepted_hash = use_candidate_artwork(&db, &pipeline, 1, 1, true)
        .unwrap()
        .expect("alt candidate artwork accepted");
    assert_eq!(accepted_hash, staged_hash);
    let guard = db.lock().unwrap();
    assert_eq!(guard.track_artwork(1).unwrap().unwrap().hash, accepted_hash);
    assert!(guard.proposed_artwork(1).unwrap().is_none());
}

#[test]
fn preview_candidate_artwork_caches_positive_and_negative_results_without_staging() {
    use resonance_db::StoredIdentification;
    use resonance_identify::library::preview_candidate_artwork;

    let folder = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("preview-cand-art");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let ok_url = "https://i.discogs.com/preview-ok.jpg";
    let missing_url = "https://coverartarchive.org/release/missing-rel/front-500";
    let fixture = Arc::new(Fixture::default().route_bytes(ok_url, &COVER));
    let mut db = Database::open(&folder.join("library.db")).unwrap();
    db.set_artwork_folder(folder.join("artwork"));
    let src = db.source("D:\\House").unwrap();
    let scanned_file = scanned(1);
    db.record_scan(src, std::slice::from_ref(&scanned_file))
        .unwrap();
    db.record_identification(&StoredIdentification {
        track_id: 1,
        online: true,
        confidence: Some(0.95),
        audio_matched: false,
        known_json: "{}".to_owned(),
        best_json: None,
        steps_json: "[]".to_owned(),
        candidates_json: serde_json::to_string(&[
            serde_json::json!({
                "provider": "discogs",
                "providers": ["discogs"],
                "confidence": 0.95,
                "audio_matched": false,
                "artwork_url": ok_url,
                "artwork_provider": "discogs",
                "evidence": { "basis": "tags_only", "providers": ["discogs"], "primary_provider": "discogs", "catno_matched": false, "isrc_matched": false }
            }),
            serde_json::json!({
                "provider": "musicbrainz",
                "providers": ["musicbrainz"],
                "confidence": 0.90,
                "audio_matched": false,
                "artwork_url": missing_url,
                "artwork_provider": "musicbrainz",
                "evidence": { "basis": "tags_only", "providers": ["musicbrainz"], "primary_provider": "musicbrainz", "catno_matched": false, "isrc_matched": false }
            }),
        ])
        .unwrap(),
        selected_candidate: 0,
        updated_ns: 0,
    })
    .unwrap();

    let db = Arc::new(Mutex::new(db));
    let pipeline = Pipeline::with_free_providers(
        Arc::clone(&fixture) as Arc<dyn HttpClient>,
        Arc::clone(&db),
        Arc::new(MemoryCredentialStore::new()),
    );

    let first_hash = preview_candidate_artwork(&db, &pipeline, 1, 0)
        .unwrap()
        .expect("ok candidate resolves hash");
    assert_eq!(fixture.calls_to(ok_url), 1);
    assert_eq!(
        preview_candidate_artwork(&db, &pipeline, 1, 0).unwrap(),
        Some(first_hash)
    );
    assert_eq!(
        fixture.calls_to(ok_url),
        1,
        "second preview must hit artwork_source cache"
    );

    assert_eq!(
        preview_candidate_artwork(&db, &pipeline, 1, 1).unwrap(),
        None
    );
    assert_eq!(fixture.calls_to(missing_url), 1);
    assert_eq!(
        preview_candidate_artwork(&db, &pipeline, 1, 1).unwrap(),
        None
    );
    assert_eq!(
        fixture.calls_to(missing_url),
        1,
        "404 preview must negative-cache as Unavailable"
    );

    let guard = db.lock().unwrap();
    assert!(guard.proposed_artwork(1).unwrap().is_none());
    assert!(guard.track_artwork(1).unwrap().is_none());
    assert_eq!(
        guard.identification(1).unwrap().unwrap().selected_candidate,
        0
    );
}

#[test]
fn musicbrainz_isrc_lookup_omits_release_groups_and_catno_collision_stays_unresolved() {
    use resonance_identify::library::run_online_pass;

    let fixture = Arc::new(
        Fixture::default()
            .route_when(
                "https://musicbrainz.org/ws/2/isrc/GBXYZ2100099",
                |req| {
                    let inc = param(req, "inc").unwrap_or_default();
                    assert!(!inc.contains("release-groups"), "invalid inc: {inc}");
                    inc.contains("releases") && inc.contains("artist-credits")
                },
                json!({ "recordings": [{
                    "id": RECORDING,
                    "title": "Night Drive",
                    "length": 300_000,
                    "artist-credit": [{ "name": "Some Artist" }],
                    "releases": [{
                        "id": "rel-isrc-1",
                        "title": "Night Drive EP",
                        "status": "Official",
                        "date": "2022",
                        "cover-art-archive": { "front": true },
                        "label-info": [{ "catalog-number": "CAT01", "label": { "name": "Club Label" } }]
                    }]
                }]})
                .to_string(),
            )
            .route_when(
                "https://api.discogs.com/database/search",
                |req| param(req, "catno") == Some("DUP009"),
                json!({ "results": [{ "id": 909 }] }).to_string(),
            )
            .route(
                "https://api.discogs.com/releases/909",
                json!({
                    "id": 909,
                    "title": "Unrelated Punk Album",
                    "artists": [{ "name": "Other Band" }],
                    "labels": [{ "name": "Punk Label", "catno": "DUP009" }],
                    "genres": ["Rock"],
                    "styles": ["Hardcore", "Punk"],
                    "tracklist": [{ "type_": "track", "position": "1", "title": "Wrong Song", "duration": "5:00" }]
                }),
            ),
    );
    let mut track = input(1, 300_000, false);
    track.file_isrc = Some("GBXYZ2100099".to_owned());
    let (_, pipeline, _) = setup(
        Fixture::default().route_when(
            "https://musicbrainz.org/ws/2/isrc/GBXYZ2100099",
            |req| {
                !param(req, "inc")
                    .unwrap_or_default()
                    .contains("release-groups")
            },
            json!({ "recordings": [{
                "id": RECORDING,
                "title": "Night Drive",
                "length": 300_000,
                "artist-credit": [{ "name": "Some Artist" }],
                "releases": [{
                    "id": "rel-isrc-1",
                    "title": "Night Drive EP",
                    "status": "Official",
                    "date": "2022",
                    "cover-art-archive": { "front": true },
                    "label-info": [{ "catalog-number": "CAT01", "label": { "name": "Club Label" } }]
                }]
            }]})
            .to_string(),
        ),
        &[],
    );
    let isrc_result = pipeline.identify(&[track], &never()).remove(0);
    assert!(isrc_result.best.as_ref().unwrap().confidence >= STOP_THRESHOLD);

    let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    {
        let mut guard = db.lock().unwrap();
        let src = guard.source("D:\\House").unwrap();
        let mut f = scanned(1);
        f.path = "Artist A - Real House Cut [DUP009].mp3".to_owned();
        f.audio.as_mut().unwrap().duration_ms = 300_000;
        f.tags = vec![
            (TagField::Artist, "Artist A".to_owned()),
            (TagField::Title, "Real House Cut".to_owned()),
            (TagField::CatNo, "DUP009".to_owned()),
        ];
        guard.record_scan(src, &[f]).unwrap();
        guard
            .store_proposals(
                1,
                &[resonance_core::staging::Proposal {
                    field: TagField::Genre,
                    value: "Rock".to_owned(),
                    provider: "discogs".to_owned(),
                    confidence: 0.40,
                    audio_matched: false,
                }],
            )
            .unwrap();
    }
    let creds = Arc::new(MemoryCredentialStore::new());
    creds.write(ProviderKey::Discogs, "discogs-key").unwrap();
    let collision_pipe = Pipeline::with_free_providers(fixture, Arc::clone(&db), creds);
    let report = run_online_pass(&db, &collision_pipe, &[], &never(), |_, _| {}).unwrap();
    assert_eq!(report.needs_url, 1);

    let guard = db.lock().unwrap();
    let stored = guard.identification(1).unwrap().unwrap();
    assert!(stored.best_json.is_none());
    assert_eq!(stored.candidates_json, "[]");
    assert!(guard.tag_layers(1).unwrap().unwrap().proposals.is_empty());
}

#[test]
fn folded_fallback_search_and_sibling_filtering_behave_cleanly() {
    let fixture = Fixture::default()
        .route_when(
            "https://api.deezer.com/search/track",
            |req| param(req, "q") == Some("nora ost its fine"),
            json!({ "data": [{
                "id": 701,
                "title": "Its Fine",
                "duration": 360,
                "artist": { "name": "Nora Öst" },
                "album": { "id": 70, "title": "Fine EP" }
            }]})
            .to_string(),
        )
        .route_when(
            "https://api.discogs.com/database/search",
            |req| param(req, "artist") == Some("Nora Öst"),
            json!({ "results": [{ "id": 801 }] }).to_string(),
        )
        .route(
            "https://api.discogs.com/releases/801",
            json!({
                "id": 801,
                "title": "Fine EP",
                "artists": [{ "name": "Various" }],
                "labels": [{ "name": "Club Label", "catno": "CL801" }],
                "tracklist": [
                    { "type_": "track", "position": "A1", "title": "Nora Öst - It's Fine", "duration": "6:00" },
                    { "type_": "track", "position": "B1", "title": "Other Act - Different Tune", "duration": "6:00" }
                ]
            }),
        );
    let (_, pipeline, _) = setup(fixture, &[ProviderKey::Discogs]);
    let mut track = input(1, 360_000, false);
    track.tags.clear();
    track.path = "Downloads/Nora Öst - It's Fine.wav".to_owned();
    let result = pipeline.identify(&[track], &never()).remove(0);

    let best = result.best.as_ref().unwrap();
    assert!(best.confidence >= STOP_THRESHOLD);
    assert!(best.providers.contains(&"deezer".to_owned()));
    assert!(best.providers.contains(&"discogs".to_owned()));
    assert_eq!(best.candidate.catno.as_deref(), Some("CL801"));
    assert_eq!(
        result.candidates.len(),
        1,
        "unrelated B1 track from the same vinyl release must be filtered out"
    );
}

#[test]
fn musicbrainz_isrc_lookup_enriches_label_when_cover_art_is_already_present() {
    let fixture = Fixture::default()
        .route(
            "https://musicbrainz.org/ws/2/isrc/GBXYZ2100088",
            json!({ "recordings": [{
                "id": RECORDING,
                "title": "Night Drive",
                "length": 300_000,
                "artist-credit": [{ "name": "Some Artist" }],
                "releases": [{
                    "id": "rel-caa-only",
                    "title": "Night Drive EP",
                    "status": "Official",
                    "date": "2023",
                    "cover-art-archive": { "front": true }
                }]
            }] }),
        )
        .route_when(
            "https://musicbrainz.org/ws/2/release",
            |req| param(req, "recording") == Some(RECORDING),
            json!({ "releases": [{
                "id": "rel-caa-only",
                "title": "Night Drive EP",
                "status": "Official",
                "date": "2023",
                "cover-art-archive": { "front": true },
                "release-group": { "primary-type": "EP" },
                "label-info": [{ "catalog-number": "CAT88", "label": { "name": "Club Label" } }]
            }] })
            .to_string(),
        );
    let (_, pipeline, _) = setup(fixture, &[]);
    let mut track = input(1, 300_000, false);
    track.file_isrc = Some("GBXYZ2100088".to_owned());
    let result = pipeline.identify(&[track], &never()).remove(0);

    let best = result.best.as_ref().unwrap();
    assert_eq!(best.candidate.label.as_deref(), Some("Club Label"));
    assert_eq!(best.candidate.catno.as_deref(), Some("CAT88"));
    assert_eq!(
        best.candidate.release_kind,
        resonance_core::identify::ReleaseKind::SingleOrEp
    );
    assert!(best.candidate.artwork_url.is_some());
}

#[test]
fn named_edit_rejects_original_release_and_matches_remixer_credited_candidate() {
    let original_only_fixture = Fixture::default()
        .route_when(
            "https://api.discogs.com/database/search",
            |req| param(req, "artist").is_some() && param(req, "track").is_some(),
            json!({ "results": [{ "id": 910 }] }).to_string(),
        )
        .route(
            "https://api.discogs.com/releases/910",
            json!({
                "id": 910,
                "title": "Original 12\"",
                "artists": [{ "name": "Some Artist" }],
                "labels": [{ "name": "Orig Label", "catno": "OR01" }],
                "tracklist": [
                    { "type_": "track", "position": "A1", "title": "Night Drive", "duration": "6:00" }
                ]
            }),
        );
    let (_, orig_pipe, _) = setup(original_only_fixture, &[ProviderKey::Discogs]);
    let mut edit_track = input(1, 360_000, false);
    edit_track.tags.clear();
    edit_track.path = "Downloads/Some Artist - Night Drive (Will & Nova Edit).wav".to_owned();
    let rejected = orig_pipe
        .identify(&[edit_track.clone()], &never())
        .remove(0);
    assert!(
        rejected.best.is_none(),
        "original release must not match a named edit"
    );
    assert!(rejected.needs_url());

    let remixer_fixture = Fixture::default()
        .route_when(
            "https://api.discogs.com/database/search",
            |req| param(req, "q") == Some("Night Drive will nova"),
            json!({ "results": [{ "id": 911 }] }).to_string(),
        )
        .route(
            "https://api.discogs.com/releases/911",
            json!({
                "id": 911,
                "title": "Edits Vol. 1",
                "artists": [{ "name": "Will & Nova" }],
                "labels": [{ "name": "Edit Imprint", "catno": "ED01" }],
                "tracklist": [
                    { "type_": "track", "position": "A1", "title": "Night Drive", "duration": "6:00" }
                ]
            }),
        );
    let (_, remixer_pipe, _) = setup(remixer_fixture, &[ProviderKey::Discogs]);
    let matched = remixer_pipe.identify(&[edit_track], &never()).remove(0);
    let best = matched.best.as_ref().unwrap();
    assert!(best.confidence > 0.85, "got {}", best.confidence);
    assert_eq!(best.candidate.label.as_deref(), Some("Edit Imprint"));
}
