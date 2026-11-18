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
    ActivityJobStatus, IdentifyStepOutcome, ProviderKeyKind, Request, Response,
};
use resonance_api::error::ErrorCode;
use resonance_db::Database;
use resonance_fixtures::signal::Signal;
use resonance_identify::credentials::{CredentialStore, MemoryCredentialStore};
use resonance_identify::http::{HttpClient, HttpRequest, HttpResponse, Unreachable};
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
            });
        };
        Ok(HttpResponse {
            status: 200,
            body: body.to_string(),
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
        .handle(Request::IdentifyOnline { track_ids: vec![] })
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
