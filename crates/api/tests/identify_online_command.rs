#![allow(
    clippy::unwrap_used,
    reason = "every function in this file is test code"
)]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use resonance_api::Api;
use resonance_api::command::{
    ActivityJobStatus, ActivityJobTier, IdentifyStepOutcome, ProviderKeyKind, Request, Response,
    SortDirection, TagName,
};
use resonance_api::error::ErrorCode;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_identify::credentials::{CredentialStore, MemoryCredentialStore};
use resonance_identify::http::{BytesResponse, HttpClient, HttpRequest, HttpResponse, Unreachable};
use serde_json::json;

/// Answers MusicBrainz search and one Discogs release; anything else is a 404.
#[derive(Default)]
struct Providers {
    asked: Mutex<Vec<String>>,
}

impl HttpClient for Providers {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
        self.asked.lock().unwrap().push(request.url.clone());
        let body = if request
            .url
            .starts_with("https://musicbrainz.org/ws/2/recording")
        {
            json!({ "recordings": [{
                "id": "b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d", "score": 100,
                "title": "Night Drive", "length": 1_500,
                "artist-credit": [{ "name": "Some Artist" }],
                "releases": [{ "id": "c0a1b2c3-0000-4000-8000-000000000001",
                               "title": "Night Drive EP", "date": "2019" }]
            }]})
        } else if request.url == "https://api.discogs.com/releases/111" {
            json!({
                "title": "Night Drive EP", "year": 2019,
                "artists": [{ "name": "Some Artist" }],
                "labels": [{ "name": "Some Label", "catno": "DEF023" }],
                "tracklist": [{ "type_": "track", "title": "Night Drive", "duration": "0:02" }]
            })
        } else {
            return Ok(HttpResponse {
                status: 404,
                body: String::new(),
                retry_after_s: None,
            });
        };
        Ok(HttpResponse {
            status: 200,
            body: body.to_string(),
            retry_after_s: None,
        })
    }

    fn fetch_bytes(
        &self,
        request: &HttpRequest,
        _limit: u64,
    ) -> Result<BytesResponse, Unreachable> {
        self.asked.lock().unwrap().push(request.url.clone());
        Ok(BytesResponse {
            status: 404,
            body: Vec::new(),
        })
    }
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < timeout,
            "nothing settled within {timeout:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn online_identification_runs_as_a_job_and_a_pasted_release_is_read() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_identify_online");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    Signal::tone(330.0, 1.5, -9.0, 44_100)
        .write_wav(&music.join("Some Artist - Night Drive.wav"))
        .unwrap();

    let providers = Arc::new(Providers::default());
    let api = Api::new_silent(Database::open_in_memory().unwrap())
        .with_credential_store(Arc::new(MemoryCredentialStore::new()) as Arc<dyn CredentialStore>)
        .with_http_client(Arc::clone(&providers) as Arc<dyn HttpClient>);
    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .unwrap();

    let Response::IdentifyOnline(queued) = api
        .handle(Request::IdentifyOnline {
            track_ids: vec![],
            only_selected: false,
        })
        .unwrap()
    else {
        panic!("expected an Activity snapshot");
    };
    assert!(
        queued
            .active
            .iter()
            .any(|job| job.kind == "identify_online")
    );

    wait_until(Duration::from_secs(10), || {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            return false;
        };
        snapshot.recent.iter().any(|job| {
            job.kind == "identify_online"
                && job.status == ActivityJobStatus::Completed
                && job.done_items == 1
        })
    });
    assert!(
        providers
            .asked
            .lock()
            .unwrap()
            .iter()
            .any(|url| url.starts_with("https://musicbrainz.org/ws/2/recording"))
    );

    let refused = api
        .handle(Request::IdentifyWithUrl {
            track_id: 1,
            url: "https://example.com/release/1".to_owned(),
        })
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);

    let Response::IdentifyWithUrl(pasted) = api
        .handle(Request::IdentifyWithUrl {
            track_id: 1,
            url: "https://www.discogs.com/release/111-Some-Artist-Night-Drive-EP".to_owned(),
        })
        .unwrap()
    else {
        panic!("expected an identification");
    };
    let best = pasted.best.unwrap();
    assert_eq!(best.catno.as_deref(), Some("DEF023"));
    assert!(best.confidence >= 0.95);
    assert!(!best.audio_matched);
    assert!(pasted.steps.iter().any(
        |report| report.step == 6 && matches!(report.outcome, IdentifyStepOutcome::Ran { .. })
    ));

    let serialised = serde_json::to_value(Response::IdentifyWithUrl(
        resonance_api::command::TrackIdentification {
            track_id: 1,
            best: None,
            needs_url: true,
            steps: vec![resonance_api::command::IdentifyStepReport {
                step: 3,
                provider: Some("acoustid".to_owned()),
                outcome: IdentifyStepOutcome::SkippedMissingKey {
                    key: ProviderKeyKind::AcoustId,
                },
            }],
        },
    ))
    .unwrap();
    assert_eq!(
        serialised["result"]["steps"][0]["outcome"],
        json!({ "kind": "skipped_missing_key", "key": "acoustid" })
    );
}

#[test]
fn identification_settings_persist_and_prep_candidate_switching_is_atomic() {
    #[derive(Default)]
    struct MultiCandidateProviders;
    impl HttpClient for MultiCandidateProviders {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            let body = if request
                .url
                .starts_with("https://api.deezer.com/search/track")
            {
                json!({ "data": [
                    {
                        "id": 10,
                        "title": "Night Drive",
                        "duration": 17,
                        "artist": { "name": "Some Artist" },
                        "album": { "id": 70, "title": "First EP" }
                    },
                    {
                        "id": 11,
                        "title": "Night Drive (Radio Edit)",
                        "duration": 11,
                        "artist": { "name": "Some Artist" },
                        "album": { "id": 71, "title": "Second Edition" }
                    }
                ]})
                .to_string()
            } else {
                return Ok(HttpResponse {
                    status: 404,
                    body: String::new(),
                    retry_after_s: None,
                });
            };
            Ok(HttpResponse {
                status: 200,
                body,
                retry_after_s: None,
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

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_candidate_switch");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    Signal::tone(330.0, 10.0, -9.0, 44_100)
        .write_wav(&music.join("Some Artist - Night Drive.wav"))
        .unwrap();

    let api = Api::new_silent(Database::open_in_memory().unwrap())
        .with_credential_store(Arc::new(MemoryCredentialStore::new()) as Arc<dyn CredentialStore>)
        .with_http_client(Arc::new(MultiCandidateProviders) as Arc<dyn HttpClient>);

    let Response::IdentificationSettings(initial_settings) =
        api.handle(Request::IdentificationSettings).unwrap()
    else {
        panic!("expected settings");
    };
    assert!(initial_settings.verify_audio_previews);

    let Response::SetIdentificationSettings(updated) = api
        .handle(Request::SetIdentificationSettings {
            settings: resonance_api::command::IdentificationSettings {
                verify_audio_previews: false,
            },
        })
        .unwrap()
    else {
        panic!("expected updated settings");
    };
    assert!(!updated.verify_audio_previews);

    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .unwrap();
    api.handle(Request::IdentifyOnline {
        track_ids: vec![],
        only_selected: false,
    })
    .unwrap();
    wait_until(Duration::from_secs(10), || {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            return false;
        };
        snapshot
            .recent
            .iter()
            .any(|job| job.kind == "identify_online" && job.status == ActivityJobStatus::Completed)
    });

    let Response::PrepQueue(queue) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected prep queue");
    };
    assert_eq!(queue.items.len(), 1);
    let item = &queue.items[0];
    assert_eq!(item.candidates.len(), 2);
    assert!(item.candidates_expanded);
    assert!(item.evidence.is_some());

    let Response::SelectPrepCandidate(switched) = api
        .handle(Request::SelectPrepCandidate {
            track_id: item.track.id,
            candidate_index: 1,
            expected_review: None,
        })
        .unwrap()
    else {
        panic!("expected switched prep item");
    };
    assert_eq!(switched.selected_candidate, 1);

    let invalid = api
        .handle(Request::AcceptPrepItems {
            track_ids: vec![item.track.id, 999],
            fields: vec![],
            candidate_index: Some(0),
            skip_remaining: false,
            include_artwork: None,
        })
        .unwrap_err();
    assert_eq!(invalid.code, ErrorCode::BadRequest);

    let Response::AcceptPrepItems(accepted) = api
        .handle(Request::AcceptPrepItems {
            track_ids: vec![item.track.id],
            fields: vec![],
            candidate_index: Some(0),
            skip_remaining: false,
            include_artwork: None,
        })
        .unwrap()
    else {
        panic!("expected atomic accept");
    };
    assert_eq!(accepted.tracks, 1);

    // Selecting a candidate whose fields already match the file (`pending_fields == 0`) still
    // keeps its real confidence, provider and audio status on PrepItem.
    let Response::PrepQueue(all_queue) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected full prep queue");
    };
    let decided = all_queue
        .items
        .iter()
        .find(|i| i.track.id == item.track.id)
        .expect("decided item present");
    assert_eq!(decided.pending_fields, 0);
    assert!(decided.confidence > 0.5, "got {}", decided.confidence);
    assert!(decided.provider.is_some());
}

#[test]
fn use_prep_candidate_artwork_stages_cover_and_exposes_summary_metadata() {
    #[derive(Default)]
    struct ArtProviders;
    impl HttpClient for ArtProviders {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            let body = if request
                .url
                .starts_with("https://api.deezer.com/search/track")
            {
                json!({ "data": [{
                    "id": 10,
                    "title": "Night Drive",
                    "duration": 2,
                    "artist": { "name": "Some Artist" },
                    "album": { "id": 70, "title": "First EP" }
                }]})
                .to_string()
            } else if request
                .url
                .starts_with("https://api.discogs.com/database/search")
            {
                json!({ "results": [{ "id": 444 }] }).to_string()
            } else if request.url == "https://api.discogs.com/releases/444" {
                json!({
                    "id": 444,
                    "title": "Night Drive 12\"",
                    "year": 2023,
                    "genres": ["Electronic"],
                    "styles": ["Deep House"],
                    "artists": [{ "name": "Some Artist" }],
                    "labels": [{ "name": "Club Label", "catno": "CL01" }],
                    "images": [{ "type": "primary", "uri": "https://i.discogs.com/cl01.jpg" }],
                    "tracklist": [{ "type_": "track", "title": "Night Drive (Extended Mix)", "duration": "0:03" }]
                })
                .to_string()
            } else {
                return Ok(HttpResponse {
                    status: 404,
                    body: String::new(),
                    retry_after_s: None,
                });
            };
            Ok(HttpResponse {
                status: 200,
                body,
                retry_after_s: None,
            })
        }

        fn fetch_bytes(
            &self,
            request: &HttpRequest,
            _limit: u64,
        ) -> Result<BytesResponse, Unreachable> {
            if request.url == "https://i.discogs.com/cl01.jpg" {
                return Ok(BytesResponse {
                    status: 200,
                    body: vec![0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F'],
                });
            }
            Ok(BytesResponse {
                status: 404,
                body: Vec::new(),
            })
        }
    }

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_cand_art");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    Signal::tone(330.0, 2.0, -9.0, 44_100)
        .write_wav(&music.join("Some Artist - Night Drive.wav"))
        .unwrap();

    let creds = Arc::new(MemoryCredentialStore::new());
    creds
        .write(
            resonance_core::providers::ProviderKey::Discogs,
            "fixture-key",
        )
        .unwrap();
    let mut db = Database::open(&root.join("library.db")).unwrap();
    db.set_artwork_folder(root.join("artwork"));
    let api = Api::new_silent(db)
        .with_credential_store(creds as Arc<dyn CredentialStore>)
        .with_http_client(Arc::new(ArtProviders) as Arc<dyn HttpClient>);

    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .unwrap();
    api.handle(Request::IdentifyOnline {
        track_ids: vec![],
        only_selected: false,
    })
    .unwrap();
    wait_until(Duration::from_secs(10), || {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            return false;
        };
        snapshot
            .recent
            .iter()
            .any(|job| job.kind == "identify_online" && job.status == ActivityJobStatus::Completed)
    });

    let Response::PrepQueue(queue) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected prep queue");
    };
    let item = &queue.items[0];
    assert!(item.candidates.len() >= 2);
    assert!(item.candidates_expanded);
    let art_cand = item
        .candidates
        .iter()
        .find(|c| c.has_artwork)
        .expect("alt candidate has legal artwork");
    assert_eq!(art_cand.artwork_provider.as_deref(), Some("Discogs"));
    assert_eq!(art_cand.subgenre.as_deref(), Some("Deep House"));

    let Response::UsePrepCandidateArtwork(staged) = api
        .handle(Request::UsePrepCandidateArtwork {
            track_id: item.track.id,
            candidate_index: art_cand.index,
            accept: false,
            expected_review: None,
        })
        .unwrap()
    else {
        panic!("expected updated prep item");
    };
    assert_eq!(staged.selected_candidate, item.selected_candidate);
    assert!(staged.artwork.proposed.is_some());
    assert_eq!(
        staged.candidates[art_cand.index as usize].artwork_hash,
        staged.artwork.proposed.as_ref().map(|p| p.artwork.clone())
    );

    // Setting a local cover clears any pending proposed_artwork so the queue does not stall.
    let Response::SetTrackArtwork(after_local) = api
        .handle(Request::SetTrackArtwork {
            track_id: item.track.id,
            image_base64: "/9j/4AAQSkY=".to_owned(),
        })
        .unwrap()
    else {
        panic!("expected SetTrackArtwork response");
    };
    assert!(after_local.artwork.is_some());
    assert!(after_local.proposed.is_none());
}

#[test]
fn fetch_prep_candidate_artwork_populates_shared_urls_and_marks_404_unavailable() {
    #[derive(Default)]
    struct PreviewArtProviders {
        art_calls: Mutex<u32>,
    }
    impl HttpClient for PreviewArtProviders {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            let body = if request
                .url
                .starts_with("https://api.discogs.com/database/search")
            {
                json!({ "results": [{ "id": 555 }, { "id": 556 }] }).to_string()
            } else if request.url == "https://api.discogs.com/releases/555" {
                json!({
                    "id": 555,
                    "title": "First Pressing",
                    "year": 2022,
                    "artists": [{ "name": "Some Artist" }],
                    "labels": [{ "name": "Club Label", "catno": "CL01" }],
                    "images": [{ "type": "primary", "uri": "https://i.discogs.com/shared.jpg" }],
                    "tracklist": [
                        { "type_": "track", "title": "Night Drive", "duration": "0:10" },
                        { "type_": "track", "title": "Night Drive (Extended Mix)", "duration": "0:15" }
                    ]
                })
                .to_string()
            } else if request.url == "https://api.discogs.com/releases/556" {
                json!({
                    "id": 556,
                    "title": "Missing Art Pressing",
                    "year": 2021,
                    "artists": [{ "name": "Some Artist" }],
                    "labels": [{ "name": "Club Label", "catno": "CL02" }],
                    "images": [{ "type": "primary", "uri": "https://i.discogs.com/missing-404.jpg" }],
                    "tracklist": [{ "type_": "track", "title": "Night Drive (Extended Mix)", "duration": "0:05" }]
                })
                .to_string()
            } else {
                return Ok(HttpResponse {
                    status: 404,
                    body: String::new(),
                    retry_after_s: None,
                });
            };
            Ok(HttpResponse {
                status: 200,
                body,
                retry_after_s: None,
            })
        }

        fn fetch_bytes(
            &self,
            request: &HttpRequest,
            _limit: u64,
        ) -> Result<BytesResponse, Unreachable> {
            *self.art_calls.lock().unwrap() += 1;
            if request.url == "https://i.discogs.com/shared.jpg" {
                return Ok(BytesResponse {
                    status: 200,
                    body: vec![0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F'],
                });
            }
            Ok(BytesResponse {
                status: 404,
                body: Vec::new(),
            })
        }
    }

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_fetch_preview_art");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    Signal::tone(330.0, 10.0, -9.0, 44_100)
        .write_wav(&music.join("Some Artist - Night Drive.wav"))
        .unwrap();

    let creds = Arc::new(MemoryCredentialStore::new());
    creds
        .write(
            resonance_core::providers::ProviderKey::Discogs,
            "fixture-key",
        )
        .unwrap();
    let http = Arc::new(PreviewArtProviders::default());
    let mut db = Database::open(&root.join("library.db")).unwrap();
    db.set_artwork_folder(root.join("artwork"));
    let api = Api::new_silent(db)
        .with_credential_store(creds as Arc<dyn CredentialStore>)
        .with_http_client(Arc::clone(&http) as Arc<dyn HttpClient>);

    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .unwrap();
    api.handle(Request::IdentifyOnline {
        track_ids: vec![],
        only_selected: false,
    })
    .unwrap();
    wait_until(Duration::from_secs(10), || {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            return false;
        };
        snapshot
            .recent
            .iter()
            .any(|job| job.kind == "identify_online" && job.status == ActivityJobStatus::Completed)
    });

    let Response::PrepQueue(initial) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected prep queue");
    };
    let item = &initial.items[0];
    assert!(item.candidates.len() >= 3);
    assert!(item.artwork.proposed.is_some());

    let miss_idx = item
        .candidates
        .iter()
        .find(|c| c.album.as_deref() == Some("Missing Art Pressing"))
        .expect("404 candidate present")
        .index;
    let shared_indices: Vec<u32> = item
        .candidates
        .iter()
        .filter(|c| c.album.as_deref() == Some("First Pressing"))
        .map(|c| c.index)
        .collect();
    assert_eq!(shared_indices.len(), 2);

    // Fetch one candidate with the shared URL and confirm both matching candidates carry artwork_hash.
    let Response::FetchPrepCandidateArtwork(fetched) = api
        .handle(Request::FetchPrepCandidateArtwork {
            track_id: item.track.id,
            candidate_index: shared_indices[1],
        })
        .unwrap()
    else {
        panic!("expected FetchPrepCandidateArtwork");
    };
    let first_hash = fetched.candidates[shared_indices[0] as usize]
        .artwork_hash
        .clone()
        .expect("shared candidate 0 populated");
    assert_eq!(
        fetched.candidates[shared_indices[1] as usize]
            .artwork_hash
            .as_deref(),
        Some(first_hash.as_str())
    );
    assert!(
        fetched.candidates[miss_idx as usize].has_artwork,
        "uncached 404 candidate still reports has_artwork until fetched"
    );

    // Fetch the 404 candidate -> marks has_artwork false and performs zero extra calls on retry.
    let Response::FetchPrepCandidateArtwork(after_miss) = api
        .handle(Request::FetchPrepCandidateArtwork {
            track_id: item.track.id,
            candidate_index: miss_idx,
        })
        .unwrap()
    else {
        panic!("expected FetchPrepCandidateArtwork");
    };
    assert!(!after_miss.candidates[miss_idx as usize].has_artwork);
    assert!(
        after_miss.candidates[miss_idx as usize]
            .artwork_hash
            .is_none()
    );

    let Response::PrepQueue(refreshed) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected refreshed prep queue");
    };
    assert_eq!(
        refreshed.items[0].candidates[shared_indices[1] as usize]
            .artwork_hash
            .as_deref(),
        Some(first_hash.as_str())
    );
    assert!(!refreshed.items[0].candidates[miss_idx as usize].has_artwork);
}

#[test]
fn prep_visible_candidates_stay_stable_when_switching_to_lower_confidence_match() {
    let mut db = Database::open_in_memory().unwrap();
    let source = db.source("D:\\Club").unwrap();
    db.record_scan(
        source,
        &[resonance_core::library::ScannedFile {
            path: "Some Artist - Night Drive.wav".to_owned(),
            size_bytes: 2_000_000,
            modified_ns: 1_700_000_000_000_000_000,
            format: resonance_core::library::AudioFormat::Wav,
            audio: Some(resonance_core::library::AudioFacts {
                duration_ms: 10_000,
                sample_rate: Some(44_100),
                channels: Some(2),
                bit_depth: Some(16),
                bitrate_kbps: None,
            }),
            tags: vec![
                (
                    resonance_core::library::TagField::Artist,
                    "Some Artist".to_owned(),
                ),
                (
                    resonance_core::library::TagField::Title,
                    "Night Drive".to_owned(),
                ),
            ],
        }],
    )
    .unwrap();

    let candidates = vec![
        json!({
            "provider": "discogs",
            "providers": ["discogs"],
            "confidence": 0.96,
            "audio_matched": false,
            "artist": "Some Artist",
            "title": "Night Drive",
            "album": "First EP",
            "url": "https://www.discogs.com/release/1",
            "evidence": {
                "basis": "tags_and_duration",
                "providers": ["discogs"],
                "primary_provider": "discogs",
                "catno_matched": false,
                "isrc_matched": false,
                "url": "https://www.discogs.com/release/1"
            }
        }),
        json!({
            "provider": "musicbrainz",
            "providers": ["musicbrainz"],
            "confidence": 0.72,
            "audio_matched": false,
            "artist": "Some Artist",
            "title": "Night Drive",
            "album": "Second EP",
            "url": "https://musicbrainz.org/release/rel-2",
            "evidence": {
                "basis": "tags_and_duration",
                "providers": ["musicbrainz"],
                "primary_provider": "musicbrainz",
                "catno_matched": false,
                "isrc_matched": false,
                "url": "https://musicbrainz.org/release/rel-2"
            }
        }),
        json!({
            "provider": "discogs",
            "providers": ["discogs"],
            "confidence": 0.60,
            "audio_matched": false,
            "artist": "Some Artist",
            "title": "Night Drive",
            "album": "Below Cutoff EP",
            "url": "https://www.discogs.com/release/3",
            "evidence": {
                "basis": "tags_and_duration",
                "providers": ["discogs"],
                "primary_provider": "discogs",
                "catno_matched": false,
                "isrc_matched": false,
                "url": "https://www.discogs.com/release/3"
            }
        }),
    ];

    db.record_identification(&resonance_db::StoredIdentification {
        track_id: 1,
        online: true,
        confidence: Some(0.96),
        audio_matched: false,
        known_json: json!({ "artist": "Some Artist", "title": "Night Drive" }).to_string(),
        best_json: Some(candidates[0].to_string()),
        steps_json: "[]".to_owned(),
        candidates_json: json!(candidates).to_string(),
        selected_candidate: 0,
        updated_ns: 0,
    })
    .unwrap();

    let api = Api::new_silent(db);
    let Response::PrepQueue(queue) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected prep queue");
    };
    assert_eq!(queue.items.len(), 1);
    assert_eq!(
        queue.items[0]
            .candidates
            .iter()
            .map(|c| c.index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );

    let Response::SelectPrepCandidate(switched) = api
        .handle(Request::SelectPrepCandidate {
            track_id: queue.items[0].track.id,
            candidate_index: 1,
            expected_review: None,
        })
        .unwrap()
    else {
        panic!("expected switched prep item");
    };
    assert_eq!(switched.selected_candidate, 1);
    assert_eq!(
        switched
            .candidates
            .iter()
            .map(|c| c.index)
            .collect::<Vec<_>>(),
        vec![0, 1],
        "switching to a lower-confidence match must not unhide sub-0.65 candidates"
    );
}

#[test]
fn prep_item_preserves_needs_url_even_when_filename_proposals_exist() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_needs_url_filename_proposals");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    Signal::tone(330.0, 2.0, -9.0, 44_100)
        .write_wav(&music.join("Artist B - Mystery Cut.wav"))
        .unwrap();

    #[derive(Default)]
    struct EmptyProviders;
    impl HttpClient for EmptyProviders {
        fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            Ok(HttpResponse {
                status: 404,
                body: String::new(),
                retry_after_s: None,
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

    let api = Api::new_silent(Database::open_in_memory().unwrap())
        .with_credential_store(Arc::new(MemoryCredentialStore::new()) as Arc<dyn CredentialStore>)
        .with_http_client(Arc::new(EmptyProviders) as Arc<dyn HttpClient>);
    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .unwrap();
    api.handle(Request::IdentifyOnline {
        track_ids: vec![],
        only_selected: false,
    })
    .unwrap();
    wait_until(Duration::from_secs(10), || {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            return false;
        };
        snapshot
            .recent
            .iter()
            .any(|job| job.kind == "identify_online" && job.status == ActivityJobStatus::Completed)
    });

    let Response::PrepQueue(queue) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: None,
        })
        .unwrap()
    else {
        panic!("expected prep queue");
    };
    assert_eq!(queue.items.len(), 1);
    let item = &queue.items[0];
    assert!(item.pending_fields > 0);
    assert!(
        item.needs_url,
        "unresolved track with local filename proposals must still set needs_url"
    );
}

#[test]
fn only_selected_online_lookup_pre_fingerprints_reidentifies_scoped_prep_queue_and_atomic_partial_accept()
 {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_only_selected_and_scoped_prep");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    Signal::tone(330.0, 2.0, -9.0, 44_100)
        .write_wav(&music.join("Some Artist - Night Drive.wav"))
        .unwrap();
    Signal::tone(440.0, 2.0, -9.0, 44_100)
        .write_wav(&music.join("Other Artist - B Side.wav"))
        .unwrap();

    #[derive(Default)]
    struct SingleTrackProviders;
    impl HttpClient for SingleTrackProviders {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            let body = if request
                .url
                .starts_with("https://musicbrainz.org/ws/2/recording")
                && request.params.iter().any(|(_, v)| v.contains("Night"))
            {
                json!({ "recordings": [{
                    "id": "b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d", "score": 100,
                    "title": "Night Drive", "length": 2_000,
                    "artist-credit": [{ "name": "Some Artist" }],
                    "releases": [{
                        "id": "c0a1b2c3-0000-4000-8000-000000000002",
                        "title": "Night Drive EP",
                        "status": "Official",
                        "date": "2022",
                        "cover-art-archive": { "front": true }
                    }]
                }]})
            } else if request
                .url
                .starts_with("https://musicbrainz.org/ws/2/release")
            {
                json!({ "releases": [{
                    "id": "c0a1b2c3-0000-4000-8000-000000000002",
                    "title": "Night Drive EP",
                    "status": "Official",
                    "date": "2022",
                    "cover-art-archive": { "front": true },
                    "label-info": [{ "catalog-number": "CWX01", "label": { "name": "Club Wax" } }]
                }]})
            } else {
                return Ok(HttpResponse {
                    status: 404,
                    body: String::new(),
                    retry_after_s: None,
                });
            };
            Ok(HttpResponse {
                status: 200,
                body: body.to_string(),
                retry_after_s: None,
            })
        }

        fn fetch_bytes(
            &self,
            _request: &HttpRequest,
            _limit: u64,
        ) -> Result<BytesResponse, Unreachable> {
            // No image bytes, so the candidate shows as having no cover.
            Ok(BytesResponse {
                status: 404,
                body: Vec::new(),
            })
        }
    }

    let api = Api::new_silent(Database::open_in_memory().unwrap())
        .with_credential_store(Arc::new(MemoryCredentialStore::new()) as Arc<dyn CredentialStore>)
        .with_http_client(Arc::new(SingleTrackProviders) as Arc<dyn HttpClient>);

    api.handle(Request::Scan {
        folder: music.to_string_lossy().into_owned(),
    })
    .unwrap();

    let Response::Tracks(page) = api
        .handle(Request::Tracks {
            offset: 0,
            limit: 10,
            query: None,
        })
        .unwrap()
    else {
        panic!("expected tracks page");
    };
    let night_id = page
        .tracks
        .iter()
        .find(|t| t.location.contains("Night Drive"))
        .expect("Night Drive scanned")
        .id;
    let other_id = page
        .tracks
        .iter()
        .find(|t| t.location.contains("B Side"))
        .expect("B Side scanned")
        .id;

    // Empty only_selected lookup is refused cleanly.
    let err = api
        .handle(Request::IdentifyOnline {
            track_ids: vec![],
            only_selected: true,
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);

    // Before online lookup runs, scoped PrepQueue shows only Night Drive and online_checked == false.
    let Response::PrepQueue(before) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: true,
            track_ids: Some(vec![night_id, night_id]),
        })
        .unwrap()
    else {
        panic!("expected scoped prep queue");
    };
    assert_eq!(before.items.len(), 1);
    assert_eq!(before.items[0].track.id, night_id);
    assert!(!before.items[0].online_checked);
    assert!(!before.items[0].has_unwritten);

    // Targeted single-track lookup runs OnDemand and deduplicates track_ids.
    let Response::IdentifyOnline(started) = api
        .handle(Request::IdentifyOnline {
            track_ids: vec![night_id, night_id],
            only_selected: true,
        })
        .unwrap()
    else {
        panic!("expected ActivitySnapshot");
    };
    let job = started
        .active
        .iter()
        .chain(started.recent.iter())
        .find(|j| j.kind == "identify_online")
        .expect("online job present");
    assert_eq!(job.label, "Look up 1 track online");
    assert_eq!(job.tier, ActivityJobTier::OnDemand);
    assert_eq!(job.total_items, Some(1));

    wait_until(Duration::from_secs(10), || {
        let Response::JobsSnapshot(snapshot) = api.handle(Request::JobsSnapshot).unwrap() else {
            return false;
        };
        snapshot
            .recent
            .iter()
            .any(|j| j.kind == "identify_online" && j.status == ActivityJobStatus::Completed)
    });

    // Only Night Drive ran online; B Side was left completely untouched.
    let Response::PrepQueue(both) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: Some(vec![night_id, other_id]),
        })
        .unwrap()
    else {
        panic!("expected both tracks");
    };
    assert_eq!(both.items.len(), 2);
    let night_before = both.items.iter().find(|i| i.track.id == night_id).unwrap();
    let other_before = both.items.iter().find(|i| i.track.id == other_id).unwrap();
    assert!(night_before.online_checked);
    assert!(!other_before.online_checked);

    // Partial atomic accept: take Album only, skip remaining pending proposals in one commit.
    let Response::AcceptPrepItems(accepted) = api
        .handle(Request::AcceptPrepItems {
            track_ids: vec![night_id],
            fields: vec![TagName::Album],
            candidate_index: None,
            skip_remaining: true,
            include_artwork: Some(false),
        })
        .unwrap()
    else {
        panic!("expected accepted subset");
    };
    assert_eq!(accepted.tracks, 1);
    assert_eq!(accepted.fields, 1);

    // Even with pending_fields == 0 and proposals_only == true, explicit track_ids keeps Night Drive
    // visible in Prep, with has_unwritten == true and dirty_on_disk only on Album.
    let Response::PrepQueue(after) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: true,
            track_ids: Some(vec![night_id]),
        })
        .unwrap()
    else {
        panic!("expected updated track");
    };
    assert_eq!(after.items.len(), 1);
    let track1 = &after.items[0];
    assert_eq!(track1.pending_fields, 0);
    assert!(track1.has_unwritten);
    assert!(
        track1
            .fields
            .iter()
            .find(|f| f.field == TagName::Album)
            .is_some_and(|f| f.dirty_on_disk)
    );
    assert!(
        !track1
            .fields
            .iter()
            .find(|f| f.field == TagName::Label)
            .is_some_and(|f| f.dirty_on_disk)
    );
}

#[test]
fn reviewed_prep_actions_reject_stale_tokens_and_succeed_with_current_token() {
    #[derive(Default)]
    struct ReviewArtProviders;
    impl HttpClient for ReviewArtProviders {
        fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, Unreachable> {
            Ok(HttpResponse {
                status: 404,
                body: String::new(),
                retry_after_s: None,
            })
        }

        fn fetch_bytes(
            &self,
            request: &HttpRequest,
            _limit: u64,
        ) -> Result<BytesResponse, Unreachable> {
            if request.url == "https://i.discogs.com/first-cover.jpg" {
                return Ok(BytesResponse {
                    status: 200,
                    body: vec![0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'1'],
                });
            }
            if request.url == "https://i.discogs.com/review-cover.jpg" {
                return Ok(BytesResponse {
                    status: 200,
                    body: vec![0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'2'],
                });
            }
            Ok(BytesResponse {
                status: 404,
                body: Vec::new(),
            })
        }
    }

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_review_token");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    let wav_path = music.join("Some Artist - Night Drive.wav");
    Signal::tone(330.0, 2.0, -9.0, 44_100)
        .write_wav(&wav_path)
        .unwrap();

    let mut db = Database::open(&root.join("library.db")).unwrap();
    db.set_artwork_folder(root.join("artwork"));
    resonance_library::scan::scan(&mut db, &music, &[]).unwrap();
    let track_id = db.tracks().unwrap()[0].track_id;
    let bytes_before = fs::read(&wav_path).unwrap();

    let candidates = vec![
        json!({
            "provider": "discogs",
            "providers": ["discogs"],
            "confidence": 0.96,
            "audio_matched": false,
            "artist": "Some Artist",
            "title": "Night Drive",
            "album": "First EP",
            "label": "Club Wax",
            "artwork_url": "https://i.discogs.com/first-cover.jpg",
            "url": "https://www.discogs.com/release/1",
            "evidence": {
                "basis": "tags_and_duration",
                "providers": ["discogs"],
                "primary_provider": "discogs",
                "catno_matched": false,
                "isrc_matched": false,
                "url": "https://www.discogs.com/release/1"
            }
        }),
        json!({
            "provider": "discogs",
            "providers": ["discogs"],
            "confidence": 0.90,
            "audio_matched": false,
            "artist": "Some Artist",
            "title": "Night Drive",
            "album": "Second EP",
            "label": "Night Wax",
            "artwork_url": "https://i.discogs.com/review-cover.jpg",
            "url": "https://www.discogs.com/release/2",
            "evidence": {
                "basis": "tags_and_duration",
                "providers": ["discogs"],
                "primary_provider": "discogs",
                "catno_matched": false,
                "isrc_matched": false,
                "url": "https://www.discogs.com/release/2"
            }
        }),
    ];

    db.record_identification(&resonance_db::StoredIdentification {
        track_id,
        online: true,
        confidence: Some(0.96),
        audio_matched: false,
        known_json: json!({ "artist": "Some Artist", "title": "Night Drive" }).to_string(),
        best_json: Some(candidates[0].to_string()),
        steps_json: "[]".to_owned(),
        candidates_json: json!(candidates).to_string(),
        selected_candidate: 0,
        updated_ns: 0,
    })
    .unwrap();
    db.store_proposals(
        track_id,
        &[resonance_core::staging::Proposal {
            field: resonance_core::library::TagField::Album,
            value: "First EP".to_owned(),
            provider: "discogs".to_owned(),
            confidence: 0.96,
            audio_matched: false,
        }],
    )
    .unwrap();

    let api = Api::new_silent(db)
        .with_credential_store(Arc::new(MemoryCredentialStore::new()) as Arc<dyn CredentialStore>)
        .with_http_client(Arc::new(ReviewArtProviders) as Arc<dyn HttpClient>);

    let Response::PrepQueue(queue) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: Some(vec![track_id]),
        })
        .unwrap()
    else {
        panic!("expected prep queue");
    };
    let initial = &queue.items[0];
    let token_0 = initial.review_token.clone();

    let stale_err = |res: Result<Response, resonance_api::error::ApiError>| {
        assert_eq!(res.unwrap_err().code, ErrorCode::BadRequest);
    };
    stale_err(api.handle(Request::SelectPrepCandidate {
        track_id,
        candidate_index: 1,
        expected_review: Some("stale-review".to_owned()),
    }));
    stale_err(api.handle(Request::UsePrepCandidateArtwork {
        track_id,
        candidate_index: 1,
        accept: false,
        expected_review: Some("stale-review".to_owned()),
    }));
    stale_err(api.handle(Request::AcceptAndWrite {
        track_ids: vec![track_id],
        fields: None,
        include_artwork: false,
        expected_review: Some("stale-review".to_owned()),
    }));
    stale_err(api.handle(Request::AcceptAndWrite {
        track_ids: vec![track_id, track_id + 1],
        fields: None,
        include_artwork: false,
        expected_review: Some(token_0.clone()),
    }));
    assert_eq!(fs::read(&wav_path).unwrap(), bytes_before);

    let Response::SelectPrepCandidate(switched) = api
        .handle(Request::SelectPrepCandidate {
            track_id,
            candidate_index: 1,
            expected_review: Some(token_0.clone()),
        })
        .unwrap()
    else {
        panic!("expected switched prep item");
    };
    assert_eq!(switched.selected_candidate, 1);
    assert_ne!(switched.review_token, token_0);

    stale_err(api.handle(Request::AcceptAndWrite {
        track_ids: vec![track_id],
        fields: None,
        include_artwork: true,
        expected_review: Some(token_0),
    }));

    let Response::UsePrepCandidateArtwork(with_art) = api
        .handle(Request::UsePrepCandidateArtwork {
            track_id,
            candidate_index: 0,
            accept: false,
            expected_review: Some(switched.review_token.clone()),
        })
        .unwrap()
    else {
        panic!("expected artwork update");
    };
    assert_eq!(with_art.selected_candidate, 1);
    assert_ne!(with_art.review_token, switched.review_token);
    assert!(with_art.artwork.proposed.is_some());

    stale_err(api.handle(Request::AcceptAndWrite {
        track_ids: vec![track_id],
        fields: None,
        include_artwork: true,
        expected_review: Some(switched.review_token),
    }));

    let Response::AcceptAndWrite(saved) = api
        .handle(Request::AcceptAndWrite {
            track_ids: vec![track_id],
            fields: None,
            include_artwork: true,
            expected_review: Some(with_art.review_token),
        })
        .unwrap()
    else {
        panic!("expected accept and write report");
    };
    assert_eq!(saved.files.written, 1, "{:?}", saved.files.failed);
    assert!(saved.files.failed.is_empty());
    assert_ne!(fs::read(&wav_path).unwrap(), bytes_before);
}

#[test]
fn prep_order_groups_by_stage_and_accept_reports_reverted_fields() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("api_prep_order_revert");
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(&music).unwrap();
    let first_wav = music.join("01 First.wav");
    Signal::tone(330.0, 2.0, -9.0, 44_100)
        .write_wav(&first_wav)
        .unwrap();
    Signal::tone(370.0, 2.0, -9.0, 44_100)
        .write_wav(&music.join("02 Second.wav"))
        .unwrap();
    Signal::tone(410.0, 2.0, -9.0, 44_100)
        .write_wav(&music.join("03 Third.wav"))
        .unwrap();

    let mut db = Database::open(&root.join("library.db")).unwrap();
    resonance_library::scan::scan(&mut db, &music, &[]).unwrap();
    let first_id = db.tracks().unwrap()[0].track_id;
    db.set_user_tags(
        &[first_id],
        &[(
            resonance_core::library::TagField::Artist,
            Some("File Artist".to_owned()),
        )],
    )
    .unwrap();
    let db = std::sync::Mutex::new(db);
    let initial_write = resonance_library::write_tags::apply_tag_writes(&db, &[first_id]).unwrap();
    assert_eq!(initial_write.written, 1);
    let mut db = db.into_inner().unwrap();

    db.set_user_tags(
        &[first_id],
        &[(
            resonance_core::library::TagField::Artist,
            Some("Manual Edit".to_owned()),
        )],
    )
    .unwrap();
    db.record_identification(&resonance_db::StoredIdentification {
        track_id: first_id,
        online: true,
        confidence: Some(0.98),
        audio_matched: false,
        known_json: json!({}).to_string(),
        best_json: None,
        steps_json: "[]".to_owned(),
        candidates_json: "[]".to_owned(),
        selected_candidate: 0,
        updated_ns: 0,
    })
    .unwrap();
    db.store_proposals(
        first_id,
        &[resonance_core::staging::Proposal {
            field: resonance_core::library::TagField::Artist,
            value: "File Artist".to_owned(),
            provider: "discogs".to_owned(),
            confidence: 0.98,
            audio_matched: false,
        }],
    )
    .unwrap();

    let api = Api::new_silent(db);

    let order = |by_stage| {
        let Response::PrepOrder(order) = api
            .handle(Request::PrepOrder {
                query: None,
                track_ids: None,
                by_stage,
            })
            .unwrap()
        else {
            panic!("expected prep order");
        };
        order
    };
    let plain = order(None);
    assert_eq!(plain.ids.len(), 3);
    assert_eq!(plain.ids[0], first_id);
    let counts = plain.stages;
    assert_eq!(
        counts.review + counts.not_searched + counts.no_match + counts.done,
        3
    );
    // Searched with no stored match: its waiting suggestion does not make it a match to review,
    // so the two tracks not yet searched come first and it comes last.
    assert_eq!(counts.no_match, 1);
    assert_eq!(order(Some(SortDirection::Asc)).ids[2], first_id);
    assert_eq!(order(Some(SortDirection::Desc)).ids[0], first_id);

    let Response::PrepQueue(rows) = api
        .handle(Request::PrepQueue {
            query: None,
            proposals_only: false,
            track_ids: Some(plain.ids[1..].to_vec()),
        })
        .unwrap()
    else {
        panic!("expected prep rows");
    };
    let row_ids: Vec<i64> = rows.items.iter().map(|item| item.track.id).collect();
    assert_eq!(row_ids, plain.ids[1..]);
    let Response::PrepTrackIds(no_match) = api
        .handle(Request::PrepTrackIds {
            query: None,
            track_ids: None,
            stage: Some(resonance_api::command::PrepStageName::NoMatch),
        })
        .unwrap()
    else {
        panic!("expected track ids");
    };
    assert!(no_match.contains(&first_id));

    let Response::AcceptAndWrite(reverted) = api
        .handle(Request::AcceptAndWrite {
            track_ids: vec![first_id],
            fields: Some(vec![resonance_api::command::TagName::Artist]),
            include_artwork: false,
            expected_review: None,
        })
        .unwrap()
    else {
        panic!("expected accept report");
    };
    assert_eq!(reverted.files.written, 0);
    assert_eq!(reverted.library_fields, 0);
    assert_eq!(reverted.reverted_fields, 1);
    assert!(reverted.files.operation_id.is_some());
}
