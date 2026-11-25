//! Chromaprint fingerprints looked up on AcoustID, which answers with MusicBrainz recordings.
//! See acoustid.org/webservice.

use resonance_core::identify::{AudioMatchSource, Candidate};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::{HttpRequest, Secret};
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, ResponseDisposition, parse_json,
    text,
};
use crate::rate::RateLimit;

pub(crate) const ID: &str = "acoustid";
const LOOKUP_URL: &str = "https://api.acoustid.org/v2/lookup";
const META: &str = "recordings releasegroups compress";

pub struct AcoustId;

impl MetadataProvider for AcoustId {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        kind == QueryKind::Fingerprint
    }

    fn requires_key(&self, _kind: QueryKind) -> Option<ProviderKey> {
        Some(ProviderKey::AcoustId)
    }

    fn key(&self) -> Option<ProviderKey> {
        Some(ProviderKey::AcoustId)
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit::per_second(3)
    }

    fn classify_response(&self, status: u16, body: &str) -> ResponseDisposition {
        if status == 400 && body.contains("\"error\"") {
            return ResponseDisposition::PassThrough;
        }
        match status {
            200 | 404 => ResponseDisposition::Cacheable,
            429 | 503 => ResponseDisposition::Retryable,
            _ => ResponseDisposition::Error,
        }
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let Query::Fingerprint {
            fingerprint,
            duration_s,
        } = query
        else {
            return Ok(Vec::new());
        };
        let request = keyed(
            HttpRequest::post_form(LOOKUP_URL)
                .param("format", "json")
                .param("meta", META)
                .param("duration", duration_s.to_string())
                .param("fingerprint", *fingerprint),
            lookup,
        )?;
        let Some(body) = lookup.fetch(&request)? else {
            return Ok(Vec::new());
        };
        let answer = checked(parse_json(ID, &body)?)?;
        Ok(candidates(answer.get("results")))
    }
}

fn keyed(request: HttpRequest, lookup: &Lookup<'_>) -> Result<HttpRequest, IdentifyError> {
    let key = lookup.key().ok_or(IdentifyError::Refused {
        provider: ID,
        reason: "no application key",
    })?;
    Ok(request.with_secret(Secret::FormField {
        name: "client",
        value: key.expose().to_owned(),
    }))
}

fn checked(answer: Value) -> Result<Value, IdentifyError> {
    if answer.get("status").and_then(Value::as_str) == Some("ok") {
        return Ok(answer);
    }
    // Code 4 is an unknown application key; the message never repeats the key.
    let code = answer
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_i64);
    Err(IdentifyError::Refused {
        provider: ID,
        reason: if code == Some(4) {
            "the application key was refused. Check it in Settings (use an Application API key, not a User key)"
        } else {
            "the lookup was refused"
        },
    })
}

fn candidates(results: Option<&Value>) -> Vec<Candidate> {
    let mut found = Vec::new();
    for result in results.and_then(Value::as_array).into_iter().flatten() {
        let score = result.get("score").and_then(Value::as_f64);
        for recording in result
            .get("recordings")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(id) = text(recording, "id") else {
                continue;
            };
            found.push(Candidate {
                provider: ID.to_owned(),
                artist: artist_names(recording.get("artists")),
                title: text(recording, "title"),
                album: recording
                    .get("releasegroups")
                    .and_then(Value::as_array)
                    .and_then(|groups| groups.first())
                    .and_then(|group| text(group, "title")),
                duration_ms: recording
                    .get("duration")
                    .and_then(Value::as_f64)
                    .map(|seconds| (seconds * 1000.0).round() as u64),
                recording_id: Some(id),
                fingerprint_score: score,
                audio_source: score
                    .filter(|s| *s >= resonance_core::identify::FINGERPRINT_MATCH_SCORE)
                    .map(|_| AudioMatchSource::Fingerprint),
                ..Candidate::default()
            });
        }
    }
    found
}

/// `Artist A feat. Artist B` from a list of artists with their join phrases.
pub(crate) fn artist_names(artists: Option<&Value>) -> Option<String> {
    let artists = artists.and_then(Value::as_array)?;
    let mut joined = String::new();
    for (index, artist) in artists.iter().enumerate() {
        let name = text(artist, "name")
            .or_else(|| artist.get("artist").and_then(|inner| text(inner, "name")))?;
        joined.push_str(&name);
        match artist.get("joinphrase").and_then(Value::as_str) {
            Some(phrase) => joined.push_str(phrase),
            None if index + 1 < artists.len() => joined.push_str(", "),
            None => {}
        }
    }
    let joined = joined.trim().to_owned();
    (!joined.is_empty()).then_some(joined)
}
