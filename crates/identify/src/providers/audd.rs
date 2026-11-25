//! Opt-in paid audio recognition through AudD (api.audd.io), run only after free steps leave
//! a track uncertain and only when an AudD API token is configured in Settings.

use resonance_core::identify::{AudioMatchSource, Candidate, ReleaseKind, split_mix};
use resonance_core::identity::encode_base64_standard;
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::{HttpRequest, Secret};
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, ResponseDisposition, parse_json,
    text, year_in,
};
use crate::rate::RateLimit;

pub(crate) const ID: &str = "audd";
const ENDPOINT: &str = "https://api.audd.io/";

pub struct AudD;

impl MetadataProvider for AudD {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        kind == QueryKind::AudioExcerpt
    }

    fn requires_key(&self, _kind: QueryKind) -> Option<ProviderKey> {
        Some(ProviderKey::AudD)
    }

    fn key(&self) -> Option<ProviderKey> {
        Some(ProviderKey::AudD)
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit::per_second(5)
    }

    fn classify_response(&self, status: u16, body: &str) -> ResponseDisposition {
        if status == 200 {
            if let Ok(json) = serde_json::from_str::<Value>(body)
                && json.get("status").and_then(Value::as_str) == Some("error")
            {
                return ResponseDisposition::PassThrough;
            }
            return ResponseDisposition::Cacheable;
        }
        match status {
            429 | 503 => ResponseDisposition::Retryable,
            _ => ResponseDisposition::Error,
        }
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let Query::AudioExcerpt {
            wav_bytes,
            audio_hash_hex,
        } = query
        else {
            return Ok(Vec::new());
        };
        let Some(token) = lookup.key() else {
            return Err(IdentifyError::Refused {
                provider: ID,
                reason: "no API token",
            });
        };
        let encoded = encode_base64_standard(wav_bytes);
        let request = HttpRequest::post_form(ENDPOINT)
            .param("return", "apple_music,deezer,musicbrainz")
            .param("audio", encoded)
            .with_cache_id(audio_hash_hex.to_owned())
            .with_secret(Secret::FormField {
                name: "api_token",
                value: token.expose().to_owned(),
            });
        let Some(body) = lookup.fetch(&request)? else {
            return Ok(Vec::new());
        };
        let answer = parse_json(ID, &body)?;
        if answer.get("status").and_then(Value::as_str) == Some("error") {
            return Err(IdentifyError::Refused {
                provider: ID,
                reason: error_reason(&answer),
            });
        }
        Ok(parse_result(&answer).into_iter().collect())
    }
}

/// What AudD's error code means for the user. Codes 900 to 902 are about the token and its
/// allowance; 300 to 700 mean the excerpt could not be read.
fn error_reason(answer: &Value) -> &'static str {
    match answer.pointer("/error/error_code").and_then(Value::as_u64) {
        Some(900..=902) => {
            "the API token was refused or has no requests left. Check it in Settings"
        }
        Some(300..=700) => "the audio excerpt could not be read",
        _ => "AudD could not answer",
    }
}

fn parse_result(answer: &Value) -> Option<Candidate> {
    let result = answer.get("result").filter(|v| v.is_object())?;
    let raw_title = text(result, "title")?;
    let (title, mix) = split_mix(&raw_title);
    let apple = result.get("apple_music");
    let deezer = result.get("deezer");
    let isrc = apple
        .and_then(|a| text(a, "isrc"))
        .or_else(|| deezer.and_then(|d| text(d, "isrc")));
    let duration_ms = apple
        .and_then(|a| a.get("durationInMillis"))
        .and_then(Value::as_u64)
        .or_else(|| {
            deezer
                .and_then(|d| d.get("duration"))
                .and_then(Value::as_u64)
                .map(|s| s * 1000)
        });
    let recording_id = result
        .get("musicbrainz")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|mb| text(mb, "id"));

    Some(Candidate {
        provider: ID.to_owned(),
        artist: text(result, "artist"),
        title: Some(title),
        mix,
        album: text(result, "album"),
        label: text(result, "label"),
        year: text(result, "release_date").as_deref().and_then(year_in),
        duration_ms,
        isrc,
        recording_id,
        url: text(result, "song_link"),
        artwork_url: None,
        artwork_provider: None,
        release_kind: ReleaseKind::Unknown,
        audio_source: Some(AudioMatchSource::Recogniser),
        ..Candidate::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn audd_parses_result_as_recogniser_match_without_copying_third_party_art() {
        let payload = json!({
            "status": "success",
            "result": {
                "artist": "Some Artist",
                "title": "Night Drive (Extended Mix)",
                "album": "Night Drive EP",
                "release_date": "2021-06-11",
                "label": "Defected",
                "song_link": "https://lis.tn/NightDrive",
                "apple_music": { "isrc": "GBABC2100001", "durationInMillis": 412_000u64 },
                "deezer": { "preview": "https://cdns-preview.dzcdn.net/x.mp3" }
            }
        });
        let candidate = parse_result(&payload).unwrap();
        assert_eq!(candidate.audio_source, Some(AudioMatchSource::Recogniser));
        assert_eq!(candidate.isrc.as_deref(), Some("GBABC2100001"));
        assert_eq!(candidate.duration_ms, Some(412_000));
        assert_eq!(candidate.artwork_url, None);
    }

    #[test]
    fn an_error_names_the_token_only_when_the_token_is_at_fault() {
        let code = |code: u64| {
            error_reason(&json!({ "status": "error", "error": { "error_code": code } }))
        };
        assert!(code(901).contains("token"));
        assert_eq!(code(500), "the audio excerpt could not be read");
        assert_eq!(code(100), "AudD could not answer");
    }

    #[test]
    fn audd_error_payload_passes_through_and_reports_refused() {
        use std::sync::{Arc, Mutex};

        use crate::credentials::CredentialStore;
        use crate::rate::RateLimiter;
        use resonance_db::Database;

        let provider = AudD;
        let payload = json!({
            "status": "error",
            "error": { "error_code": 901, "error_message": "Invalid api_token" }
        })
        .to_string();
        assert_eq!(
            provider.classify_response(200, &payload),
            ResponseDisposition::PassThrough
        );

        let store = crate::credentials::MemoryCredentialStore::default();
        store
            .write(ProviderKey::AudD, "bad-token")
            .expect("store token");
        let secret = store.read(ProviderKey::AudD).unwrap();
        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let fetcher = crate::provider::Fetcher::new(Arc::new(StubHttp(payload.clone())), db);
        let limiter = RateLimiter::new(provider.rate_limit());
        let lookup = Lookup {
            provider: &provider,
            fetcher: &fetcher,
            limiter: &limiter,
            key: secret.as_ref(),
        };
        let err = provider
            .lookup(
                &Query::AudioExcerpt {
                    wav_bytes: b"RIFF",
                    audio_hash_hex: "abc",
                },
                &lookup,
            )
            .unwrap_err();
        assert!(matches!(err, IdentifyError::Refused { .. }), "{err:?}");
    }

    struct StubHttp(String);

    impl crate::http::HttpClient for StubHttp {
        fn send(
            &self,
            _request: &HttpRequest,
        ) -> Result<crate::http::HttpResponse, crate::http::Unreachable> {
            Ok(crate::http::HttpResponse {
                status: 200,
                body: self.0.clone(),
                retry_after_s: None,
            })
        }

        fn fetch_bytes(
            &self,
            _request: &HttpRequest,
            _limit: u64,
        ) -> Result<crate::http::BytesResponse, crate::http::Unreachable> {
            unreachable!()
        }
    }
}
