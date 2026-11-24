//! Release URL parsing and comment URL extraction for Step 6 and embedded tag links.

use std::fmt;

/// A release page link, either pasted by the user in Step 6 or found in file comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseUrl {
    DiscogsRelease(u64),
    DiscogsMaster(u64),
    Bandcamp(String),
    SoundCloud(String),
    YouTube(String),
    DeezerTrack(u64),
    DeezerAlbum(u64),
    AppleMusicAlbum {
        album_id: u64,
        track_id: Option<u64>,
    },
    MusicBrainzRelease(String),
    MusicBrainzRecording(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlRejected {
    NotAUrl,
    UnknownSite,
    NotARelease,
}

impl fmt::Display for UrlRejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAUrl => "That is not a web address. Paste the full link to the release.",
            Self::UnknownSite => {
                "Paste a link from Discogs, Bandcamp, SoundCloud, YouTube, Deezer, Apple Music or MusicBrainz."
            }
            Self::NotARelease => {
                "That page is not a release. Open the track or release itself and copy its address."
            }
        })
    }
}

impl std::error::Error for UrlRejected {}

/// Reads a release or track page link from any supported music service.
pub fn parse_release_url(input: &str) -> Result<ReleaseUrl, UrlRejected> {
    let input = input.trim();
    let rest = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .ok_or(UrlRejected::NotAUrl)?;
    let (without_fragment, _) = rest.split_once('#').unwrap_or((rest, ""));
    let (without_query, query) = without_fragment
        .split_once('?')
        .unwrap_or((without_fragment, ""));
    let (host, path) = without_query.split_once('/').unwrap_or((without_query, ""));
    let host = host.to_ascii_lowercase();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(UrlRejected::NotAUrl);
    }
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if host == "discogs.com" || host == "www.discogs.com" {
        let found = segments.windows(2).find_map(|pair| {
            let id = parse_leading_u64(pair[1])?;
            match pair[0] {
                "release" => Some(ReleaseUrl::DiscogsRelease(id)),
                "master" => Some(ReleaseUrl::DiscogsMaster(id)),
                _ => None,
            }
        });
        return found.ok_or(UrlRejected::NotARelease);
    }

    if let Some(artist) = host.strip_suffix(".bandcamp.com")
        && !artist.is_empty()
        && !artist.contains('.')
    {
        return match segments.as_slice() {
            [kind @ ("album" | "track"), name] => Ok(ReleaseUrl::Bandcamp(format!(
                "https://{host}/{kind}/{name}"
            ))),
            _ => Err(UrlRejected::NotARelease),
        };
    }

    if host == "soundcloud.com" || host == "www.soundcloud.com" || host == "on.soundcloud.com" {
        return parse_soundcloud(&host, &segments);
    }

    if host == "youtube.com"
        || host == "www.youtube.com"
        || host == "m.youtube.com"
        || host == "music.youtube.com"
        || host == "youtu.be"
    {
        return parse_youtube(&host, &segments, query);
    }

    if host == "deezer.com" || host == "www.deezer.com" {
        return parse_deezer(&segments);
    }

    if host == "music.apple.com" {
        return parse_apple_music(&segments, query);
    }

    if host == "musicbrainz.org" {
        return parse_musicbrainz(&segments);
    }

    Err(UrlRejected::UnknownSite)
}

/// Collects valid release/track URLs found inside text such as a file's Comment tag.
pub fn extract_release_urls(text: &str) -> Vec<ReleaseUrl> {
    let mut urls = Vec::new();
    for raw in text.split_whitespace() {
        let trimmed =
            raw.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';'));
        if let Ok(parsed) = parse_release_url(trimmed)
            && !urls.contains(&parsed)
        {
            urls.push(parsed);
        }
    }
    urls
}

fn parse_leading_u64(segment: &str) -> Option<u64> {
    let digits: String = segment.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u64>().ok()
}

fn parse_soundcloud(host: &str, segments: &[&str]) -> Result<ReleaseUrl, UrlRejected> {
    if host == "on.soundcloud.com" {
        return Err(UrlRejected::NotARelease);
    }
    match segments {
        [user, track]
            if !matches!(*user, "discover" | "charts" | "stream" | "search" | "you")
                && !track.is_empty() =>
        {
            Ok(ReleaseUrl::SoundCloud(format!(
                "https://soundcloud.com/{user}/{track}"
            )))
        }
        [user, "sets", set_slug] if !user.is_empty() && !set_slug.is_empty() => Ok(
            ReleaseUrl::SoundCloud(format!("https://soundcloud.com/{user}/sets/{set_slug}")),
        ),
        _ => Err(UrlRejected::NotARelease),
    }
}

fn parse_youtube(host: &str, segments: &[&str], query: &str) -> Result<ReleaseUrl, UrlRejected> {
    let id = if host == "youtu.be" {
        segments.first().copied()
    } else if segments.first() == Some(&"watch") {
        query_param(query, "v")
    } else if matches!(segments.first(), Some(&"shorts" | &"embed" | &"live")) {
        segments.get(1).copied()
    } else {
        None
    };
    let Some(video_id) = id.filter(|id| is_youtube_id(id)) else {
        return Err(UrlRejected::NotARelease);
    };
    Ok(ReleaseUrl::YouTube(video_id.to_owned()))
}

fn is_youtube_id(id: &str) -> bool {
    id.len() == 11
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn parse_deezer(segments: &[&str]) -> Result<ReleaseUrl, UrlRejected> {
    let found = segments.windows(2).find_map(|pair| {
        let id = pair[1].parse::<u64>().ok()?;
        match pair[0] {
            "track" => Some(ReleaseUrl::DeezerTrack(id)),
            "album" => Some(ReleaseUrl::DeezerAlbum(id)),
            _ => None,
        }
    });
    found.ok_or(UrlRejected::NotARelease)
}

fn parse_apple_music(segments: &[&str], query: &str) -> Result<ReleaseUrl, UrlRejected> {
    let album_pos = segments.iter().position(|&seg| seg == "album");
    let Some(pos) = album_pos else {
        return Err(UrlRejected::NotARelease);
    };
    let raw_id = segments.get(pos + 2).or_else(|| segments.get(pos + 1));
    let Some(album_id) = raw_id
        .and_then(|s| s.strip_prefix("id").or(Some(*s)))
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return Err(UrlRejected::NotARelease);
    };
    let track_id = query_param(query, "i").and_then(|s| s.parse::<u64>().ok());
    Ok(ReleaseUrl::AppleMusicAlbum { album_id, track_id })
}

fn parse_musicbrainz(segments: &[&str]) -> Result<ReleaseUrl, UrlRejected> {
    match segments {
        ["release", mbid, ..] if is_mbid(mbid) => {
            Ok(ReleaseUrl::MusicBrainzRelease((*mbid).to_owned()))
        }
        ["recording", mbid, ..] if is_mbid(mbid) => {
            Ok(ReleaseUrl::MusicBrainzRecording((*mbid).to_owned()))
        }
        _ => Err(UrlRejected::NotARelease),
    }
}

fn is_mbid(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_urls_are_read_and_others_refused() {
        assert_eq!(
            parse_release_url("https://www.discogs.com/release/123456-Artist-Title?x=1"),
            Ok(ReleaseUrl::DiscogsRelease(123_456))
        );
        assert_eq!(
            parse_release_url("https://www.discogs.com/de/master/42-Some-Release"),
            Ok(ReleaseUrl::DiscogsMaster(42))
        );
        assert_eq!(
            parse_release_url("https://artist.bandcamp.com/album/the-release#t1"),
            Ok(ReleaseUrl::Bandcamp(
                "https://artist.bandcamp.com/album/the-release".to_owned()
            ))
        );
        assert_eq!(
            parse_release_url("https://soundcloud.com/label/artist-track-mix"),
            Ok(ReleaseUrl::SoundCloud(
                "https://soundcloud.com/label/artist-track-mix".to_owned()
            ))
        );
        assert_eq!(
            parse_release_url("https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD1"),
            Ok(ReleaseUrl::YouTube("dQw4w9WgXcQ".to_owned()))
        );
        assert_eq!(
            parse_release_url("https://youtu.be/dQw4w9WgXcQ"),
            Ok(ReleaseUrl::YouTube("dQw4w9WgXcQ".to_owned()))
        );
        assert_eq!(
            parse_release_url("https://www.deezer.com/en/track/3135556"),
            Ok(ReleaseUrl::DeezerTrack(3_135_556))
        );
        assert_eq!(
            parse_release_url(
                "https://music.apple.com/gb/album/night-drive/1440857781?i=1440857789"
            ),
            Ok(ReleaseUrl::AppleMusicAlbum {
                album_id: 1_440_857_781,
                track_id: Some(1_440_857_789),
            })
        );
        assert_eq!(
            parse_release_url(
                "https://musicbrainz.org/release/b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d"
            ),
            Ok(ReleaseUrl::MusicBrainzRelease(
                "b10bbbfc-cf9e-42e0-be17-e2c3e1d2600d".to_owned()
            ))
        );
        assert_eq!(
            parse_release_url("https://label.bandcamp.com"),
            Err(UrlRejected::NotARelease)
        );
        assert_eq!(
            parse_release_url("https://www.discogs.com/artist/1-Someone"),
            Err(UrlRejected::NotARelease)
        );
        assert_eq!(
            parse_release_url("https://example.com/release/1"),
            Err(UrlRejected::UnknownSite)
        );
        assert_eq!(
            parse_release_url("discogs release 1"),
            Err(UrlRejected::NotAUrl)
        );
    }
}
