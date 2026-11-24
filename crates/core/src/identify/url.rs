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

/// Words that name the kind of mix rather than who made it.
const MIX_ROLE_WORDS: [&str; 12] = [
    "edit", "re-edit", "remix", "rework", "refix", "bootleg", "flip", "mix", "dub", "version",
    "vip", "rmx",
];

/// The role words that say someone else made the mix. "Vocal Mix" or "Dub" alone is a version
/// by the artist, and names no one else to look for.
const MAKER_ROLE_WORDS: [&str; 9] = [
    "edit", "re-edit", "remix", "rework", "refix", "bootleg", "flip", "vip", "rmx",
];

/// Mix names that belong to no one, so they point at no Bandcamp page.
const GENERIC_MIX_NAMES: [&str; 8] = [
    "original",
    "extended",
    "club",
    "radio",
    "main",
    "album",
    "instrumental",
    "short",
];

/// Most Bandcamp pages a track's names are allowed to point at, so an unmatched track costs a
/// few page reads at most.
pub const MAX_GUESSED_BANDCAMP_PAGES: usize = 10;

/// Bandcamp track pages the file's names point at. Bandcamp builds a seller's address from
/// their name and a track's address from its title, so a self-released edit sold there can be
/// read directly from its page without searching Bandcamp. Tried in order: the remixer, the
/// name in the mix ("Will & Batty Edit"), the artist, then the label, each with the title alone
/// and with "Artist - Title", the form self-released edits and label compilations use.
pub fn guessed_bandcamp_pages(known: &super::KnownTags) -> Vec<ReleaseUrl> {
    let (Some(artist), Some(title)) = (known.artist.as_deref(), known.title.as_deref()) else {
        return Vec::new();
    };
    let full_title = crate::staging::title_with_mix(title, known.mix.as_deref());
    let mix_name = known.mix.as_deref().and_then(mix_maker);
    let album_seller = known.album.as_deref().and_then(super::release_seller);
    let sellers = [
        known.remixer.as_deref(),
        mix_name.as_deref(),
        Some(artist),
        known.label.as_deref(),
        album_seller.as_deref(),
    ];
    let slugs = [
        bandcamp_slug(&super::fold_latin(&full_title)),
        bandcamp_slug(&super::fold_latin(&format!("{artist} - {full_title}"))),
    ];
    // Sellers already matched for this artist come first: an artist who sells under another
    // name, such as a label account, is found that way once one of their tracks is matched.
    let named = sellers.into_iter().flatten().map(|seller| {
        super::fold_latin(seller)
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .flat_map(|c| c.to_lowercase())
            .collect::<String>()
    });
    let hosts = known.bandcamp_sellers.iter().cloned().chain(named);
    let mut pages = Vec::new();
    for host in hosts.filter(|host| host.len() >= 3) {
        for slug in slugs.iter().filter(|slug| !slug.is_empty()) {
            let page = ReleaseUrl::Bandcamp(format!("https://{host}.bandcamp.com/track/{slug}"));
            if !pages.contains(&page) {
                pages.push(page);
            }
            if pages.len() == MAX_GUESSED_BANDCAMP_PAGES {
                return pages;
            }
        }
    }
    pages
}

/// Who made a mix, from its name: "Will & Batty Edit" gives "Will & Batty".
pub fn mix_maker(mix: &str) -> Option<String> {
    let mut words: Vec<&str> = mix.split_whitespace().collect();
    let mut made_by_someone = false;
    while let Some(word) = words.last() {
        let word = word.to_lowercase();
        if !MIX_ROLE_WORDS.contains(&word.as_str()) {
            break;
        }
        made_by_someone |= MAKER_ROLE_WORDS.contains(&word.as_str());
        words.pop();
    }
    let name = words.join(" ");
    let generic = words
        .iter()
        .all(|word| GENERIC_MIX_NAMES.contains(&word.to_lowercase().as_str()));
    (made_by_someone && !name.is_empty() && !generic).then_some(name)
}

/// A title in the form Bandcamp puts in its addresses: lower case, with every run of other
/// characters turned into one hyphen and "&" dropped.
fn bandcamp_slug(text: &str) -> String {
    let mut slug = String::new();
    let mut gap = false;
    for c in text.chars().filter(|c| *c != '&' && *c != '\'') {
        if c.is_ascii_alphanumeric() {
            if gap && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(c.to_ascii_lowercase());
            gap = false;
        } else {
            gap = true;
        }
    }
    slug
}

/// A Bandcamp page a track was matched to, with the names the track was known by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedPage {
    pub artist: Option<String>,
    pub remixer: Option<String>,
    pub mix: Option<String>,
    pub url: String,
}

/// The Bandcamp sellers earlier matches point at for this track: any matched track that shares
/// its artist, remixer or the name in its mix. Remixers and edit makers often sell their work
/// for many artists from one account.
pub fn learned_sellers(known: &super::KnownTags, pages: &[MatchedPage]) -> Vec<String> {
    let names = |artist: Option<&str>, remixer: Option<&str>, mix: Option<&str>| {
        [
            artist.map(str::to_owned),
            remixer.map(str::to_owned),
            mix.and_then(mix_maker),
        ]
        .into_iter()
        .flatten()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
    };
    let own = names(
        known.artist.as_deref(),
        known.remixer.as_deref(),
        known.mix.as_deref(),
    );
    let mut sellers: Vec<String> = Vec::new();
    for page in pages {
        let theirs = names(
            page.artist.as_deref(),
            page.remixer.as_deref(),
            page.mix.as_deref(),
        );
        if theirs.iter().any(|name| own.contains(name))
            && let Some(seller) = bandcamp_seller(&page.url)
            && !sellers.iter().any(|held| held == seller)
        {
            sellers.push(seller.to_owned());
        }
    }
    sellers
}
/// The seller part of a Bandcamp address: `kitsounds` in `https://kitsounds.bandcamp.com/track/x`.
pub fn bandcamp_seller(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split('/').next()?;
    host.strip_suffix(".bandcamp.com")
        .filter(|seller| !seller.is_empty() && !seller.contains('.'))
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
    fn sellers_come_from_earlier_bandcamp_matches_sharing_a_name() {
        let page = |artist: &str, mix: Option<&str>, url: &str| MatchedPage {
            artist: Some(artist.to_owned()),
            remixer: None,
            mix: mix.map(str::to_owned),
            url: url.to_owned(),
        };
        let pages = [
            page(
                "Ada Marsh",
                None,
                "https://kitsounds.bandcamp.com/track/one",
            ),
            page(
                "Ada Marsh",
                None,
                "https://kitsounds.bandcamp.com/track/two",
            ),
            page(
                "Someone",
                Some("Rowe Edit"),
                "https://roweshop.bandcamp.com/track/x",
            ),
            page("Nobody", None, "https://other.bandcamp.com/track/y"),
        ];
        let known = super::super::KnownTags {
            artist: Some(" Ada Marsh ".to_owned()),
            mix: Some("Rowe Remix".to_owned()),
            ..Default::default()
        };
        assert_eq!(learned_sellers(&known, &pages), ["kitsounds", "roweshop"]);
        assert!(learned_sellers(&super::super::KnownTags::default(), &pages).is_empty());
    }

    #[test]
    fn bandcamp_pages_are_guessed_from_the_names_in_the_file() {
        let known = super::super::KnownTags {
            artist: Some("Ada Marsh".to_owned()),
            title: Some("Blue Hour".to_owned()),
            mix: Some("Kit & Rowe Edit".to_owned()),
            ..Default::default()
        };
        let pages = guessed_bandcamp_pages(&known);
        assert_eq!(
            pages,
            [
                "https://kitrowe.bandcamp.com/track/blue-hour-kit-rowe-edit",
                "https://kitrowe.bandcamp.com/track/ada-marsh-blue-hour-kit-rowe-edit",
                "https://adamarsh.bandcamp.com/track/blue-hour-kit-rowe-edit",
                "https://adamarsh.bandcamp.com/track/ada-marsh-blue-hour-kit-rowe-edit",
            ]
            .map(|url| ReleaseUrl::Bandcamp(url.to_owned()))
        );

        let labelled = super::super::KnownTags {
            mix: None,
            label: Some("Night Owl Records".to_owned()),
            ..known.clone()
        };
        assert!(
            guessed_bandcamp_pages(&labelled).contains(&ReleaseUrl::Bandcamp(
                "https://nightowlrecords.bandcamp.com/track/ada-marsh-blue-hour".to_owned()
            ))
        );

        let extended = super::super::KnownTags {
            mix: Some("Extended Mix".to_owned()),
            ..known.clone()
        };
        assert!(guessed_bandcamp_pages(&extended).iter().all(
            |page| matches!(page, ReleaseUrl::Bandcamp(url) if url.starts_with("https://adamarsh."))
        ));
        assert!(guessed_bandcamp_pages(&super::super::KnownTags::default()).is_empty());

        let learned = super::super::KnownTags {
            bandcamp_sellers: vec!["kitsounds".to_owned()],
            ..known.clone()
        };
        assert_eq!(
            guessed_bandcamp_pages(&learned)[0],
            ReleaseUrl::Bandcamp(
                "https://kitsounds.bandcamp.com/track/blue-hour-kit-rowe-edit".to_owned()
            )
        );
        assert_eq!(
            bandcamp_seller("https://kitsounds.bandcamp.com/album/x"),
            Some("kitsounds")
        );
        assert_eq!(bandcamp_seller("https://www.discogs.com/release/1"), None);
    }

    #[test]
    fn only_a_remix_or_edit_names_someone_else_to_look_for() {
        assert_eq!(mix_maker("Kit Rowe Remix").as_deref(), Some("Kit Rowe"));
        assert_eq!(mix_maker("Kit Rowe Dub Edit").as_deref(), Some("Kit Rowe"));
        assert_eq!(mix_maker("Vocal Mix"), None);
        assert_eq!(mix_maker("Extended Vocal Mix"), None);
        assert_eq!(mix_maker("Dub"), None);
        assert_eq!(mix_maker("Original Remix"), None);
    }

    #[test]
    fn accented_names_give_the_same_address_bandcamp_does() {
        let known = super::super::KnownTags {
            artist: Some("Ròse Ølsen".to_owned()),
            title: Some("Café Nuit".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            guessed_bandcamp_pages(&known)[0],
            ReleaseUrl::Bandcamp("https://roseolsen.bandcamp.com/track/cafe-nuit".to_owned())
        );
    }

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
