//! iTunes Search API: public fallback metadata and 30-second AAC previews without a key.
//! Apple artwork is never stored or embedded (`artwork_url` stays `None`).

use resonance_core::identify::{
    Candidate, KnownTags, MixFamily, ReleaseKind, ReleaseUrl, artist_search_terms, classify_mix,
    mix_family_matches, mix_search_hint, search_fallback_text, search_title_clean, similarity,
    split_mix,
};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text, year_in,
};
use crate::rate::RateLimit;

pub(crate) const ID: &str = "itunes";
const API: &str = "https://itunes.apple.com";

pub struct Itunes;

impl MetadataProvider for Itunes {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        matches!(kind, QueryKind::Text | QueryKind::Release)
    }

    fn requires_key(&self, _kind: QueryKind) -> Option<ProviderKey> {
        None
    }

    fn key(&self) -> Option<ProviderKey> {
        None
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit::per_minute(18)
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        match query {
            Query::Text(known) => search(known, lookup),
            Query::Release(ReleaseUrl::AppleMusicAlbum { album_id, track_id }) => {
                album_lookup(*album_id, *track_id, lookup)
            }
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
    let artist_term = known
        .artist
        .as_deref()
        .and_then(|artist| artist_search_terms(artist).into_iter().next());
    let base_term = match &artist_term {
        Some(artist) => format!("{artist} {title}"),
        None => title.to_owned(),
    };
    let mut candidates = search_term(&base_term, lookup)?;
    let has_match = candidates.iter().any(|c| {
        c.title
            .as_deref()
            .is_some_and(|ct| similarity(ct, title) >= 0.70)
    });
    if !has_match {
        let folded = search_fallback_text(&base_term);
        if !folded.is_empty() && folded != base_term.to_lowercase() {
            for cand in search_term(&folded, lookup)? {
                push_unique_candidate(&mut candidates, cand);
            }
        }
    }
    if let Some(hint) = mix_search_hint(known.mix.as_deref()) {
        let has_mix = candidates.iter().any(|c| {
            c.title
                .as_deref()
                .is_some_and(|ct| similarity(ct, title) >= 0.70)
                && mix_family_matches(known.mix.as_deref(), c.mix.as_deref())
        });
        if !has_mix {
            let hinted_term = format!("{base_term} {hint}");
            for cand in search_term(&hinted_term, lookup)? {
                push_unique_candidate(&mut candidates, cand);
            }
            let still_missing = !candidates.iter().any(|c| {
                c.title
                    .as_deref()
                    .is_some_and(|ct| similarity(ct, title) >= 0.70)
                    && mix_family_matches(known.mix.as_deref(), c.mix.as_deref())
            });
            if still_missing
                && artist_term.is_some()
                && matches!(
                    classify_mix(known.mix.as_deref()),
                    MixFamily::NamedRemix { .. }
                )
            {
                let remixer_only = format!("{title} {hint}");
                for cand in search_term(&remixer_only, lookup)? {
                    push_unique_candidate(&mut candidates, cand);
                }
            }
        }
    }
    Ok(candidates)
}

fn push_unique_candidate(candidates: &mut Vec<Candidate>, cand: Candidate) {
    let dup = candidates.iter().any(|existing| {
        existing.url.is_some() && existing.url == cand.url
            || (existing.title == cand.title
                && existing.mix == cand.mix
                && existing.artist == cand.artist)
    });
    if !dup {
        candidates.push(cand);
    }
}

fn search_term(term: &str, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let request = HttpRequest::get(format!("{API}/search"))
        .param("media", "music")
        .param("entity", "song")
        .param("limit", "10")
        .param("term", term);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    Ok(parse_songs(&answer, None))
}

fn album_lookup(
    album_id: u64,
    track_id: Option<u64>,
    lookup: &Lookup<'_>,
) -> Result<Vec<Candidate>, IdentifyError> {
    let request = HttpRequest::get(format!("{API}/lookup"))
        .param("id", album_id.to_string())
        .param("entity", "song");
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    Ok(parse_songs(&answer, track_id))
}

fn parse_songs(answer: &Value, preferred_track_id: Option<u64>) -> Vec<Candidate> {
    let mut candidates: Vec<(bool, Candidate)> = answer
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("wrapperType")
                .and_then(Value::as_str)
                .is_none_or(|kind| kind == "track")
                && text(entry, "trackName").is_some()
        })
        .filter_map(|entry| {
            let raw_title = text(entry, "trackName")?;
            let (title, mix) = split_mix(&raw_title);
            let track_num = entry
                .get("trackNumber")
                .and_then(Value::as_u64)
                .filter(|&n| n > 0)
                .map(|n| n.to_string());
            let duration_ms = entry
                .get("trackTimeMillis")
                .and_then(Value::as_u64)
                .filter(|&ms| ms > 0);
            let year = text(entry, "releaseDate").as_deref().and_then(year_in);
            let album = text(entry, "collectionName");
            let release_kind = classify_collection(album.as_deref());
            let this_id = entry.get("trackId").and_then(Value::as_u64);
            let preferred = preferred_track_id.is_some() && this_id == preferred_track_id;
            Some((
                preferred,
                Candidate {
                    provider: ID.to_owned(),
                    artist: text(entry, "artistName"),
                    title: Some(title),
                    mix,
                    album,
                    genre: text(entry, "primaryGenreName"),
                    year,
                    duration_ms,
                    track_number: track_num,
                    release_id: entry
                        .get("collectionId")
                        .and_then(Value::as_u64)
                        .map(|id| id.to_string()),
                    url: text(entry, "trackViewUrl").or_else(|| text(entry, "collectionViewUrl")),
                    artwork_url: None,
                    artwork_provider: None,
                    preview_url: text(entry, "previewUrl")
                        .filter(|url| url.starts_with("https://")),
                    release_kind,
                    ..Candidate::default()
                },
            ))
        })
        .collect();
    if preferred_track_id.is_some() && candidates.iter().any(|(pref, _)| *pref) {
        candidates.retain(|(pref, _)| *pref);
    }
    candidates.into_iter().map(|(_, c)| c).collect()
}

fn classify_collection(album: Option<&str>) -> ReleaseKind {
    let Some(name) = album else {
        return ReleaseKind::Unknown;
    };
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(" - single") || lower.ends_with(" - ep") {
        ReleaseKind::SingleOrEp
    } else {
        ReleaseKind::Album
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn itunes_results_never_carry_artwork_and_extract_preview_and_duration() {
        let payload = json!({
            "results": [{
                "wrapperType": "track",
                "trackId": 1440857789u64,
                "collectionId": 1440857781u64,
                "artistName": "Some Artist",
                "trackName": "Night Drive (Extended Mix)",
                "collectionName": "Night Drive - Single",
                "primaryGenreName": "House",
                "releaseDate": "2022-04-01T07:00:00Z",
                "trackTimeMillis": 412340u64,
                "trackNumber": 1,
                "trackViewUrl": "https://music.apple.com/gb/album/night-drive/1440857781?i=1440857789",
                "previewUrl": "https://audio-ssl.itunes.apple.com/preview.m4a",
                "artworkUrl100": "https://is1-ssl.mzstatic.com/image/100x100bb.jpg"
            }]
        });
        let found = parse_songs(&payload, None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artwork_url, None);
        assert_eq!(found[0].artwork_provider, None);
        assert_eq!(found[0].release_kind, ReleaseKind::SingleOrEp);
        assert_eq!(found[0].duration_ms, Some(412_340));
        assert_eq!(
            found[0].preview_url.as_deref(),
            Some("https://audio-ssl.itunes.apple.com/preview.m4a")
        );
    }
}
