//! A Bandcamp album or track page the user pasted or stored in file comments. Only structured
//! JSON-LD published for search engines is read; no search scraping.

use resonance_core::identify::{
    Candidate, ReleaseKind, ReleaseUrl, clean_release_title, find_catno, is_placeholder_artist,
    split_artist_from_title, split_mix, strip_promo_and_catno,
};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text, year_in,
};
use crate::rate::RateLimit;

pub(crate) const ID: &str = "bandcamp";

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
    let publisher_name = data
        .get("publisher")
        .and_then(|publisher| text(publisher, "name"));
    let label = publisher_name
        .clone()
        .filter(|publisher| Some(publisher) != artist.as_ref());
    let artwork_url = image_url(data);
    let artwork_provider = artwork_url.as_ref().map(|_| ID.to_owned());
    let (genre, subgenre) = keywords_genres(data);

    let base = Candidate {
        provider: ID.to_owned(),
        artist,
        label,
        year,
        genre,
        subgenre,
        url: Some(url.to_owned()),
        artwork_url,
        artwork_provider,
        ..Candidate::default()
    };

    let track =
        |item: &Value, album: Option<String>, position: Option<String>, kind: ReleaseKind| {
            let (clean_raw, name_catno) = strip_promo_and_catno(&text(item, "name")?);
            let catno = album
                .as_deref()
                .and_then(find_catno)
                .or(name_catno)
                .or_else(|| text(data, "description").as_deref().and_then(find_catno));
            let raw_artist = item
                .get("byArtist")
                .and_then(|artist| text(artist, "name"))
                .or_else(|| base.artist.clone());
            let (artist, title, mix, track_label) = if let Some((left, right)) =
                split_artist_from_title(&clean_raw)
                && (raw_artist.is_none()
                    || raw_artist.as_deref() == publisher_name.as_deref()
                    || raw_artist.as_deref().is_some_and(is_placeholder_artist))
            {
                let (title, mix) = split_mix(&right);
                (
                    Some(left),
                    title,
                    mix,
                    base.label.clone().or_else(|| publisher_name.clone()),
                )
            } else {
                let (title, mix) = split_mix(&clean_raw);
                (raw_artist, title, mix, base.label.clone())
            };
            // A seller who leaves the artist as "Unknown Artist" says nothing about it, and that
            // must never replace the artist the file names.
            let artist = artist.filter(|name| !is_placeholder_artist(name));
            let item_art = image_url(item).or_else(|| base.artwork_url.clone());
            let item_art_provider = item_art.as_ref().map(|_| ID.to_owned());
            let (item_genre, item_subgenre) = keywords_genres(item);
            let album = album
                .as_deref()
                .map(|name| clean_release_title(name, artist.as_deref(), catno.as_deref()));
            Some(Candidate {
                artist,
                title: Some(title),
                mix,
                album,
                label: track_label,
                catno,
                track_number: position,
                genre: item_genre.or_else(|| base.genre.clone()),
                subgenre: item_subgenre.or_else(|| base.subgenre.clone()),
                artwork_url: item_art,
                artwork_provider: item_art_provider,
                release_kind: kind,
                duration_ms: text(item, "duration").as_deref().and_then(iso_duration),
                ..base.clone()
            })
        };

    match data.get("@type").and_then(Value::as_str) {
        Some("MusicRecording") => track(
            data,
            data.get("inAlbum").and_then(|album| text(album, "name")),
            None,
            ReleaseKind::SingleOrEp,
        )
        .into_iter()
        .collect(),
        _ => {
            let album = text(data, "name");
            let elements = data
                .get("track")
                .and_then(|tracks| tracks.get("itemListElement"))
                .and_then(Value::as_array);
            let count = elements.map_or(0, Vec::len);
            let kind = if (1..=6).contains(&count) {
                ReleaseKind::SingleOrEp
            } else if count > 6 {
                ReleaseKind::Album
            } else {
                ReleaseKind::Unknown
            };
            elements
                .into_iter()
                .flatten()
                .filter_map(|element| {
                    let pos = element
                        .get("position")
                        .and_then(Value::as_u64)
                        .filter(|&p| p > 0)
                        .map(|p| p.to_string());
                    track(element.get("item")?, album.clone(), pos, kind)
                })
                .collect()
        }
    }
}

fn image_url(value: &Value) -> Option<String> {
    match value.get("image") {
        Some(Value::String(s)) if s.trim().starts_with("https://") => Some(s.trim().to_owned()),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .find(|s| s.starts_with("https://"))
            .map(str::to_owned),
        Some(Value::Object(map)) => map
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| s.starts_with("https://"))
            .map(str::to_owned),
        _ => None,
    }
}

fn keywords_genres(value: &Value) -> (Option<String>, Option<String>) {
    let tags: Vec<String> = match value.get("keywords") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|entry| entry.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    let genre = tags.first().cloned();
    let subgenre = (tags.len() > 1).then(|| {
        tags.iter()
            .skip(1)
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    });
    (genre, subgenre)
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
             "byArtist":{"name":"Someone"},"inAlbum":{"name":"The EP [CAT009]"},
             "image":"https://f4.bcbits.com/img/a123_10.jpg",
             "keywords":["House","Deep House","Tech House"],
             "datePublished":"03 May 2019 00:00:00 GMT"}
            </script></html>"#;
        let data = parse_json(ID, structured_data(page).unwrap()).unwrap();
        let found = candidates(&data, "https://someone.bandcamp.com/track/tune");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title.as_deref(), Some("Tune"));
        assert_eq!(found[0].mix.as_deref(), Some("Dub"));
        assert_eq!(found[0].album.as_deref(), Some("The EP"));
        assert_eq!(found[0].catno.as_deref(), Some("CAT009"));
        assert_eq!(found[0].genre.as_deref(), Some("House"));
        assert_eq!(found[0].subgenre.as_deref(), Some("Deep House, Tech House"));
        assert_eq!(found[0].artwork_provider.as_deref(), Some("bandcamp"));
        assert_eq!(found[0].year, Some(2019));
        assert_eq!(found[0].duration_ms, Some(300_000));
    }

    #[test]
    fn a_placeholder_artist_on_the_page_is_not_offered() {
        let page = r#"<html><script type="application/ld+json">
            {"@type":"MusicRecording","name":"Night Cut (kit remix)","duration":"P00H06M00S",
             "byArtist":{"name":"Unknown Artist"},"publisher":{"name":"kit"}}
            </script></html>"#;
        let data = parse_json(ID, structured_data(page).unwrap()).unwrap();
        let found = candidates(&data, "https://kit.bandcamp.com/track/night-cut-kit-remix");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artist, None);
        assert_eq!(found[0].title.as_deref(), Some("Night Cut"));
    }

    #[test]
    fn bandcamp_label_page_extracts_artist_title_mix_label_and_catno() {
        let page = r#"<html><script type="application/ld+json">
            {"@type":"MusicRecording","name":"PREMIERE: Artist A - Night Cut (Dub Mix) [CAT008]","duration":"P00H06M00S",
             "byArtist":{"name":"Underground Wax"},"publisher":{"name":"Underground Wax"},
             "description":"Catalog CAT008 on Underground Wax",
             "image":"https://f4.bcbits.com/img/a999_10.jpg",
             "keywords":["Electronic","House"]}
            </script></html>"#;
        let data = parse_json(ID, structured_data(page).unwrap()).unwrap();
        let found = candidates(
            &data,
            "https://undergroundwax.bandcamp.com/track/artist-a-night-cut",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artist.as_deref(), Some("Artist A"));
        assert_eq!(found[0].title.as_deref(), Some("Night Cut"));
        assert_eq!(found[0].mix.as_deref(), Some("Dub Mix"));
        assert_eq!(found[0].label.as_deref(), Some("Underground Wax"));
        assert_eq!(found[0].catno.as_deref(), Some("CAT008"));
        assert_eq!(found[0].genre.as_deref(), Some("Electronic"));
        assert_eq!(found[0].subgenre.as_deref(), Some("House"));
    }

    #[test]
    fn bandcamp_self_released_title_with_mix_dash_preserves_artist_and_omits_label() {
        let mix_dash_page = r#"<html><script type="application/ld+json">
            {"@type":"MusicRecording","name":"Night Drive - Dub Mix","duration":"P00H06M00S",
             "byArtist":{"name":"Some Artist"},"publisher":{"name":"Some Artist"}}
            </script></html>"#;
        let data = parse_json(ID, structured_data(mix_dash_page).unwrap()).unwrap();
        let found = candidates(
            &data,
            "https://someartist.bandcamp.com/track/night-drive-dub",
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artist.as_deref(), Some("Some Artist"));
        assert_eq!(found[0].title.as_deref(), Some("Night Drive"));
        assert_eq!(found[0].mix.as_deref(), Some("Dub Mix"));
        assert_eq!(found[0].label, None);

        let part_page = r#"<html><script type="application/ld+json">
            {"@type":"MusicRecording","name":"Prelude - Part 1","duration":"P00H04M00S",
             "byArtist":{"name":"Some Artist"},"publisher":{"name":"Some Artist"}}
            </script></html>"#;
        let part_data = parse_json(ID, structured_data(part_page).unwrap()).unwrap();
        let part_found = candidates(&part_data, "https://someartist.bandcamp.com/track/prelude");
        assert_eq!(part_found[0].artist.as_deref(), Some("Some Artist"));
        assert_eq!(part_found[0].title.as_deref(), Some("Prelude - Part 1"));
        assert_eq!(part_found[0].mix, None);
        assert_eq!(part_found[0].label, None);
    }
}
