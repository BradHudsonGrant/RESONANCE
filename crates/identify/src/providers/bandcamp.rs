//! A Bandcamp album or track page the user pasted. Only pages the user asks for are read, and
//! only the structured data the page publishes for search engines.

use resonance_core::identify::{Candidate, ReleaseUrl, split_mix};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text, year_in,
};
use crate::rate::RateLimit;

const ID: &str = "bandcamp";

pub struct Bandcamp;

impl MetadataProvider for Bandcamp {
    fn id(&self) -> &'static str {
        ID
    }

    fn answers(&self, kind: QueryKind) -> bool {
        kind == QueryKind::Release
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
        let Query::Release(ReleaseUrl::Bandcamp(url)) = query else {
            return Ok(Vec::new());
        };
        let Some(page) = lookup.fetch(&HttpRequest::get(url.as_str()))? else {
            return Ok(Vec::new());
        };
        let data = structured_data(&page).ok_or(IdentifyError::Unexpected { provider: ID })?;
        Ok(candidates(&parse_json(ID, data)?, url))
    }
}

fn structured_data(page: &str) -> Option<&str> {
    let marker = page.find("application/ld+json")?;
    let start = marker + page[marker..].find('>')? + 1;
    let length = page[start..].find("</script>")?;
    Some(page[start..start + length].trim())
}

fn candidates(data: &Value, url: &str) -> Vec<Candidate> {
    let artist = data.get("byArtist").and_then(|artist| text(artist, "name"));
    let year = text(data, "datePublished").as_deref().and_then(year_in);
    let label = data
        .get("publisher")
        .and_then(|publisher| text(publisher, "name"))
        .filter(|publisher| Some(publisher) != artist.as_ref());
    let base = Candidate {
        provider: ID.to_owned(),
        artist: artist.clone(),
        label,
        year,
        url: Some(url.to_owned()),
        ..Candidate::default()
    };
    let track = |item: &Value, album: Option<String>| {
        let (title, mix) = split_mix(&text(item, "name")?);
        Some(Candidate {
            artist: item
                .get("byArtist")
                .and_then(|artist| text(artist, "name"))
                .or_else(|| base.artist.clone()),
            title: Some(title),
            mix,
            album,
            duration_ms: text(item, "duration").as_deref().and_then(iso_duration),
            ..base.clone()
        })
    };
    match data.get("@type").and_then(Value::as_str) {
        Some("MusicRecording") => track(
            data,
            data.get("inAlbum").and_then(|album| text(album, "name")),
        )
        .into_iter()
        .collect(),
        _ => {
            let album = text(data, "name");
            data.get("track")
                .and_then(|tracks| tracks.get("itemListElement"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|element| track(element.get("item")?, album.clone()))
                .collect()
        }
    }
}

/// `P00H06M52S` as milliseconds.
fn iso_duration(duration: &str) -> Option<u64> {
    let mut rest = duration.strip_prefix('P')?.trim_start_matches('T');
    let mut seconds = 0u64;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit() && c != '.')?;
        let value: f64 = rest[..digits].parse().ok()?;
        let unit = rest[digits..].chars().next()?;
        seconds += match unit {
            'H' => (value * 3600.0) as u64,
            'M' => (value * 60.0) as u64,
            'S' => value as u64,
            _ => return None,
        };
        rest = rest[digits + 1..].trim_start_matches('T');
    }
    (seconds > 0).then_some(seconds * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_in_the_form_the_page_uses() {
        assert_eq!(iso_duration("P00H06M52S"), Some(412_000));
        assert_eq!(iso_duration("PT3M5S"), Some(185_000));
        assert_eq!(iso_duration("six minutes"), None);
    }

    #[test]
    fn the_structured_data_is_found_in_the_page() {
        let page = r#"<html><script type="application/ld+json">
            {"@type":"MusicRecording","name":"Tune (Dub)","duration":"P00H05M00S",
             "byArtist":{"name":"Someone"},"inAlbum":{"name":"The EP"},
             "datePublished":"03 May 2019 00:00:00 GMT"}
            </script></html>"#;
        let data = parse_json(ID, structured_data(page).unwrap()).unwrap();
        let found = candidates(&data, "https://someone.bandcamp.com/track/tune");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title.as_deref(), Some("Tune"));
        assert_eq!(found[0].mix.as_deref(), Some("Dub"));
        assert_eq!(found[0].album.as_deref(), Some("The EP"));
        assert_eq!(found[0].year, Some(2019));
        assert_eq!(found[0].duration_ms, Some(300_000));
    }
}
