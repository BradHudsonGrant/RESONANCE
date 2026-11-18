//! Chromaprint fingerprints looked up on AcoustID, which answers with MusicBrainz recordings.
//! See acoustid.org/webservice.

use resonance_core::identify::Candidate;
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::{HttpRequest, Secret};
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text,
};
use crate::rate::RateLimit;

const ID: &str = "acoustid";
const LOOKUP_URL: &str = "https://api.acoustid.org/v2/lookup";
const META: &str = "recordings releasegroups compress";

/// How many fingerprints go in one request.
pub const BATCH_SIZE: usize = 10;

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

    fn lookup_batch(
        &self,
        queries: &[Query<'_>],
        lookup: &Lookup<'_>,
    ) -> Vec<Result<Vec<Candidate>, IdentifyError>> {
        if queries.len() < 2 {
            return queries
                .iter()
                .map(|query| self.lookup(query, lookup))
                .collect();
        }
        let mut results: Vec<Result<Vec<Candidate>, IdentifyError>> =
            Vec::with_capacity(queries.len());
        for chunk in queries.chunks(BATCH_SIZE) {
            match lookup_chunk(chunk, lookup) {
                Ok(found) => results.extend(found.into_iter().map(Ok)),
                Err(error) => {
                    let message = error.to_string();
                    results.push(Err(error));
                    for _ in 1..chunk.len() {
                        results.push(Err(IdentifyError::Unreachable {
                            provider: ID,
                            reason: message.clone(),
                        }));
                    }
                }
            }
        }
        results
    }
}

// The batch form numbers each fingerprint and its duration, and the answer lists results under
// `fingerprints` with the same index.
fn lookup_chunk(
    chunk: &[Query<'_>],
    lookup: &Lookup<'_>,
) -> Result<Vec<Vec<Candidate>>, IdentifyError> {
    let mut request = HttpRequest::post_form(LOOKUP_URL)
        .param("format", "json")
        .param("meta", META);
    for (index, query) in chunk.iter().enumerate() {
        if let Query::Fingerprint {
            fingerprint,
            duration_s,
        } = query
        {
            request = request
                .param(&format!("duration.{index}"), duration_s.to_string())
                .param(&format!("fingerprint.{index}"), *fingerprint);
        }
    }
    let request = keyed(request, lookup)?;
    let mut found = vec![Vec::new(); chunk.len()];
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(found);
    };
    let answer = checked(parse_json(ID, &body)?)?;
    for entry in answer
        .get("fingerprints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let index = match entry.get("index") {
            Some(Value::Number(number)) => number.as_u64(),
            Some(Value::String(text)) => text.parse().ok(),
            _ => None,
        };
        if let Some(slot) = index.and_then(|index| found.get_mut(index as usize)) {
            *slot = candidates(entry.get("results"));
        }
    }
    Ok(found)
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
            "the application key was refused. Check it in Settings"
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
