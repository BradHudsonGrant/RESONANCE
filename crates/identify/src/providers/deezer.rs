//! Deezer public API (`api.deezer.com`): fast zero-key search, ISRC lookup, album label
//! enrichment and 30-second previews verified in memory. Deezer artwork is never stored or
//! embedded (`artwork_url` stays `None`), and Deezer BPM is never staged.

use resonance_core::identify::{
    Candidate, KnownTags, MixFamily, ReleaseKind, ReleaseUrl, artist_search_terms, classify_mix,
    mix_family_matches, mix_search_hint, normalise_tokens, search_fallback_text,
    search_title_clean, similarity, split_mix,
};
use std::time::Duration;

use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, ResponseDisposition, parse_json,
    text, year_in,
};
use crate::rate::RateLimit;

/// Shorter than the life of a signed preview link, so a cached answer's links still play.
const PREVIEW_LINK_LIFETIME: Duration = Duration::from_secs(10 * 60);

pub(crate) const ID: &str = "deezer";
const API: &str = "https://api.deezer.com";
const MAX_ENRICHED_HITS: usize = 2;

pub struct Deezer;

impl MetadataProvider for Deezer {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        matches!(kind, QueryKind::Text | QueryKind::Isrc | QueryKind::Release)
    }

    fn requires_key(&self, _kind: QueryKind) -> Option<ProviderKey> {
        None
    }

    fn key(&self) -> Option<ProviderKey> {
        None
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit::per_second(8)
    }

    /// Preview links in Deezer's answers are signed and stop working after about 20 minutes.
    fn cache_ttl(&self) -> Duration {
        PREVIEW_LINK_LIFETIME
    }

    fn classify_response(&self, status: u16, body: &str) -> ResponseDisposition {
        if status == 200 && body.contains("\"error\"") {
            if let Ok(json) = serde_json::from_str::<Value>(body)
                && let Some(err) = json.get("error")
            {
                return match err.get("code").and_then(Value::as_i64) {
                    Some(4) => ResponseDisposition::Retryable,
                    Some(800) => ResponseDisposition::Cacheable,
                    _ => ResponseDisposition::Error,
                };
            }
            return ResponseDisposition::Error;
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
        match query {
            Query::Text(known) => search(known, lookup),
            Query::Isrc(isrc) => isrc_lookup(isrc, lookup),
            Query::Release(ReleaseUrl::DeezerTrack(id)) => track_by_id(*id, lookup),
            Query::Release(ReleaseUrl::DeezerAlbum(id)) => album_tracks(*id, lookup),
            _ => Ok(Vec::new()),
        }
    }
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
    let artist_terms = known
        .artist
        .as_deref()
        .map(artist_search_terms)
        .unwrap_or_default();

    let mut raw_hits: Vec<Value> = Vec::new();
    if let Some(artist) = artist_terms.first() {
        let fielded = format!(
            "artist:\"{}\" track:\"{}\"",
            clean_q(artist),
            clean_q(title)
        );
        raw_hits = search_raw(&fielded, lookup)?;
    }

    let has_title_match = raw_hits.iter().any(|item| {
        text(item, "title").is_some_and(|t| similarity(&split_mix(&t).0, title) >= 0.70)
    });

    if !has_title_match {
        let relaxed_artist = artist_terms.get(1).or_else(|| artist_terms.first());
        let relaxed = match relaxed_artist {
            Some(artist) => format!("{} {}", clean_q(artist), clean_q(title)),
            None if normalise_tokens(title).len() >= 3 => clean_q(title),
            None => String::new(),
        };
        if !relaxed.is_empty() {
            append_search_hits(&relaxed, &mut raw_hits, lookup)?;
            let still_missing = !raw_hits.iter().any(|item| {
                text(item, "title").is_some_and(|t| similarity(&split_mix(&t).0, title) >= 0.70)
            });
            if still_missing {
                let folded = search_fallback_text(&relaxed);
                if !folded.is_empty() && folded != relaxed.to_lowercase() {
                    append_search_hits(&folded, &mut raw_hits, lookup)?;
                }
            }
        }
    }

    if let Some(hint) = mix_search_hint(known.mix.as_deref()) {
        let has_matching_mix = raw_hits.iter().any(|item| {
            text(item, "title").is_some_and(|t| {
                let (ct, cm) = split_mix(&t);
                similarity(&ct, title) >= 0.70
                    && mix_family_matches(known.mix.as_deref(), cm.as_deref())
            })
        });
        if !has_matching_mix {
            let hinted_q = match artist_terms.first() {
                Some(artist) => {
                    format!("{} {} {}", clean_q(artist), clean_q(title), clean_q(&hint))
                }
                None => format!("{} {}", clean_q(title), clean_q(&hint)),
            };
            for hit in search_raw(&hinted_q, lookup)? {
                let id = hit.get("id").and_then(Value::as_u64);
                if !raw_hits.iter().any(|existing| {
                    id.is_some() && existing.get("id").and_then(Value::as_u64) == id
                }) {
                    raw_hits.push(hit);
                }
            }
            let still_missing_mix = !raw_hits.iter().any(|item| {
                text(item, "title").is_some_and(|t| {
                    let (ct, cm) = split_mix(&t);
                    similarity(&ct, title) >= 0.70
                        && mix_family_matches(known.mix.as_deref(), cm.as_deref())
                })
            });
            if still_missing_mix
                && !artist_terms.is_empty()
                && matches!(
                    classify_mix(known.mix.as_deref()),
                    MixFamily::NamedRemix { .. }
                )
            {
                let remixer_only_q = format!("{} {}", clean_q(title), clean_q(&hint));
                for hit in search_raw(&remixer_only_q, lookup)? {
                    let id = hit.get("id").and_then(Value::as_u64);
                    if !raw_hits.iter().any(|existing| {
                        id.is_some() && existing.get("id").and_then(Value::as_u64) == id
                    }) {
                        raw_hits.push(hit);
                    }
                }
            }
        }
    }

    if raw_hits.is_empty() {
        return Ok(Vec::new());
    }

    let mut scored_indices: Vec<(bool, f64, usize)> = raw_hits
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let item_title = text(item, "title").unwrap_or_default();
            let (clean, item_mix) = split_mix(&item_title);
            let sim = similarity(&clean, title);
            let mix_ok = known.mix.is_some()
                && mix_family_matches(known.mix.as_deref(), item_mix.as_deref());
            (mix_ok, sim, idx)
        })
        .collect();
    scored_indices.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.total_cmp(&a.1)));

    let mut enriched_ids = Vec::new();
    for &(_, sim, idx) in scored_indices.iter().take(MAX_ENRICHED_HITS) {
        if sim >= 0.65
            && let Some(id) = raw_hits[idx].get("id").and_then(Value::as_u64)
        {
            enriched_ids.push(id);
        }
    }

    let mut out = Vec::with_capacity(raw_hits.len());
    for item in &raw_hits {
        let track_id = item.get("id").and_then(Value::as_u64);
        let full_track = match track_id.filter(|id| enriched_ids.contains(id)) {
            Some(id) => fetch_track_json(id, lookup)?,
            None => None,
        };
        let source = full_track.as_ref().unwrap_or(item);
        let album_obj = source.get("album");
        let album_meta = match album_obj
            .and_then(|a| a.get("id"))
            .and_then(Value::as_u64)
            .filter(|_| full_track.is_some())
        {
            Some(album_id) => fetch_album_json(album_id, lookup)?,
            None => None,
        };
        if let Some(cand) = build_candidate(source, album_meta.as_ref()) {
            out.push(cand);
        }
    }
    Ok(out)
}

fn append_search_hits(
    q: &str,
    raw_hits: &mut Vec<Value>,
    lookup: &Lookup<'_>,
) -> Result<(), IdentifyError> {
    for hit in search_raw(q, lookup)? {
        let id = hit.get("id").and_then(Value::as_u64);
        if !raw_hits
            .iter()
            .any(|existing| id.is_some() && existing.get("id").and_then(Value::as_u64) == id)
        {
            raw_hits.push(hit);
        }
    }
    Ok(())
}

fn search_raw(q: &str, lookup: &Lookup<'_>) -> Result<Vec<Value>, IdentifyError> {
    let request = HttpRequest::get(format!("{API}/search/track"))
        .param("q", q)
        .param("limit", "10");
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    if answer.get("error").is_some() {
        return Ok(Vec::new());
    }
    Ok(answer
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
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
    let request = HttpRequest::get(format!("{API}/track/isrc:{clean}"));
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    if answer.get("error").is_some() {
        return Ok(Vec::new());
    }
    let album_meta = match answer
        .get("album")
        .and_then(|a| a.get("id"))
        .and_then(Value::as_u64)
    {
        Some(album_id) => fetch_album_json(album_id, lookup)?,
        None => None,
    };
    Ok(build_candidate(&answer, album_meta.as_ref())
        .map(|mut c| {
            if c.isrc.is_none() {
                c.isrc = Some(clean);
            }
            c
        })
        .into_iter()
        .collect())
}

fn track_by_id(id: u64, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let Some(track) = fetch_track_json(id, lookup)? else {
        return Ok(Vec::new());
    };
    let album_meta = match track
        .get("album")
        .and_then(|a| a.get("id"))
        .and_then(Value::as_u64)
    {
        Some(album_id) => fetch_album_json(album_id, lookup)?,
        None => None,
    };
    Ok(build_candidate(&track, album_meta.as_ref())
        .into_iter()
        .collect())
}

fn album_tracks(album_id: u64, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let Some(album) = fetch_album_json(album_id, lookup)? else {
        return Ok(Vec::new());
    };
    let items = album
        .get("tracks")
        .and_then(|t| t.get("data"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let track_id = item.get("id").and_then(Value::as_u64);
        let full = match track_id {
            Some(id) if out.len() < 8 => fetch_track_json(id, lookup)?,
            _ => None,
        };
        if let Some(c) = build_candidate(full.as_ref().unwrap_or(&item), Some(&album)) {
            out.push(c);
        }
    }
    Ok(out)
}

fn fetch_track_json(id: u64, lookup: &Lookup<'_>) -> Result<Option<Value>, IdentifyError> {
    let Some(body) = lookup.fetch(&HttpRequest::get(format!("{API}/track/{id}")))? else {
        return Ok(None);
    };
    let val = parse_json(ID, &body)?;
    if val.get("error").is_some() {
        return Ok(None);
    }
    Ok(Some(val))
}

fn fetch_album_json(id: u64, lookup: &Lookup<'_>) -> Result<Option<Value>, IdentifyError> {
    let Some(body) = lookup.fetch(&HttpRequest::get(format!("{API}/album/{id}")))? else {
        return Ok(None);
    };
    let val = parse_json(ID, &body)?;
    if val.get("error").is_some() {
        return Ok(None);
    }
    Ok(Some(val))
}

fn build_candidate(track: &Value, album_meta: Option<&Value>) -> Option<Candidate> {
    let raw_title = text(track, "title")?;
    let (title, mix) = split_mix(&raw_title);
    let artist = track
        .get("contributors")
        .and_then(Value::as_array)
        .and_then(|list| {
            let names: Vec<String> = list.iter().filter_map(|a| text(a, "name")).collect();
            (!names.is_empty()).then(|| names.join(", "))
        })
        .or_else(|| track.get("artist").and_then(|a| text(a, "name")));

    let album_obj = album_meta.or_else(|| track.get("album"));
    let album = album_obj.and_then(|a| text(a, "title"));
    let label = album_meta.and_then(|a| text(a, "label"));
    let genre = album_meta.and_then(|a| {
        a.get("genres")
            .and_then(|g| g.get("data"))
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|g| text(g, "name"))
    });
    let year = text(track, "release_date")
        .as_deref()
        .and_then(year_in)
        .or_else(|| {
            album_obj
                .and_then(|a| text(a, "release_date"))
                .as_deref()
                .and_then(year_in)
        });
    let duration_ms = track
        .get("duration")
        .and_then(Value::as_u64)
        .filter(|&s| s > 0)
        .map(|s| s * 1000);
    let track_number = track
        .get("track_position")
        .and_then(Value::as_u64)
        .filter(|&p| p > 0)
        .map(|p| p.to_string());
    let release_kind =
        album_obj
            .and_then(|a| text(a, "record_type"))
            .map_or(ReleaseKind::Unknown, |rt| match rt.as_str() {
                "single" | "ep" => ReleaseKind::SingleOrEp,
                "album" => ReleaseKind::Album,
                "compile" => ReleaseKind::Compilation,
                _ => ReleaseKind::Unknown,
            });

    Some(Candidate {
        provider: ID.to_owned(),
        artist,
        title: Some(title),
        mix,
        album,
        label,
        year,
        duration_ms,
        genre,
        track_number,
        isrc: text(track, "isrc"),
        release_id: album_obj
            .and_then(|a| a.get("id"))
            .and_then(Value::as_u64)
            .map(|id| id.to_string()),
        url: text(track, "link"),
        // Legal requirement: Deezer cover art is never stored or embedded.
        artwork_url: None,
        artwork_provider: None,
        preview_url: text(track, "preview").filter(|url| url.starts_with("https://")),
        release_kind,
        ..Candidate::default()
    })
}

fn clean_q(value: &str) -> String {
    value.replace('"', " ").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deezer_candidate_keeps_artwork_none_even_when_album_carries_cover() {
        let track = json!({
            "id": 1001,
            "title": "Night Drive (Extended Mix)",
            "duration": 412,
            "isrc": "GBXYZ2100001",
            "link": "https://www.deezer.com/track/1001",
            "preview": "https://cdns-preview.dzcdn.net/stream/c-123.mp3",
            "artist": { "name": "Some Artist" },
            "album": { "id": 2001, "title": "Night Drive EP", "cover_xl": "https://cdn-images.dzcdn.net/x.jpg" }
        });
        let album = json!({
            "id": 2001,
            "title": "Night Drive EP",
            "label": "Defected",
            "release_date": "2021-05-01",
            "record_type": "ep",
            "genres": { "data": [{ "name": "House" }] }
        });
        let c = build_candidate(&track, Some(&album)).unwrap();
        assert_eq!(c.artwork_url, None);
        assert_eq!(c.artwork_provider, None);
        assert_eq!(c.label.as_deref(), Some("Defected"));
        assert_eq!(c.genre.as_deref(), Some("House"));
        assert_eq!(c.release_kind, ReleaseKind::SingleOrEp);
        assert_eq!(c.isrc.as_deref(), Some("GBXYZ2100001"));
    }

    #[test]
    fn quota_error_is_retryable_never_cacheable() {
        let dz = Deezer;
        let quota = r#"{"error":{"type":"Exception","message":"Quota limit exceeded","code":4}}"#;
        assert_eq!(
            dz.classify_response(200, quota),
            ResponseDisposition::Retryable
        );
        let no_data = r#"{"error":{"type":"DataException","message":"no data","code":800}}"#;
        assert_eq!(
            dz.classify_response(200, no_data),
            ResponseDisposition::Cacheable
        );
    }
}
