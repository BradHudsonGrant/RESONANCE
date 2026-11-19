//! MusicBrainz: the releases behind a fingerprint match, and recording search by artist and
//! title. See musicbrainz.org/doc/MusicBrainz_API. The limit is one request a second, and every
//! request carries the `User-Agent` from `http`.

use resonance_core::identify::{Candidate, KnownTags};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text, year_in,
};
use crate::providers::acoustid::artist_names;
use crate::rate::RateLimit;

const ID: &str = "musicbrainz";
const API: &str = "https://musicbrainz.org/ws/2";
const SEARCH_LIMIT: &str = "10";
const RELEASE_LIMIT: usize = 10;
/// MusicBrainz scores search hits from 0 to 100. Below this they rarely share more than a word.
const MIN_SEARCH_SCORE: i64 = 50;
/// The Cover Art Archive's front cover of a release, 500 pixels wide: enough for any screen in
/// the app and small enough to embed in every file of the release.
const COVER_ART_ARCHIVE: &str = "https://coverartarchive.org/release";

pub struct MusicBrainz;

impl MetadataProvider for MusicBrainz {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        matches!(kind, QueryKind::Recording | QueryKind::Text)
    }

    fn requires_key(&self, _kind: QueryKind) -> Option<ProviderKey> {
        None
    }

    fn key(&self) -> Option<ProviderKey> {
        None
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit::per_second(1)
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        match query {
            Query::Recording { id } => releases_of(id, lookup),
            Query::Text(known) => search(known, lookup),
            Query::Fingerprint { .. } | Query::Release(_) => Ok(Vec::new()),
        }
    }
}

fn is_mbid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn releases_of(recording: &str, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    if !is_mbid(recording) {
        return Ok(Vec::new());
    }
    let request = HttpRequest::get(format!("{API}/release"))
        .param("recording", recording)
        .param("inc", "labels artist-credits")
        .param("fmt", "json")
        .param("limit", RELEASE_LIMIT.to_string());
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let releases = answer
        .get("releases")
        .and_then(Value::as_array)
        .ok_or(IdentifyError::Unexpected { provider: ID })?;
    Ok(releases
        .iter()
        .take(RELEASE_LIMIT)
        .map(|release| {
            let mut candidate = release_candidate(release);
            candidate.recording_id = Some(recording.to_owned());
            candidate
        })
        .collect())
}

fn release_candidate(release: &Value) -> Candidate {
    let label_info = release
        .get("label-info")
        .and_then(Value::as_array)
        .and_then(|labels| labels.first());
    let release_id = text(release, "id");
    Candidate {
        provider: ID.to_owned(),
        album: text(release, "title"),
        label: label_info
            .and_then(|info| info.get("label"))
            .and_then(|label| text(label, "name")),
        catno: label_info.and_then(|info| text(info, "catalog-number")),
        year: text(release, "date").as_deref().and_then(year_in),
        url: release_id
            .as_ref()
            .map(|id| format!("https://musicbrainz.org/release/{id}")),
        artwork_url: release_id
            .as_ref()
            .filter(|_| has_front_cover(release))
            .map(|id| format!("{COVER_ART_ARCHIVE}/{id}/front-500")),
        release_id,
        ..Candidate::default()
    }
}

/// Whether the Cover Art Archive holds a front cover for the release, as MusicBrainz reports it.
fn has_front_cover(release: &Value) -> bool {
    release
        .get("cover-art-archive")
        .and_then(|archive| archive.get("front"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

// Quoted Lucene phrases: inside quotes only a quote and a backslash need escaping.
fn phrase(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn search(known: &KnownTags, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let (Some(artist), Some(title)) = (&known.artist, &known.title) else {
        return Ok(Vec::new());
    };
    let query = format!("recording:{} AND artist:{}", phrase(title), phrase(artist));
    let request = HttpRequest::get(format!("{API}/recording"))
        .param("query", query)
        .param("fmt", "json")
        .param("limit", SEARCH_LIMIT);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let recordings = answer
        .get("recordings")
        .and_then(Value::as_array)
        .ok_or(IdentifyError::Unexpected { provider: ID })?;
    Ok(recordings
        .iter()
        .filter(|recording| {
            recording
                .get("score")
                .and_then(Value::as_i64)
                .is_none_or(|score| score >= MIN_SEARCH_SCORE)
        })
        .map(|recording| {
            let release = recording
                .get("releases")
                .and_then(Value::as_array)
                .and_then(|releases| releases.first());
            let mut candidate = release.map(release_candidate).unwrap_or_default();
            candidate.provider = ID.to_owned();
            candidate.artist = artist_names(recording.get("artist-credit"));
            candidate.title = text(recording, "title");
            candidate.duration_ms = recording.get("length").and_then(Value::as_u64);
            candidate.recording_id = text(recording, "id");
            candidate
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrases_escape_quotes_and_backslashes() {
        assert_eq!(phrase(r#"A "B" \ C"#), r#""A \"B\" \\ C""#);
    }

    #[test]
    fn only_mbids_are_followed() {
        assert!(is_mbid("b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d"));
        assert!(!is_mbid("../../release?x=1"));
    }
}
