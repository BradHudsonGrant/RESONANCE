//! YouTube watch links resolved through the public oEmbed endpoint.

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

pub(crate) const ID: &str = "youtube";
const OEMBED: &str = "https://www.youtube.com/oembed";

pub struct YouTube;

impl MetadataProvider for YouTube {
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
        let Query::Release(ReleaseUrl::YouTube(video_id)) = query else {
            return Ok(Vec::new());
        };
        let watch_url = format!("https://www.youtube.com/watch?v={video_id}");
        let request = HttpRequest::get(OEMBED)
            .param("format", "json")
            .param("url", watch_url.as_str());
        let Some(body) = lookup.fetch(&request)? else {
            return Ok(Vec::new());
        };
        let answer = parse_json(ID, &body)?;
        Ok(parse_oembed(&answer, &watch_url).into_iter().collect())
    }
}

fn parse_oembed(answer: &Value, watch_url: &str) -> Option<Candidate> {
    let raw_title = text(answer, "title")?;
    let author = text(answer, "author_name").map(|name| {
        name.strip_suffix(" - Topic")
            .unwrap_or(&name)
            .trim()
            .to_owned()
    });
    let (stripped, catno) = strip_promo_and_catno(&raw_title);
    let (artist, title_part) = if let Some((left, right)) = split_artist_from_title(&stripped) {
        (Some(left), right)
    } else {
        (author, stripped)
    };
    let (title, mix) = split_mix(&title_part);

    Some(Candidate {
        provider: ID.to_owned(),
        artist,
        title: Some(title),
        mix,
        catno,
        url: Some(watch_url.to_owned()),
        // YouTube thumbnails are 16:9 letterboxed frames, not square release cover art.
        artwork_url: None,
        artwork_provider: None,
        release_kind: ReleaseKind::Unknown,
        ..Candidate::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn youtube_oembed_strips_topic_suffix_and_never_sets_artwork() {
        let data = json!({
            "title": "Night Drive (Club Mix)",
            "author_name": "Some Artist - Topic",
            "thumbnail_url": "https://i.ytimg.com/vi/abc/hqdefault.jpg"
        });
        let candidate = parse_oembed(&data, "https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(candidate.artist.as_deref(), Some("Some Artist"));
        assert_eq!(candidate.title.as_deref(), Some("Night Drive"));
        assert_eq!(candidate.mix.as_deref(), Some("Club Mix"));
        assert_eq!(candidate.artwork_url, None);
    }

    #[test]
    fn youtube_oembed_strips_premiere_prefix_and_catno() {
        let data = json!({
            "title": "PREMIERE: Some Artist - Night Cut (Dub) [CAT008]",
            "author_name": "Promo Channel"
        });
        let candidate = parse_oembed(&data, "https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(candidate.artist.as_deref(), Some("Some Artist"));
        assert_eq!(candidate.title.as_deref(), Some("Night Cut"));
        assert_eq!(candidate.mix.as_deref(), Some("Dub"));
        assert_eq!(candidate.catno.as_deref(), Some("CAT008"));
        assert_eq!(candidate.artwork_url, None);
    }

    #[test]
    fn youtube_oembed_keeps_dash_mix_title_under_topic_author() {
        let data = json!({
            "title": "Night Drive - Extended Mix",
            "author_name": "Some Artist - Topic"
        });
        let candidate = parse_oembed(&data, "https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap();
        assert_eq!(candidate.artist.as_deref(), Some("Some Artist"));
        assert_eq!(candidate.title.as_deref(), Some("Night Drive"));
        assert_eq!(candidate.mix.as_deref(), Some("Extended Mix"));
    }
}
