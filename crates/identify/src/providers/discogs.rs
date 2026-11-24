//! Discogs: tiered release search with the user's personal access token, and releases the user
//! pastes. Authenticated requests are limited to 60 a minute.

use resonance_core::identify::{
    Candidate, KnownTags, MixFamily, ReleaseKind, ReleaseUrl, artist_search_terms, classify_mix,
    is_placeholder_artist, mix_family_matches, mix_search_hint, search_fallback_text,
    search_title_clean, similarity, split_mix,
};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::{HttpRequest, Secret};
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text,
};
use crate::rate::RateLimit;

pub(crate) const ID: &str = "discogs";
const API: &str = "https://api.discogs.com";
const RELEASES_READ: usize = 3;
const MAX_SEARCH_CALLS: usize = 5;

pub struct Discogs;

impl MetadataProvider for Discogs {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        matches!(kind, QueryKind::Text | QueryKind::Release)
    }

    // Search needs a token; reading a release the user pasted does not.
    fn requires_key(&self, kind: QueryKind) -> Option<ProviderKey> {
        (kind == QueryKind::Text).then_some(ProviderKey::Discogs)
    }

    fn key(&self) -> Option<ProviderKey> {
        Some(ProviderKey::Discogs)
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit::per_minute(60)
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        match query {
            Query::Text(known) => search(known, lookup),
            Query::Release(ReleaseUrl::DiscogsRelease(id)) => release(*id, known_empty(), lookup),
            Query::Release(ReleaseUrl::DiscogsMaster(id)) => master(*id, lookup),
            _ => Ok(Vec::new()),
        }
    }
}

fn known_empty() -> &'static KnownTags {
    static EMPTY: std::sync::LazyLock<KnownTags> = std::sync::LazyLock::new(KnownTags::default);
    &EMPTY
}

fn authorised(request: HttpRequest, lookup: &Lookup<'_>) -> HttpRequest {
    match lookup.key() {
        Some(token) => request.with_secret(Secret::Header {
            name: "Authorization",
            value: format!("Discogs token={}", token.expose()),
        }),
        None => request,
    }
}

fn search(known: &KnownTags, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let mut seen_release_ids: Vec<u64> = Vec::new();
    let mut found: Vec<Candidate> = Vec::new();
    let mut relevant = false;
    let mut calls = 0usize;

    // Tier 1: catalogue number paired with label avoids cross-label false catno collisions.
    if let (Some(catno), Some(label)) = (&known.catno, &known.label) {
        let req = HttpRequest::get(format!("{API}/database/search"))
            .param("type", "release")
            .param("per_page", "5")
            .param("catno", catno.as_str())
            .param("label", label.as_str());
        calls += 1;
        let prev_len = found.len();
        read_search_hits(req, known, lookup, &mut seen_release_ids, &mut found)?;
        relevant |= has_relevant_track(&found[prev_len..], known);
    }

    // Tier 2: remix-hinted search first when NamedRemix, then exact artist + track.
    if calls < MAX_SEARCH_CALLS
        && !relevant
        && let Some(raw_title) = &known.title
        && let Some(artist) = &known.artist
    {
        let clean_title = search_title_clean(raw_title);
        let title = if clean_title.is_empty() {
            raw_title.as_str()
        } else {
            clean_title.as_str()
        };
        let artists = artist_search_terms(artist);
        let remix_hint = matches!(
            classify_mix(known.mix.as_deref()),
            MixFamily::NamedRemix { .. }
        )
        .then(|| mix_search_hint(known.mix.as_deref()))
        .flatten();
        if let (Some(hint), Some(lead_artist)) = (remix_hint.as_deref(), artists.first()) {
            let hinted_track = format!("{title} {hint}");
            let req = HttpRequest::get(format!("{API}/database/search"))
                .param("type", "release")
                .param("per_page", "5")
                .param("artist", lead_artist.as_str())
                .param("track", hinted_track.as_str());
            calls += 1;
            let prev_len = found.len();
            read_search_hits(req, known, lookup, &mut seen_release_ids, &mut found)?;
            relevant |= has_relevant_track(&found[prev_len..], known);
        }
        if !relevant {
            for artist_term in artists.into_iter().take(2) {
                if calls >= MAX_SEARCH_CALLS {
                    break;
                }
                let req = HttpRequest::get(format!("{API}/database/search"))
                    .param("type", "release")
                    .param("per_page", "5")
                    .param("artist", artist_term)
                    .param("track", title);
                calls += 1;
                let prev_len = found.len();
                read_search_hits(req, known, lookup, &mut seen_release_ids, &mut found)?;
                relevant |= has_relevant_track(&found[prev_len..], known);
                if relevant {
                    break;
                }
            }
        }
    }

    // Tier 3: broad free-text `q` (catches VA compilations, remix EPs and title-only white labels).
    if calls < MAX_SEARCH_CALLS
        && !relevant
        && let Some(raw_title) = &known.title
    {
        let clean_title = search_title_clean(raw_title);
        let title = if clean_title.is_empty() {
            raw_title.as_str()
        } else {
            clean_title.as_str()
        };
        let mut q = match known.artist.as_deref().and_then(|a| {
            artist_search_terms(a)
                .into_iter()
                .nth(1)
                .or_else(|| Some(a.to_owned()))
        }) {
            Some(artist) => format!("{artist} {title}"),
            None => title.to_owned(),
        };
        if matches!(
            classify_mix(known.mix.as_deref()),
            MixFamily::NamedRemix { .. }
        ) && let Some(remixer) = mix_search_hint(known.mix.as_deref())
        {
            q.push(' ');
            q.push_str(&remixer);
        }
        let needs_fallback = q
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && !c.is_ascii_whitespace());
        if needs_fallback {
            let folded = search_fallback_text(&q);
            if !folded.is_empty() {
                q = folded;
            }
        }
        let req = HttpRequest::get(format!("{API}/database/search"))
            .param("type", "release")
            .param("per_page", "5")
            .param("q", q);
        calls += 1;
        let prev_len = found.len();
        read_search_hits(req, known, lookup, &mut seen_release_ids, &mut found)?;
        relevant |= has_relevant_track(&found[prev_len..], known);
    }

    // Tier 3b: remixer-led free-text `q` without the original artist (catches white-label edits).
    if calls < MAX_SEARCH_CALLS
        && !relevant
        && known.artist.is_some()
        && matches!(
            classify_mix(known.mix.as_deref()),
            MixFamily::NamedRemix { .. }
        )
        && let Some(raw_title) = &known.title
        && let Some(remixer) = mix_search_hint(known.mix.as_deref())
    {
        let clean_title = search_title_clean(raw_title);
        let title = if clean_title.is_empty() {
            raw_title.as_str()
        } else {
            clean_title.as_str()
        };
        let mut q = format!("{title} {remixer}");
        if q.chars()
            .any(|c| !c.is_ascii_alphanumeric() && !c.is_ascii_whitespace())
        {
            let folded = search_fallback_text(&q);
            if !folded.is_empty() {
                q = folded;
            }
        }
        let req = HttpRequest::get(format!("{API}/database/search"))
            .param("type", "release")
            .param("per_page", "5")
            .param("q", q);
        calls += 1;
        let prev_len = found.len();
        read_search_hits(req, known, lookup, &mut seen_release_ids, &mut found)?;
        relevant |= has_relevant_track(&found[prev_len..], known);
    }

    // Tier 4: bare `catno` only as a last resort when label was absent and artist/title gave nothing.
    if calls < MAX_SEARCH_CALLS
        && !relevant
        && known.label.is_none()
        && let Some(catno) = &known.catno
    {
        let req = HttpRequest::get(format!("{API}/database/search"))
            .param("type", "release")
            .param("per_page", "5")
            .param("catno", catno.as_str());
        read_search_hits(req, known, lookup, &mut seen_release_ids, &mut found)?;
    }

    Ok(found)
}

fn read_search_hits(
    request: HttpRequest,
    known: &KnownTags,
    lookup: &Lookup<'_>,
    seen_ids: &mut Vec<u64>,
    found: &mut Vec<Candidate>,
) -> Result<(), IdentifyError> {
    let Some(body) = lookup.fetch(&authorised(request, lookup))? else {
        return Ok(());
    };
    let answer = parse_json(ID, &body)?;
    let results = answer
        .get("results")
        .and_then(Value::as_array)
        .ok_or(IdentifyError::Unexpected { provider: ID })?;
    let mut read_here = 0usize;
    for id in results
        .iter()
        .filter_map(|result| result.get("id").and_then(Value::as_u64))
    {
        if read_here >= RELEASES_READ {
            break;
        }
        if seen_ids.contains(&id) {
            continue;
        }
        seen_ids.push(id);
        read_here += 1;
        found.extend(release(id, known, lookup)?);
    }
    Ok(())
}

fn has_relevant_track(candidates: &[Candidate], known: &KnownTags) -> bool {
    let Some(raw_title) = &known.title else {
        return !candidates.is_empty();
    };
    let clean = search_title_clean(raw_title);
    let wanted = if clean.is_empty() {
        raw_title.as_str()
    } else {
        clean.as_str()
    };
    candidates.iter().any(|c| {
        c.title
            .as_deref()
            .is_some_and(|ct| similarity(&search_title_clean(ct), wanted) >= 0.70)
            && mix_family_matches(known.mix.as_deref(), c.mix.as_deref())
    })
}

fn master(id: u64, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let request = authorised(HttpRequest::get(format!("{API}/masters/{id}")), lookup);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    match answer.get("main_release").and_then(Value::as_u64) {
        Some(main) => release(main, known_empty(), lookup),
        None => Err(IdentifyError::Unexpected { provider: ID }),
    }
}

/// Every track on a release, as candidates. The ranker picks the one that is this file.
fn release(
    id: u64,
    known: &KnownTags,
    lookup: &Lookup<'_>,
) -> Result<Vec<Candidate>, IdentifyError> {
    let request = authorised(HttpRequest::get(format!("{API}/releases/{id}")), lookup);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let release_artist = artists(answer.get("artists"));
    let is_va_release = release_artist.as_deref().is_some_and(is_placeholder_artist);
    let (label_name, catno) = pick_label(&answer, known);
    let year = answer
        .get("year")
        .and_then(Value::as_u64)
        .and_then(|year| u16::try_from(year).ok())
        .filter(|year| *year > 0);
    let genre = answer
        .get("genres")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let subgenre = answer
        .get("styles")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .take(2)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|s| !s.is_empty());
    let artwork_url = primary_image(&answer);
    let artwork_provider = artwork_url.as_ref().map(|_| ID.to_owned());
    let release_kind = classify_release_format(&answer, is_va_release);

    let base = Candidate {
        provider: ID.to_owned(),
        album: text(&answer, "title"),
        label: label_name,
        catno,
        year,
        genre,
        subgenre,
        release_id: Some(id.to_string()),
        url: Some(format!("https://www.discogs.com/release/{id}")),
        artwork_url,
        artwork_provider,
        release_kind,
        ..Candidate::default()
    };

    Ok(answer
        .get("tracklist")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|track| {
            track
                .get("type_")
                .and_then(Value::as_str)
                .is_none_or(|kind| kind == "track")
        })
        .filter_map(|track| {
            let raw_title = text(track, "title")?;
            let track_artists = artists(track.get("artists"));
            let (artist, title_part) = match track_artists {
                Some(a) => (Some(a), raw_title),
                None if is_va_release => match raw_title.split_once(" - ") {
                    Some((va_artist, va_title))
                        if !va_artist.trim().is_empty() && !va_title.trim().is_empty() =>
                    {
                        (
                            Some(va_artist.trim().to_owned()),
                            va_title.trim().to_owned(),
                        )
                    }
                    _ => (release_artist.clone(), raw_title),
                },
                None => (release_artist.clone(), raw_title),
            };
            let (title, mix) = split_mix(&title_part);
            Some(Candidate {
                artist,
                title: Some(title),
                mix,
                track_number: text(track, "position"),
                duration_ms: text(track, "duration").as_deref().and_then(minutes_seconds),
                ..base.clone()
            })
        })
        .collect())
}

fn pick_label(answer: &Value, known: &KnownTags) -> (Option<String>, Option<String>) {
    let Some(labels) = answer.get("labels").and_then(Value::as_array) else {
        return (None, None);
    };
    let chosen = known
        .catno
        .as_deref()
        .and_then(|wanted| {
            let wanted_key = catno_compare_key(wanted);
            (!wanted_key.is_empty())
                .then_some(wanted_key)
                .and_then(|wk| {
                    labels.iter().find(|label| {
                        text(label, "catno").is_some_and(|c| catno_compare_key(&c) == wk)
                    })
                })
        })
        .or_else(|| {
            known.label.as_deref().and_then(|wanted| {
                labels.iter().find(|label| {
                    text(label, "name")
                        .is_some_and(|n| similarity(&without_number(&n), wanted) >= 0.8)
                })
            })
        })
        .or_else(|| {
            labels
                .iter()
                .find(|label| text(label, "catno").is_some_and(|c| !c.eq_ignore_ascii_case("none")))
        })
        .or_else(|| labels.first());

    let Some(label) = chosen else {
        return (None, None);
    };
    let name = text(label, "name").map(|n| without_number(&n));
    let catno = text(label, "catno").filter(|c| !c.eq_ignore_ascii_case("none"));
    (name, catno)
}

fn classify_release_format(answer: &Value, is_va_release: bool) -> ReleaseKind {
    let descriptions: Vec<String> = answer
        .get("formats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|fmt| {
            fmt.get("descriptions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase)
        })
        .collect();
    if is_va_release || descriptions.iter().any(|d| d.contains("compilation")) {
        return ReleaseKind::Compilation;
    }
    if descriptions
        .iter()
        .any(|d| matches!(d.as_str(), "single" | "ep" | "maxi-single" | "12\""))
    {
        return ReleaseKind::SingleOrEp;
    }
    if descriptions
        .iter()
        .any(|d| matches!(d.as_str(), "album" | "lp"))
    {
        return ReleaseKind::Album;
    }
    ReleaseKind::Unknown
}

fn primary_image(release: &Value) -> Option<String> {
    let images = release.get("images").and_then(Value::as_array)?;
    images
        .iter()
        .find(|image| image.get("type").and_then(Value::as_str) == Some("primary"))
        .or_else(|| images.first())
        .and_then(|image| text(image, "uri"))
        .filter(|uri| uri.starts_with("https://"))
}

fn minutes_seconds(duration: &str) -> Option<u64> {
    let mut total = 0u64;
    for part in duration.split(':') {
        total = total
            .checked_mul(60)?
            .checked_add(part.trim().parse().ok()?)?;
    }
    (total > 0).then_some(total * 1000)
}

fn catno_compare_key(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn without_number(name: &str) -> String {
    match name.rsplit_once(" (") {
        Some((base, number))
            if number
                .strip_suffix(')')
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) =>
        {
            base.to_owned()
        }
        _ => name.to_owned(),
    }
}

fn artists(list: Option<&Value>) -> Option<String> {
    let list = list.and_then(Value::as_array)?;
    let mut joined = String::new();
    for (index, artist) in list.iter().enumerate() {
        let name = text(artist, "anv").or_else(|| text(artist, "name"))?;
        joined.push_str(&without_number(&name));
        if index + 1 < list.len() {
            match text(artist, "join") {
                Some(join) if join == "," => joined.push_str(", "),
                Some(join) => {
                    joined.push(' ');
                    joined.push_str(&join);
                    joined.push(' ');
                }
                None => joined.push_str(", "),
            }
        }
    }
    (!joined.is_empty()).then_some(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_and_names_read_as_discogs_writes_them() {
        assert_eq!(minutes_seconds("6:52"), Some(412_000));
        assert_eq!(minutes_seconds("1:02:03"), Some(3_723_000));
        assert_eq!(minutes_seconds(""), None);
        assert_eq!(without_number("Some Artist (2)"), "Some Artist");
        assert_eq!(without_number("Tune (Dub)"), "Tune (Dub)");
    }
}
