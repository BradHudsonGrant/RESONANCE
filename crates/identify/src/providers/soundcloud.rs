//! SoundCloud release pages via the public oEmbed endpoint. No `api-v2` scraping.

use resonance_core::identify::{
    Candidate, ReleaseKind, ReleaseUrl, split_artist_from_title, split_mix, strip_promo_and_catno,
};
use resonance_core::providers::ProviderKey;
use serde_json::Value;

use crate::http::HttpRequest;
use crate::provider::{
    IdentifyError, Lookup, MetadataProvider, Query, QueryKind, parse_json, text,
};
use crate::rate::RateLimit;

pub(crate) const ID: &str = "soundcloud";
const OEMBED: &str = "https://soundcloud.com/oembed";

pub struct SoundCloud;

impl MetadataProvider for SoundCloud {
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
        RateLimit::per_second(2)
    }

    fn lookup(
        &self,
        query: &Query<'_>,
        lookup: &Lookup<'_>,
    ) -> Result<Vec<Candidate>, IdentifyError> {
        let Query::Release(ReleaseUrl::SoundCloud(url)) = query else {
            return Ok(Vec::new());
        };
        let request = HttpRequest::get(OEMBED)
            .param("format", "json")
            .param("url", url.as_str());
        let Some(body) = lookup.fetch(&request)? else {
            return Ok(Vec::new());
        };
        let answer = parse_json(ID, &body)?;
        Ok(parse_oembed(&answer, url).into_iter().collect())
    }
}

fn parse_oembed(answer: &Value, url: &str) -> Option<Candidate> {
    let raw_title = text(answer, "title")?;
    let author = text(answer, "author_name");
    let clean_title = strip_by_author(&raw_title, author.as_deref());
    let (stripped, catno) = strip_promo_and_catno(&clean_title);
    let (artist, title_part) = if let Some((left, right)) = split_artist_from_title(&stripped) {
        (Some(left), right)
    } else {
        (author, stripped)
    };
    let (title, mix) = split_mix(&title_part);
    let artwork_url = text(answer, "thumbnail_url").filter(|thumb| thumb.starts_with("https://"));
    let artwork_provider = artwork_url.as_ref().map(|_| ID.to_owned());

    Some(Candidate {
        provider: ID.to_owned(),
        artist,
        title: Some(title),
        mix,
        catno,
        url: Some(url.to_owned()),
        artwork_url,
        artwork_provider,
        release_kind: ReleaseKind::Unknown,
        ..Candidate::default()
    })
}

fn strip_by_author(title: &str, author: Option<&str>) -> String {
    if let Some(author) = author
        && let Some((head, tail)) = title.rsplit_once(" by ")
        && tail.trim().eq_ignore_ascii_case(author.trim())
        && !head.trim().is_empty()
    {
        return head.trim().to_owned();
    }
    title.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn soundcloud_oembed_extracts_artist_title_mix_and_artwork() {
        let data = json!({
            "title": "Some Artist - Night Drive (Extended Mix) by Label Page",
            "author_name": "Label Page",
            "thumbnail_url": "https://i1.sndcdn.com/artworks-123-t500x500.jpg"
        });
        let candidate = parse_oembed(&data, "https://soundcloud.com/label/night-drive").unwrap();
        assert_eq!(candidate.artist.as_deref(), Some("Some Artist"));
        assert_eq!(candidate.title.as_deref(), Some("Night Drive"));
        assert_eq!(candidate.mix.as_deref(), Some("Extended Mix"));
        assert_eq!(candidate.artwork_provider.as_deref(), Some("soundcloud"));
    }

    #[test]
    fn soundcloud_oembed_strips_premiere_prefix_and_catno() {
        let data = json!({
            "title": "PREMIERE: Some Artist - Night Cut (Dub) [CAT008] by Channel Name",
            "author_name": "Channel Name"
        });
        let candidate = parse_oembed(&data, "https://soundcloud.com/channel/night-cut").unwrap();
        assert_eq!(candidate.artist.as_deref(), Some("Some Artist"));
        assert_eq!(candidate.title.as_deref(), Some("Night Cut"));
        assert_eq!(candidate.mix.as_deref(), Some("Dub"));
        assert_eq!(candidate.catno.as_deref(), Some("CAT008"));
    }

    #[test]
    fn soundcloud_oembed_keeps_dash_mix_title_under_author() {
        let data = json!({
            "title": "Night Drive - Dub Mix by Some Artist",
            "author_name": "Some Artist"
        });
        let candidate =
            parse_oembed(&data, "https://soundcloud.com/artist/night-drive-dub").unwrap();
        assert_eq!(candidate.artist.as_deref(), Some("Some Artist"));
        assert_eq!(candidate.title.as_deref(), Some("Night Drive"));
        assert_eq!(candidate.mix.as_deref(), Some("Dub Mix"));
    }
}
