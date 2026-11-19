//! The cascade against fixture providers: every answer comes from this file, nothing goes to
//! the network.

#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

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
use resonance_identify::credentials::{CredentialStore, MemoryCredentialStore};
use resonance_identify::http::{BytesResponse, HttpClient, HttpRequest, HttpResponse, Unreachable};
use resonance_identify::library::{identify_track_with_url, run_online_pass};
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
            },
            None => HttpResponse {
                status: 404,
                body: String::new(),
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
    assert!(radio.confidence >= STOP_THRESHOLD);

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
        outcomes(&found, Step::TextSearch)[1],
        (Some("discogs"), StepOutcome::SkippedConfident)
    );
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
    assert_eq!(first.looked_up, 10);
    assert_eq!(first.needs_url, 10);
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
    assert_eq!(second.looked_up, 2);
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
                (TagField::Album, "Night Drive"),
                (TagField::Year, "2019"),
                (TagField::Label, "Some Label"),
                (TagField::Mix, "Extended Mix"),
                (TagField::CatNo, "DEF023"),
            ]
        );
        database
            .set_user_tags(
                &[track],
                &[(TagField::Label, Some("Typed Label".to_owned()))],
            )
            .unwrap();
        database.accept_proposals(track, &[TagField::Mix]).unwrap();
    }

    let second = run_online_pass(&database, &pipeline, &[], &never(), |_, _| {}).unwrap();

    assert_eq!(second.looked_up, 1);
    let mut database = database.lock().unwrap();
    let layers = database.tag_layers(track).unwrap().unwrap();
    assert_eq!(
        layers.user,
        [
            (TagField::Label, "Typed Label".to_owned()),
            (TagField::Mix, "Extended Mix".to_owned()),
        ]
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
    assert_eq!(accepted.fields, 3, "album, year and catalogue number");
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
