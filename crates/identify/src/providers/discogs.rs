//! Discogs: release search with the user's personal access token, and releases the user pastes.
//! See discogs.com/developers. Authenticated requests are limited to 60 a minute.

use resonance_core::identify::{Candidate, KnownTags, ReleaseUrl, split_mix};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::{HttpRequest, Secret};
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text,
};
use crate::rate::RateLimit;

const ID: &str = "discogs";
const API: &str = "https://api.discogs.com";
/// Each search hit costs one more request for its tracklist, so only the best few are read.
const RELEASES_READ: usize = 2;

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
            Query::Release(ReleaseUrl::DiscogsRelease(id)) => release(*id, lookup),
            Query::Release(ReleaseUrl::DiscogsMaster(id)) => master(*id, lookup),
            Query::Release(ReleaseUrl::Bandcamp(_))
            | Query::Fingerprint { .. }
            | Query::Recording { .. } => Ok(Vec::new()),
        }
    }
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
    let mut request = HttpRequest::get(format!("{API}/database/search"))
        .param("type", "release")
        .param("per_page", "5");
    let mut searched = false;
    for (name, value) in [
        ("artist", &known.artist),
        ("track", &known.title),
        ("catno", &known.catno),
        ("label", &known.label),
    ] {
        if let Some(value) = value {
            request = request.param(name, value.as_str());
            searched = true;
        }
    }
    if !searched {
        return Ok(Vec::new());
    }
    let Some(body) = lookup.fetch(&authorised(request, lookup))? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let results = answer
        .get("results")
        .and_then(Value::as_array)
        .ok_or(IdentifyError::Unexpected { provider: ID })?;
    let mut found = Vec::new();
    for id in results
        .iter()
        .filter_map(|result| result.get("id").and_then(Value::as_u64))
        .take(RELEASES_READ)
    {
        found.extend(release(id, lookup)?);
    }
    Ok(found)
}

fn master(id: u64, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let request = authorised(HttpRequest::get(format!("{API}/masters/{id}")), lookup);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    match answer.get("main_release").and_then(Value::as_u64) {
        Some(main) => release(main, lookup),
        None => Err(IdentifyError::Unexpected { provider: ID }),
    }
}

/// Every track on a release, as candidates. The ranker picks the one that is this file.
fn release(id: u64, lookup: &Lookup<'_>) -> Result<Vec<Candidate>, IdentifyError> {
    let request = authorised(HttpRequest::get(format!("{API}/releases/{id}")), lookup);
    let Some(body) = lookup.fetch(&request)? else {
        return Ok(Vec::new());
    };
    let answer = parse_json(ID, &body)?;
    let release_artist = artists(answer.get("artists"));
    let label = answer
        .get("labels")
        .and_then(Value::as_array)
        .and_then(|labels| labels.first());
    let year = answer
        .get("year")
        .and_then(Value::as_u64)
        .and_then(|year| u16::try_from(year).ok())
        .filter(|year| *year > 0);
    let base = Candidate {
        provider: ID.to_owned(),
        album: text(&answer, "title"),
        label: label
            .and_then(|label| text(label, "name"))
            .map(|name| without_number(&name)),
        catno: label
            .and_then(|label| text(label, "catno"))
            .filter(|catno| !catno.eq_ignore_ascii_case("none")),
        year,
        release_id: Some(id.to_string()),
        url: Some(format!("https://www.discogs.com/release/{id}")),
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
            let (title, mix) = split_mix(&text(track, "title")?);
            Some(Candidate {
                artist: artists(track.get("artists")).or_else(|| release_artist.clone()),
                title: Some(title),
                mix,
                duration_ms: text(track, "duration").as_deref().and_then(minutes_seconds),
                ..base.clone()
            })
        })
        .collect())
}

/// `6:52` or `1:02:03` as milliseconds.
fn minutes_seconds(duration: &str) -> Option<u64> {
    let mut total = 0u64;
    for part in duration.split(':') {
        total = total
            .checked_mul(60)?
            .checked_add(part.trim().parse().ok()?)?;
    }
    (total > 0).then_some(total * 1000)
}

// Discogs tells artists with the same name apart with ` (2)`, which is not part of the name.
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
