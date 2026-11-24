//! MusicBrainz: releases, ISRC lookup, URL relationships and recording search.
//! See musicbrainz.org/doc/MusicBrainz_API. Limited to 1 request/second with our repository
//! `User-Agent`.

use resonance_core::identify::{
    Candidate, KnownTags, ReleaseKind, ReleaseUrl, artist_search_terms, mix_family_matches,
    mix_search_hint, normalise_tokens, search_title_clean, split_mix,
};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text, year_in,
};
use crate::providers::acoustid::artist_names;
use crate::rate::RateLimit;

pub(crate) const ID: &str = "musicbrainz";
const API: &str = "https://musicbrainz.org/ws/2";
const SEARCH_LIMIT: &str = "10";
const RELEASE_LIMIT: usize = 10;
const MIN_SEARCH_SCORE: i64 = 50;
const COVER_ART_ARCHIVE: &str = "https://coverartarchive.org/release";

pub struct MusicBrainz;

impl MetadataProvider for MusicBrainz {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        matches!(
            kind,
            QueryKind::Recording
                | QueryKind::Text
                | QueryKind::Isrc
                | QueryKind::Url
                | QueryKind::Release
        )
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
            Query::Isrc(isrc) => isrc_lookup(isrc, lookup),
            Query::Url(url) => url_lookup(url, lookup),
            Query::Release(ReleaseUrl::MusicBrainzRelease(id)) => release_by_id(id, lookup),
            Query::Release(ReleaseUrl::MusicBrainzRecording(id)) => releases_of(id, lookup),
            _ => Ok(Vec::new()),
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
        .param("inc", "labels artist-credits release-groups")
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
    let mut ranked: Vec<Candidate> = releases
        .iter()
        .take(RELEASE_LIMIT)
        .map(|release| {
            let mut candidate = release_candidate(release);
            candidate.recording_id = Some(recording.to_owned());
            candidate
        })
        .collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.release_kind_rank()));
    Ok(ranked)
}

fn release_by_id(id: &str, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    if !is_mbid(id) {
        return Ok(Vec::new());
    }
    let request = HttpRequest::get(format!("{API}/release/{id}"))
        .param("inc", "recordings artist-credits labels release-groups")
        .param("fmt", "json");
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let release = parse_json(ID, &body)?;
    let base = release_candidate(&release);
    let mut out = Vec::new();
    for medium in release
        .get("media")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for track in medium
            .get("tracks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let recording = track.get("recording").unwrap_or(track);
            let mut cand = base.clone();
            cand.artist = artist_names(
                track
                    .get("artist-credit")
                    .or_else(|| recording.get("artist-credit")),
            )
            .or_else(|| artist_names(release.get("artist-credit")));
            let raw_title = text(track, "title").or_else(|| text(recording, "title"));
            let (title, mix) = match raw_title {
                Some(raw) => {
                    let (clean, extracted) = split_mix(&raw);
                    (Some(clean), extracted)
                }
                None => (None, None),
            };
            cand.title = title;
            if mix.is_some() {
                cand.mix = mix;
            }
            cand.duration_ms = track
                .get("length")
                .and_then(Value::as_u64)
                .or_else(|| recording.get("length").and_then(Value::as_u64));
            cand.recording_id = text(recording, "id");
            cand.track_number = text(track, "number");
            out.push(cand);
        }
    }
    Ok(out)
}

fn isrc_lookup(isrc: &str, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let clean: String = isrc
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if clean.len() != 12 {
        return Ok(Vec::new());
    }
    let request = HttpRequest::get(format!("{API}/isrc/{clean}"))
        .param("inc", "releases artist-credits")
        .param("fmt", "json");
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let recordings = answer
        .get("recordings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut followed_recording = false;
    for recording in &recordings {
        let base_cand = recording_to_candidate(recording, None);
        let mut enriched = Vec::new();
        if !followed_recording
            && (base_cand.label.is_none() || base_cand.artwork_url.is_none())
            && let Some(rec_id) = base_cand.recording_id.as_deref()
            && let Ok(releases) = releases_of(rec_id, lookup)
        {
            followed_recording = true;
            for mut rel in releases {
                if rel.artist.is_none() {
                    rel.artist.clone_from(&base_cand.artist);
                }
                if rel.title.is_none() {
                    rel.title.clone_from(&base_cand.title);
                }
                if rel.mix.is_none() {
                    rel.mix.clone_from(&base_cand.mix);
                }
                if rel.duration_ms.is_none() {
                    rel.duration_ms = base_cand.duration_ms;
                }
                if rel.artwork_url.is_none() && rel.release_id == base_cand.release_id {
                    rel.artwork_url.clone_from(&base_cand.artwork_url);
                    rel.artwork_provider.clone_from(&base_cand.artwork_provider);
                }
                rel.isrc = Some(clean.clone());
                enriched.push(rel);
            }
        }
        if enriched.is_empty() {
            let mut c = base_cand;
            c.isrc = Some(clean.clone());
            out.push(c);
        } else {
            out.extend(enriched);
        }
    }
    Ok(out)
}

fn url_lookup(resource: &str, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    if !resource.starts_with("https://") {
        return Ok(Vec::new());
    }
    let request = HttpRequest::get(format!("{API}/url"))
        .param("resource", resource)
        .param("inc", "recording-rels release-rels")
        .param("fmt", "json");
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let mut out = Vec::new();
    for rel in answer
        .get("relations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(recording) = rel.get("recording") {
            let mut c = recording_to_candidate(recording, None);
            if c.title.is_some() {
                out.push(c);
            } else if let Some(id) = c.recording_id.take() {
                out.extend(releases_of(&id, lookup)?);
            }
        } else if let Some(release) = rel.get("release")
            && let Some(id) = text(release, "id")
        {
            out.extend(release_by_id(&id, lookup)?);
        }
    }
    Ok(out)
}

fn release_candidate(release: &Value) -> Candidate {
    let label_info = release
        .get("label-info")
        .and_then(Value::as_array)
        .and_then(|labels| labels.first());
    let release_id = text(release, "id");
    let artwork_url = release_id
        .as_ref()
        .filter(|_| has_front_cover(release))
        .map(|id| format!("{COVER_ART_ARCHIVE}/{id}/front-500"));
    let artwork_provider = artwork_url.as_ref().map(|_| ID.to_owned());
    let release_kind = classify_release_group(release.get("release-group"));

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
        artwork_url,
        artwork_provider,
        release_kind,
        release_id,
        ..Candidate::default()
    }
}

fn classify_release_group(group: Option<&Value>) -> ReleaseKind {
    let Some(group) = group else {
        return ReleaseKind::Unknown;
    };
    if group
        .get("secondary-types")
        .and_then(Value::as_array)
        .is_some_and(|sec| sec.iter().any(|v| v.as_str() == Some("Compilation")))
    {
        return ReleaseKind::Compilation;
    }
    match group.get("primary-type").and_then(Value::as_str) {
        Some("Single" | "EP") => ReleaseKind::SingleOrEp,
        Some("Album") => ReleaseKind::Album,
        _ => ReleaseKind::Unknown,
    }
}

fn has_front_cover(release: &Value) -> bool {
    release
        .get("cover-art-archive")
        .and_then(|archive| archive.get("front"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn phrase(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn lucene_tokens_clause(field: &str, text_val: &str) -> Option<String> {
    let mut tokens = normalise_tokens(text_val);
    let non_and_count = tokens.iter().filter(|tok| tok.as_str() != "and").count();
    if non_and_count >= 2 {
        tokens.retain(|tok| tok.as_str() != "and");
    }
    if tokens.is_empty() {
        return None;
    }
    let parts: Vec<String> = tokens
        .into_iter()
        .map(|tok| format!("{field}:{tok}"))
        .collect();
    Some(format!("({})", parts.join(" AND ")))
}

fn search(known: &KnownTags, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let Some(raw_title) = &known.title else {
        return Ok(Vec::new());
    };
    let clean_title = search_title_clean(raw_title);
    let title = if clean_title.is_empty() {
        raw_title.as_str()
    } else {
        clean_title.as_str()
    };
    let mut recordings = Vec::new();
    if let Some(artist) = &known.artist {
        let exact_query = format!("recording:{} AND artist:{}", phrase(title), phrase(artist));
        recordings = run_recording_search(&exact_query, lookup)?;
        if recordings.is_empty() {
            let lead = artist_search_terms(artist)
                .into_iter()
                .nth(1)
                .unwrap_or_else(|| artist.clone());
            if let (Some(t_clause), Some(a_clause)) = (
                lucene_tokens_clause("recording", title),
                lucene_tokens_clause("artist", &lead),
            ) {
                let relaxed = format!("{t_clause} AND {a_clause}");
                if relaxed != exact_query {
                    recordings = run_recording_search(&relaxed, lookup)?;
                }
            }
        }
        if let Some(hint) = mix_search_hint(known.mix.as_deref()) {
            let has_mix = recordings.iter().any(|rec| {
                let rec_mix = text(rec, "title").and_then(|t| split_mix(&t).1);
                mix_family_matches(known.mix.as_deref(), rec_mix.as_deref())
            });
            if !has_mix {
                let lead = artist_search_terms(artist)
                    .into_iter()
                    .nth(1)
                    .unwrap_or_else(|| artist.clone());
                let title_with_hint = format!("{title} {hint}");
                if let (Some(t_clause), Some(a_clause)) = (
                    lucene_tokens_clause("recording", &title_with_hint),
                    lucene_tokens_clause("artist", &lead),
                ) {
                    let q = format!("{t_clause} AND {a_clause}");
                    for rec in run_recording_search(&q, lookup)? {
                        let id = text(&rec, "id");
                        if !recordings
                            .iter()
                            .any(|existing| id.is_some() && text(existing, "id") == id)
                        {
                            recordings.push(rec);
                        }
                    }
                }
            }
        }
    } else if normalise_tokens(title).len() >= 3
        && let Some(t_clause) = lucene_tokens_clause("recording", title)
    {
        recordings = run_recording_search(&t_clause, lookup)?;
    }

    Ok(recordings
        .iter()
        .filter(|recording| {
            recording
                .get("score")
                .and_then(Value::as_i64)
                .is_none_or(|score| score >= MIN_SEARCH_SCORE)
        })
        .map(|recording| recording_to_candidate(recording, known.catno.as_deref()))
        .collect())
}

fn run_recording_search(query: &str, lookup: &Lookup<'_>) -> Result<Vec<Value>, IdentifyError> {
    let request = HttpRequest::get(format!("{API}/recording"))
        .param("query", query)
        .param("fmt", "json")
        .param("limit", SEARCH_LIMIT);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    answer
        .get("recordings")
        .and_then(Value::as_array)
        .cloned()
        .ok_or(IdentifyError::Unexpected { provider: ID })
}

fn recording_to_candidate(recording: &Value, preferred_catno: Option<&str>) -> Candidate {
    let chosen_release = recording
        .get("releases")
        .and_then(Value::as_array)
        .and_then(|releases| pick_best_release(releases, preferred_catno));
    let mut candidate = chosen_release.map(release_candidate).unwrap_or_default();
    candidate.provider = ID.to_owned();
    candidate.artist = artist_names(recording.get("artist-credit"));
    if let Some(raw_title) = text(recording, "title") {
        let (clean, extracted_mix) = split_mix(&raw_title);
        candidate.title = Some(clean);
        if extracted_mix.is_some() {
            candidate.mix = extracted_mix;
        }
    }
    candidate.duration_ms = recording.get("length").and_then(Value::as_u64);
    candidate.recording_id = text(recording, "id");
    if candidate.isrc.is_none() {
        candidate.isrc = recording
            .get("isrcs")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    candidate
}

fn pick_best_release<'a>(
    releases: &'a [Value],
    preferred_catno: Option<&str>,
) -> Option<&'a Value> {
    releases.iter().max_by_key(|release| {
        let official = u8::from(
            release
                .get("status")
                .and_then(Value::as_str)
                .is_none_or(|s| s == "Official"),
        );
        let cat_match = u8::from(preferred_catno.is_some_and(|preferred| {
            release
                .get("label-info")
                .and_then(Value::as_array)
                .and_then(|labels| labels.first())
                .and_then(|info| info.get("catalog-number"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_some_and(|catno| catno.eq_ignore_ascii_case(preferred))
        }));
        let kind_rank = classify_release_group(release.get("release-group")).release_kind_rank();
        let has_cover = u8::from(has_front_cover(release));
        let has_year = u8::from(
            release
                .get("date")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(year_in)
                .is_some(),
        );
        (cat_match, official, kind_rank, has_cover, has_year)
    })
}

trait ReleaseKindExt {
    fn release_kind_rank(&self) -> u8;
}

impl ReleaseKindExt for ReleaseKind {
    fn release_kind_rank(&self) -> u8 {
        match self {
            ReleaseKind::SingleOrEp => 3,
            ReleaseKind::Album => 2,
            ReleaseKind::Unknown => 1,
            ReleaseKind::Compilation => 0,
        }
    }
}

impl ReleaseKindExt for Candidate {
    fn release_kind_rank(&self) -> u8 {
        self.release_kind.release_kind_rank()
    }
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

    #[test]
    fn release_picker_prefers_official_single_over_earlier_compilation() {
        let comp = serde_json::json!({
            "id": "rel-comp",
            "title": "Club Hits Compilation",
            "status": "Official",
            "release-group": { "primary-type": "Album", "secondary-types": ["Compilation"] }
        });
        let single = serde_json::json!({
            "id": "rel-single",
            "title": "Track EP",
            "status": "Official",
            "cover-art-archive": { "front": true },
            "release-group": { "primary-type": "EP" }
        });
        let rec = serde_json::json!({
            "id": "rec-1",
            "title": "Track",
            "releases": [comp, single]
        });
        let cand = recording_to_candidate(&rec, None);
        assert_eq!(cand.release_id.as_deref(), Some("rel-single"));
        assert_eq!(cand.artwork_provider.as_deref(), Some("musicbrainz"));
        assert_eq!(cand.release_kind, ReleaseKind::SingleOrEp);
    }
}
