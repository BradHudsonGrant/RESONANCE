//! Tag and path normalisation for identification Steps 1 and 2.

use std::collections::HashSet;

use crate::library::TagField;

use super::{KnownTags, url::extract_release_urls};

const WEB_SUFFIXES: [&str; 16] = [
    "com", "net", "org", "info", "biz", "ru", "io", "co", "uk", "to", "cc", "me", "fm", "tk", "ws",
    "su",
];

const MIX_WORDS: [&str; 25] = [
    "mix",
    "remix",
    "rmx",
    "edit",
    "dub",
    "version",
    "vip",
    "rework",
    "bootleg",
    "instrumental",
    "remake",
    "refix",
    "reedit",
    "acapella",
    "accapella",
    "extended",
    "club",
    "radio",
    "vocal",
    "rub",
    "flip",
    "mashup",
    "rerub",
    "reprise",
    "tool",
];

const NOT_CATALOGUE: [&str; 22] = [
    "BPM", "KBPS", "KHZ", "HZ", "TRACK", "CD", "DISC", "VOL", "PT", "PART", "MIX", "EP", "LP",
    "FLAC", "WAV", "MP", "AIFF", "M4A", "AAC", "ALAC", "VINYL", "PROMO",
];

const PLACEHOLDER_ARTISTS: [&str; 9] = [
    "unknown",
    "unknown artist",
    "various",
    "various artists",
    "va",
    "v/a",
    "v.a.",
    "none",
    "untitled",
];

const GENERIC_DISTRIBUTORS: [&str; 22] = [
    "self released",
    "not on label",
    "independent",
    "distrokid",
    "tunecore",
    "cd baby",
    "cdbaby",
    "amuse",
    "amuseio",
    "routenote",
    "routernote",
    "unitedmasters",
    "landr",
    "awal",
    "awal recordings",
    "ditto music",
    "symphonic distribution",
    "labelworx",
    "proton",
    "the orchard",
    "believe",
    "onerpm",
];

const LABEL_GENERIC_SUFFIXES: [&str; 15] = [
    "records",
    "recordings",
    "music",
    "audio",
    "digital",
    "rec",
    "ltd",
    "limited",
    "inc",
    "gmbh",
    "uk",
    "us",
    "nyc",
    "london",
    "berlin",
];

const PROMO_PREFIXES: [&str; 4] = ["[PREMIERE] ", "PREMIERE: ", "FREE DOWNLOAD: ", "[FREE DL] "];

const VINYL_STRIP_TAGS: [&str; 14] = [
    "(vinyl only)",
    "[vinyl only]",
    "(vinyl rip)",
    "[vinyl rip]",
    "[vinyl]",
    "(vinyl)",
    "[promo]",
    "(promo)",
    "[unreleased]",
    "(unreleased)",
    "[free download]",
    "(free download)",
    "[mastered]",
    "(mastered)",
];

const VINYL_BARE_SUFFIXES: [&str; 2] = ["vinyl only", "vinyl rip"];

const CONTRACTION_S_STEMS: [&str; 14] = [
    "it", "that", "what", "let", "who", "she", "he", "here", "there", "where", "how", "when",
    "why", "one",
];

/// Folds common Latin diacritics to ASCII lowercase for resilient token comparisons.
pub fn fold_latin(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|c| !('\u{0300}'..='\u{036f}').contains(c))
    {
        match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => out.push('e'),
            'í' | 'ì' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => out.push('i'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' => out.push('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => out.push('u'),
            'ñ' | 'ń' | 'ņ' | 'ň' => out.push('n'),
            'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => out.push('c'),
            'ý' | 'ÿ' | 'ŷ' => out.push('y'),
            'ž' | 'ź' | 'ż' => out.push('z'),
            'š' | 'ś' | 'ŝ' | 'ş' => out.push('s'),
            'ř' | 'ŕ' | 'ŗ' => out.push('r'),
            'ť' | 'ţ' | 'ŧ' => out.push('t'),
            'ď' | 'đ' | 'ð' => out.push('d'),
            'þ' => out.push_str("th"),
            'ğ' | 'ĝ' | 'ġ' | 'ģ' => out.push('g'),
            'ł' | 'ĺ' | 'ļ' | 'ľ' => out.push('l'),
            'ß' => out.push_str("ss"),
            'æ' => out.push_str("ae"),
            'œ' => out.push_str("oe"),
            other => out.push(other),
        }
    }
    out
}

/// Lowercased ASCII-folded alphanumeric tokens, stripping trailing `'s` possessives while keeping
/// `'s` contractions (`it's -> its`), collapsing apostrophes (`don't -> dont`), and treating `&` as
/// `"and"`.
pub fn normalise_tokens(text: &str) -> Vec<String> {
    let folded = fold_latin(text);
    let mut chars = folded.chars();
    let mut c0 = chars.next();
    let mut c1 = chars.next();
    let mut c2 = chars.next();
    let mut out = Vec::new();
    let mut cur = String::new();
    while let Some(c) = c0 {
        if matches!(c, '’' | '\'' | '`' | 'ʼ') {
            let is_possessive = c1 == Some('s') && c2.is_none_or(|next| !next.is_alphanumeric());
            c0 = c1;
            c1 = c2;
            c2 = chars.next();
            if is_possessive && !cur.is_empty() && !CONTRACTION_S_STEMS.contains(&cur.as_str()) {
                c0 = c1;
                c1 = c2;
                c2 = chars.next();
            }
            continue;
        }
        if c == '&' {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push("and".to_owned());
        } else if c.is_alphanumeric() {
            cur.push(c);
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        c0 = c1;
        c1 = c2;
        c2 = chars.next();
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// ASCII-folded, apostrophe-collapsed, `&`-expanded query text for provider search fallbacks.
pub fn search_fallback_text(text: &str) -> String {
    normalise_tokens(text).join(" ")
}

/// Removes trailing non-mix release/EP bracket notes (`[Sampler Vol.4]`) and `(feat. ...)` clauses
/// for provider text searches and fallback title comparisons without altering stored tag titles.
pub fn search_title_clean(title: &str) -> String {
    let (without_feat, _) = split_featured(title);
    let mut current = without_feat.trim();
    loop {
        if current.ends_with(']')
            && let Some(start) = current.rfind('[')
            && start > 0
        {
            let before = current[..start].trim_end();
            if !before.is_empty() {
                current = before;
                continue;
            }
        }
        break;
    }
    current.trim().to_owned()
}

/// How alike two names are, from 0 to 1, by the normalised words they share.
pub fn similarity(a: &str, b: &str) -> f64 {
    let a: HashSet<String> = normalise_tokens(a).into_iter().collect();
    let b: HashSet<String> = normalise_tokens(b).into_iter().collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(&b).count();
    2.0 * shared as f64 / (a.len() + b.len()) as f64
}

/// Splits multi-artist strings (`A & B`, `A, B`, `A vs. B`, `A x B`, `A feat. B`) while keeping the
/// full string first so single acts such as `Simon & Garfunkel` try exact matching first.
pub fn artist_search_terms(artist: &str) -> Vec<String> {
    let trimmed = artist.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut terms = vec![trimmed.to_owned()];
    let mut working = trimmed.to_owned();
    for sep in [
        " feat. ",
        " feat ",
        " ft. ",
        " ft ",
        " featuring ",
        " vs. ",
        " vs ",
        " & ",
        " and ",
        ", ",
        " x ",
        " + ",
    ] {
        working = replace_ci(&working, sep, "\n");
    }
    for part in working.lines().map(str::trim).filter(|p| !p.is_empty()) {
        if !terms
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(part))
        {
            terms.push(part.to_owned());
        }
    }
    terms
}

fn replace_ci(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() || !needle.is_ascii() {
        return haystack.to_owned();
    }
    let needle_bytes = needle.as_bytes();
    let src = haystack.as_bytes();
    let mut out = String::with_capacity(haystack.len());
    let mut cursor = 0;
    while cursor < src.len() {
        if src[cursor..].len() >= needle_bytes.len()
            && src[cursor..cursor + needle_bytes.len()].eq_ignore_ascii_case(needle_bytes)
            && haystack.is_char_boundary(cursor)
            && haystack.is_char_boundary(cursor + needle_bytes.len())
        {
            out.push_str(replacement);
            cursor += needle_bytes.len();
        } else if let Some(ch) = haystack[cursor..].chars().next() {
            out.push(ch);
            cursor += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

pub fn is_placeholder_artist(value: &str) -> bool {
    let lowered = fold_latin(value.trim());
    PLACEHOLDER_ARTISTS.contains(&lowered.as_str())
}

pub fn is_generic_distributor_label(label: &str) -> bool {
    let tokens = normalise_tokens(label);
    if tokens.is_empty() {
        return false;
    }
    let joined = tokens.join(" ");
    GENERIC_DISTRIBUTORS.contains(&joined.as_str())
}

/// Normalises a label name for comparison without ever collapsing `"Records"` or `"Music Records"`
/// to an empty string.
pub fn normalise_label(label: &str) -> String {
    let mut tokens = normalise_tokens(label);
    if tokens.is_empty() {
        return String::new();
    }
    while tokens.len() > 1
        && tokens
            .last()
            .is_some_and(|last| LABEL_GENERIC_SUFFIXES.contains(&last.as_str()))
    {
        tokens.pop();
    }
    tokens.join(" ")
}

/// Cleans an explicit `TagField::CatNo`, rejecting placeholder strings such as `"none"`.
pub fn normalise_catno_tag(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "none" | "n/a" | "na" | "unknown" | "-" | "--"
        )
    {
        return None;
    }
    Some(trimmed.to_ascii_uppercase())
}

/// Splits trailing `(feat. Artist)` or `feat. Artist` from a track title.
pub fn split_featured(title: &str) -> (String, Option<String>) {
    let trimmed = title.trim();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if trimmed.ends_with(close)
            && let Some(start) = trimmed.rfind(open)
        {
            let inner = trimmed[start + open.len_utf8()..trimmed.len() - close.len_utf8()].trim();
            if let Some(rest) = strip_feat_prefix(inner) {
                let before = trimmed[..start].trim();
                if !before.is_empty() {
                    return (before.to_owned(), Some(rest.to_owned()));
                }
            }
        }
    }
    let lower = trimmed.to_ascii_lowercase();
    for marker in [" feat. ", " ft. ", " featuring ", " feat ", " ft "] {
        if let Some(pos) = lower.rfind(marker)
            && trimmed.is_char_boundary(pos)
            && trimmed.is_char_boundary(pos + marker.len())
        {
            let before = trimmed[..pos].trim();
            let after = trimmed[pos + marker.len()..].trim();
            if !before.is_empty() && !after.is_empty() {
                return (before.to_owned(), Some(after.to_owned()));
            }
        }
    }
    (trimmed.to_owned(), None)
}

fn strip_feat_prefix(text: &str) -> Option<&str> {
    for prefix in ["feat.", "feat", "ft.", "ft", "featuring"] {
        if text.len() > prefix.len()
            && text
                .get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
            && text[prefix.len()..].starts_with(char::is_whitespace)
        {
            let rest = text[prefix.len()..].trim();
            if !rest.is_empty() {
                return Some(rest);
            }
        }
    }
    None
}

/// Step 1: the file's own tags, trimmed, without website spam, promo/DJ prefixes, or trailing
/// catalogue/vinyl markers, and with URLs and ISRCs captured from comments.
pub fn normalise_tags(tags: &[(TagField, String)]) -> KnownTags {
    let raw = |wanted: TagField| {
        tags.iter()
            .find(|(field, _)| *field == wanted)
            .map(|(_, value)| value.as_str())
    };
    let field = |wanted: TagField| raw(wanted).and_then(clean_text);

    let comment_raw = raw(TagField::Comment);
    let embedded_urls = comment_raw.map(extract_release_urls).unwrap_or_default();
    let isrc = comment_raw.and_then(extract_isrc);

    let artist = field(TagField::Artist)
        .filter(|a| !is_placeholder_artist(a))
        .or_else(|| field(TagField::AlbumArtist).filter(|a| !is_placeholder_artist(a)));

    let mut known = KnownTags {
        artist,
        album: field(TagField::Album),
        label: field(TagField::Label).map(|l| clean_label_tag(&l)),
        year: field(TagField::Year).and_then(|year| parse_year(&year)),
        track_number: field(TagField::TrackNumber),
        remixer: field(TagField::Remixer),
        catno: raw(TagField::CatNo).and_then(normalise_catno_tag),
        isrc,
        embedded_urls,
        ..KnownTags::default()
    };

    if known.catno.is_none() {
        known.catno = [TagField::Comment, TagField::Album, TagField::Label]
            .into_iter()
            .filter_map(raw)
            .find_map(find_catno);
    }

    if let Some(title_raw) = field(TagField::Title) {
        let mut working = strip_promo_prefix(&title_raw);
        working = strip_dj_prefix(&working);
        let (without_catno, bracket_catno) = take_bracketed_catno(&working);
        if known.catno.is_none() {
            known.catno = bracket_catno;
        }
        working = clean_text(&without_catno).unwrap_or(without_catno);
        working = strip_vinyl_only_suffix(&working);
        let (without_hint, release_hint) = take_release_hint(&working);
        if known.album.is_none() {
            known.album = release_hint;
        }
        working = without_hint;

        if let Some(existing_artist) = known.artist.as_deref()
            && let Some((left, right)) = working.split_once(" - ")
        {
            let right_trimmed = right.trim();
            if !right_trimmed.is_empty() && similarity(left.trim(), existing_artist) >= 0.90 {
                working = right_trimmed.to_owned();
            }
        }

        if known.artist.is_none()
            && let Some((inferred_artist, rest_title)) = split_artist_from_title(&working)
        {
            known.artist = Some(inferred_artist);
            known.artist_inferred_from_title = true;
            working = rest_title;
        }

        let (title, mix) = split_mix(&working);
        let title = strip_vinyl_only_suffix(&title);
        known.title = (!title.is_empty()).then_some(title);
        known.mix = mix
            .map(|m| strip_vinyl_only_suffix(&m))
            .filter(|m| !m.is_empty());
    }

    known
}

fn clean_label_tag(label: &str) -> String {
    let trimmed = label.trim();
    if let Some((base, suffix)) = trimmed.rsplit_once(" [")
        && let Some(digits) = suffix.strip_suffix(']')
        && !base.trim().is_empty()
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
    {
        return base.trim().to_owned();
    }
    trimmed.to_owned()
}

fn strip_promo_prefix(text: &str) -> String {
    let mut out = text.trim();
    loop {
        let mut stripped = false;
        for prefix in PROMO_PREFIXES {
            if out.len() > prefix.len()
                && out
                    .get(..prefix.len())
                    .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
            {
                out = out[prefix.len()..].trim_start();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    out.to_owned()
}

/// Strips leading DJ key/BPM prefixes such as `8A - 124 - Title` or `124 BPM - Title`, leaving
/// numeric artist names (`808 State`, `4B`, `112`) untouched.
fn strip_dj_prefix(text: &str) -> String {
    let mut current = text.trim();
    let mut stripped_key = false;
    for _ in 0..2 {
        let Some((head, tail)) = current.split_once(" - ") else {
            break;
        };
        let head = head.trim();
        let tail_trimmed = tail.trim_start();
        if !stripped_key && is_camelot_token(head) {
            let next = tail_trimmed
                .split_once(" - ")
                .map_or(tail_trimmed, |(part, _)| part)
                .trim();
            if tail_trimmed.contains(" - ") || is_bpm_token(next, true) {
                current = tail_trimmed;
                stripped_key = true;
                continue;
            }
            break;
        }
        if is_bpm_token(head, stripped_key) {
            current = tail_trimmed;
        } else {
            break;
        }
    }
    current.to_owned()
}

fn is_camelot_token(token: &str) -> bool {
    let Some(last) = token.chars().last() else {
        return false;
    };
    if !matches!(last, 'a' | 'A' | 'b' | 'B') {
        return false;
    }
    let digits = &token[..token.len() - 1];
    matches!(digits.parse::<u8>(), Ok(1..=12))
}

fn is_bpm_token(token: &str, allow_bare: bool) -> bool {
    let upper = token.trim().to_ascii_uppercase();
    let (digits, has_unit) = match upper.strip_suffix("BPM") {
        Some(rest) => (rest.trim(), true),
        None => (upper.as_str(), false),
    };
    if !has_unit && !allow_bare {
        return false;
    }
    let valid_number = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '.');
    if !valid_number {
        return false;
    }
    matches!(digits.parse::<f64>(), Ok(bpm) if (60.0..=200.0).contains(&bpm))
}

/// UTF-8-safe removal of trailing vinyl/promo markers such as `(Vinyl Only)` or `Vinyl Only`.
fn strip_vinyl_only_suffix(text: &str) -> String {
    let mut current = text.trim();
    loop {
        let mut trimmed_any = false;
        for tag in VINYL_STRIP_TAGS {
            let len = tag.len();
            if current.len() > len {
                let split = current.len() - len;
                if current.is_char_boundary(split) && current[split..].eq_ignore_ascii_case(tag) {
                    current = current[..split].trim_end_matches(|c: char| {
                        c.is_whitespace() || matches!(c, '-' | '|' | '/')
                    });
                    trimmed_any = true;
                    break;
                }
            }
        }
        if !trimmed_any {
            for tag in VINYL_BARE_SUFFIXES {
                let len = tag.len();
                if current.len() > len {
                    let split = current.len() - len;
                    if current.is_char_boundary(split) && current[split..].eq_ignore_ascii_case(tag)
                    {
                        let prefix = current[..split].trim_end();
                        if prefix.ends_with('-') || current[..split].ends_with(' ') {
                            current =
                                prefix.trim_end_matches(|c: char| c.is_whitespace() || c == '-');
                            trimmed_any = true;
                            break;
                        }
                    }
                }
            }
        }
        if !trimmed_any {
            break;
        }
    }
    current.trim().to_owned()
}

fn is_bare_mix_or_part(text: &str) -> bool {
    let tokens = normalise_tokens(text);
    if tokens.is_empty() {
        return true;
    }
    let as_strs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    if matches!(
        as_strs.first().copied(),
        Some("part" | "pt" | "vol" | "volume" | "chapter" | "disc" | "side")
    ) && tokens.len() <= 3
    {
        return true;
    }
    let last = as_strs.last().copied().unwrap_or_default();
    if matches!(
        last,
        "mix"
            | "remix"
            | "rmx"
            | "edit"
            | "dub"
            | "version"
            | "vip"
            | "rework"
            | "bootleg"
            | "instrumental"
            | "remake"
            | "refix"
            | "reedit"
            | "acapella"
            | "accapella"
            | "reprise"
            | "tool"
    ) {
        return true;
    }
    tokens.len() == 1 && matches!(last, "extended" | "radio")
}

pub fn split_artist_from_title(title: &str) -> Option<(String, String)> {
    let (left, right) = title.split_once(" - ")?;
    let left = left.trim();
    let right = right.trim();
    if left.is_empty()
        || right.is_empty()
        || is_placeholder_artist(left)
        || left.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    let (right_core, _) = split_mix(right);
    if right_core.is_empty() || is_bare_mix_or_part(&right_core) {
        return None;
    }
    Some((left.to_owned(), right.to_owned()))
}

/// Step 2: what the file's name and its folders say, for a path relative to its source with
/// forward slashes. Handles `01 - Artist - Title (Mix)`, `A1 Artist - Title`, and folders named
/// `[CAT001] Label - Release (2023)` or `Artist/Album (2023)/Track`.
pub fn parse_path(path: &str) -> KnownTags {
    let mut segments = path.rsplit('/');
    let file = segments.next().unwrap_or_default();
    let parent = segments.next();
    let grandparent = segments.next();

    let mut known = KnownTags::default();
    let mut stem = strip_extension(file).to_owned();
    if !stem.contains(' ') && stem.contains('_') {
        stem = stem.replace('_', " ");
    }
    let (stem, catno) = take_bracketed_catno(&stem);
    known.catno = catno;
    let Some(stem) = clean_text(&stem) else {
        return known;
    };
    let stem = strip_promo_prefix(&stem);
    let stem = strip_dj_prefix(&stem);
    let (track_number, rest) = split_track_number(&stem);
    known.track_number = track_number;

    let parts: Vec<&str> = rest
        .split(" - ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let title = match parts.as_slice() {
        [] => None,
        [title] => Some((*title).to_owned()),
        [artist, rest @ ..] => {
            if !is_placeholder_artist(artist) {
                known.artist = Some((*artist).to_owned());
            }
            Some(rest.join(" - "))
        }
    };
    if let Some(title) = title {
        let cleaned = strip_vinyl_only_suffix(&title);
        let (title, mix) = split_mix(&cleaned);
        known.title = (!title.is_empty()).then_some(title);
        known.mix = mix
            .map(|m| strip_vinyl_only_suffix(&m))
            .filter(|m| !m.is_empty());
    }

    if let Some(parent) = parent {
        let folder = parse_folder(parent);
        if known.catno.is_none() {
            known.catno.clone_from(&folder.catno);
        }
        known.year = folder.year;
        known.album = folder.album;
        if folder.catno.is_some() {
            known.label = folder.left;
        } else if known.artist.is_none()
            && folder
                .left
                .as_deref()
                .is_some_and(|a| !is_placeholder_artist(a))
        {
            known.artist = folder.left;
        }
        if known.artist.is_none()
            && folder.year.is_some()
            && let Some(grandparent) = grandparent
                .and_then(clean_text)
                .filter(|g| !is_placeholder_artist(g))
        {
            known.artist = Some(grandparent);
        }
    }
    known
}

struct FolderFacts {
    catno: Option<String>,
    year: Option<u16>,
    left: Option<String>,
    album: Option<String>,
}

fn parse_folder(folder: &str) -> FolderFacts {
    let (rest, catno) = take_bracketed_catno(folder);
    let (rest, year) = take_bracketed_year(&rest);
    let rest = clean_text(&rest);
    let (left, album) = match rest.as_deref().and_then(|rest| rest.split_once(" - ")) {
        Some((left, album)) => (clean_text(left), clean_text(album)),
        None if catno.is_some() || year.is_some() => (None, rest),
        None => (None, None),
    };
    FolderFacts {
        catno,
        year,
        left,
        album,
    }
}

fn strip_extension(file: &str) -> &str {
    match file.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty()
                && (1..=5).contains(&extension.len())
                && extension.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem
        }
        _ => file,
    }
}

fn split_track_number(stem: &str) -> (Option<String>, &str) {
    let bytes = stem.as_bytes();
    let mut end = 0;
    let side = bytes.first().is_some_and(|b| (b'A'..=b'H').contains(b));
    if side {
        end = 1;
    }
    if !side
        && bytes.len() > 2
        && bytes[0].is_ascii_digit()
        && bytes[1] == b'-'
        && bytes[2].is_ascii_digit()
    {
        end = 2;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() && end - digits_start < 3 {
        end += 1;
    }
    let digits = end - digits_start;
    if digits == 0 || (side && digits > 2) {
        return (None, stem);
    }
    let number = &stem[..end];
    let after = &stem[end..];
    for separator in [". - ", " - ", ". ", "."] {
        if let Some(rest) = after.strip_prefix(separator)
            && !rest.trim().is_empty()
        {
            if separator == " - "
                && !side
                && digits_start == 0
                && digits == 3
                && !number.starts_with('0')
                && !rest.contains(" - ")
            {
                continue;
            }
            return (Some(number.to_owned()), rest.trim_start());
        }
    }
    let bare_space_allowed = side || digits == 2 || digits_start == 2;
    if bare_space_allowed
        && let Some(rest) = after.strip_prefix(' ')
        && !rest.trim().is_empty()
    {
        return (Some(number.to_owned()), rest.trim_start());
    }
    (None, stem)
}

/// Trims, collapses runs of whitespace, and removes website spam such as `www.example.com` or
/// `[example.net]`. `None` when nothing is left.
pub fn clean_text(value: &str) -> Option<String> {
    let mut kept = String::with_capacity(value.len());
    let chars: Vec<char> = value.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let c = chars[index];
        if let Some(close) = closing_bracket(c)
            && let Some(offset) = chars[index + 1..].iter().position(|&next| next == close)
        {
            let inner: String = chars[index + 1..index + 1 + offset].iter().collect();
            if inner.split_whitespace().any(is_web_address) {
                index += offset + 2;
                continue;
            }
        }
        kept.push(c);
        index += 1;
    }

    let words: Vec<&str> = kept
        .split_whitespace()
        .filter(|word| !is_web_address(word))
        .collect();
    let joined = words.join(" ");
    let trimmed = joined
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '|' | '_' | '~' | ':'))
        .to_owned();
    (!trimmed.is_empty()).then_some(trimmed)
}

fn closing_bracket(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

fn is_web_address(word: &str) -> bool {
    let word = word
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '/')
        .to_lowercase();
    if word.starts_with("http://") || word.starts_with("https://") || word.starts_with("www.") {
        return true;
    }
    let host = word.split('/').next().unwrap_or_default();
    let Some((name, suffix)) = host.rsplit_once('.') else {
        return false;
    };
    let name = name.rsplit('.').next().unwrap_or(name);
    WEB_SUFFIXES.contains(&suffix)
        && name.len() >= 3
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Words that mark bracketed text as the release a track came from, not its mix.
const RELEASE_WORDS: [&str; 7] = [
    "vol",
    "volume",
    "ep",
    "lp",
    "compilation",
    "sampler",
    "series",
];

/// Takes a trailing `[Label Vol.4]` or `(Name EP)` off a title. Downloads from compilations
/// often carry the release in the title, which hides the track's own name from every search.
pub fn take_release_hint(title: &str) -> (String, Option<String>) {
    let trimmed = title.trim();
    let Some(close) = trimmed.chars().last() else {
        return (trimmed.to_owned(), None);
    };
    let open = match close {
        ']' => '[',
        ')' => '(',
        _ => return (trimmed.to_owned(), None),
    };
    let Some(start) = trimmed.rfind(open) else {
        return (trimmed.to_owned(), None);
    };
    let inner = trimmed[start + open.len_utf8()..trimmed.len() - close.len_utf8()].trim();
    let before = trimmed[..start].trim();
    let names_release = normalise_tokens(inner)
        .iter()
        .any(|token| RELEASE_WORDS.contains(&token.trim_end_matches(char::is_numeric)));
    if before.is_empty() || inner.is_empty() || is_mix_name(inner) || !names_release {
        return (trimmed.to_owned(), None);
    }
    (before.to_owned(), Some(inner.to_owned()))
}

/// The seller or label a release name points at: `night owl` from `night owl Vol.2`.
pub fn release_seller(release: &str) -> Option<String> {
    let mut words = Vec::new();
    for word in release.split_whitespace() {
        let token = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        let stem = token.trim_end_matches(|c: char| c.is_numeric() || c == '.');
        if RELEASE_WORDS.contains(&stem) || token.starts_with("vol") {
            break;
        }
        words.push(word);
    }
    let name = words.join(" ");
    (!name.is_empty() && words.len() < 4).then_some(name)
}

fn is_mix_name(text: &str) -> bool {
    normalise_tokens(text)
        .into_iter()
        .any(|token| MIX_WORDS.contains(&token.as_str()))
}

/// Splits mix suffixes off the end of a title (`Title (Extended Mix)`, `Title [Dub]`,
/// `Title - Radio Edit`, or multi-pass `Title [Extended Mix] (Promo)`). Brackets that do not
/// describe a mix, such as `(feat. X)`, stay in the title.
pub fn split_mix(title: &str) -> (String, Option<String>) {
    let mut current = title.trim().to_owned();
    let mut collected: Option<String> = None;
    for _ in 0..3 {
        let Some((before, mix)) = split_mix_once(&current) else {
            break;
        };
        current = before;
        if collected.is_none() || mix.len() > collected.as_deref().map_or(0, str::len) {
            collected = Some(mix);
        }
    }
    (current, collected)
}

fn split_mix_once(trimmed: &str) -> Option<(String, String)> {
    if let Some(close) = trimmed.chars().last()
        && let Some(open) = match close {
            ')' => Some('('),
            ']' => Some('['),
            _ => None,
        }
        && let Some(start) = trimmed.rfind(open)
    {
        let inner = trimmed[start + open.len_utf8()..trimmed.len() - close.len_utf8()].trim();
        let before = trimmed[..start].trim();
        if !before.is_empty() && is_mix_name(inner) {
            return Some((before.to_owned(), inner.to_owned()));
        }
    }
    if let Some((before, after)) = trimmed.rsplit_once(" - ") {
        let (before, after) = (before.trim(), after.trim());
        if !before.is_empty() && is_mix_name(after) {
            return Some((before.to_owned(), after.to_owned()));
        }
    }
    None
}

/// Extracts a clean ISO 3901 ISRC (`[A-Z]{2}[A-Z0-9]{3}[0-9]{7}` with optional hyphens).
pub fn extract_isrc(text: &str) -> Option<String> {
    for raw_token in text.split(|c: char| !c.is_ascii_alphanumeric() && c != '-') {
        let clean: String = raw_token
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if is_valid_isrc(&clean) {
            return Some(clean);
        }
    }
    None
}

fn is_valid_isrc(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 12
        && bytes[0..2].iter().all(u8::is_ascii_uppercase)
        && bytes[2..5].iter().all(u8::is_ascii_alphanumeric)
        && bytes[5..12].iter().all(u8::is_ascii_digit)
}

/// The first catalogue number in `text`, such as `DEF023`, `CAT-001`, `CMC218D2` or `100M012`,
/// in upper case.
pub fn find_catno(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let boundary = start == 0 || !chars[start - 1].is_alphanumeric();
        if boundary
            && let Some((catno, _)) =
                catno_at(&chars, start).or_else(|| digit_catno_at(&chars, start))
        {
            return Some(catno);
        }
        start += 1;
    }
    None
}

/// Strips leading release promo tags (`PREMIERE:`, `[FREE DL]`, etc.) and bracketed catalogue
/// codes (`[CAT008]`, `(CAT-01)`) while returning the extracted upper-case catalogue number if found.
pub fn strip_promo_and_catno(raw: &str) -> (String, Option<String>) {
    const PROMO_BRACKETED: [&str; 6] = [
        "premiere",
        "free dl",
        "free download",
        "out now",
        "exclusive",
        "full stream",
    ];
    const PROMO_PREFIXES: [&str; 5] = [
        "premiere",
        "out now",
        "exclusive",
        "free dl",
        "free download",
    ];

    let mut s = raw.trim().to_owned();
    loop {
        let trimmed = s.trim_start().to_owned();
        let lower = trimmed.to_ascii_lowercase();
        let mut stripped: Option<String> = None;

        for (open, close) in [('[', ']'), ('(', ')')] {
            if trimmed.starts_with(open)
                && let Some(end) = trimmed[1..].find(close)
            {
                let inner = trimmed[1..1 + end].trim().to_ascii_lowercase();
                if PROMO_BRACKETED.contains(&inner.as_str()) {
                    stripped = Some(trimmed[2 + end..].to_owned());
                    break;
                }
            }
        }

        if stripped.is_none() {
            for kw in PROMO_PREFIXES {
                if let Some(rest) = lower.strip_prefix(kw)
                    && (rest.is_empty() || rest.starts_with([':', '|', '-', ' ']))
                {
                    let tail = trimmed[kw.len()..].trim_start();
                    if let Some(after_sep) = tail.strip_prefix([':', '|', '-']) {
                        stripped = Some(after_sep.trim_start().to_owned());
                        break;
                    }
                    let all_caps = !trimmed[..kw.len()].chars().any(|c| c.is_ascii_lowercase());
                    if all_caps && tail != &trimmed[kw.len()..] {
                        // e.g. `PREMIERE Artist - Title`
                        stripped = Some(tail.to_owned());
                        break;
                    }
                }
            }
        }

        match stripped {
            Some(next) if next != s => s = next,
            _ => break,
        }
    }

    let mut catno: Option<String> = None;
    loop {
        let (rest, found) = take_bracketed_catno(&s);
        let Some(code) = found else {
            break;
        };
        if catno.is_none() {
            catno = Some(code);
        }
        s = rest;
    }
    if catno.is_none() {
        catno = find_catno(&s);
    }

    let cleaned = s
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| matches!(c, '|' | '-'))
        .trim()
        .to_owned();
    (cleaned, catno)
}

fn catno_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut index = start;
    while index < chars.len() && chars[index].is_ascii_uppercase() {
        index += 1;
    }
    let letters: String = chars[start..index].iter().collect();
    if !(2..=8).contains(&letters.len()) || NOT_CATALOGUE.contains(&letters.as_str()) {
        return None;
    }
    let mut separator = None;
    if index < chars.len() && matches!(chars[index], '-' | ' ' | '.') {
        separator = Some(chars[index]);
        index += 1;
    }
    if separator == Some(' ') && letters.len() > 6 {
        return None;
    }
    let digits_start = index;
    while index < chars.len() && chars[index].is_ascii_digit() {
        index += 1;
    }
    let digits = index - digits_start;
    if !(2..=5).contains(&digits) {
        return None;
    }
    let mut suffix = String::new();
    while index < chars.len()
        && suffix.len() < 2
        && (chars[index].is_ascii_uppercase() || chars[index].is_ascii_digit())
    {
        suffix.push(chars[index]);
        index += 1;
    }
    if index < chars.len() && chars[index].is_alphanumeric() {
        return None;
    }
    if !suffix.is_empty() && !suffix.chars().any(|c| c.is_ascii_uppercase()) {
        return None;
    }
    let number: String = chars[digits_start..digits_start + digits].iter().collect();
    let sep_str = match separator {
        Some('-') => "-",
        Some(' ') => " ",
        Some('.') => ".",
        _ => "",
    };
    let catno = format!("{letters}{sep_str}{number}{suffix}");
    if (separator == Some(' ') && suffix.is_empty() && is_likely_year(&number))
        || (separator == Some(' ') && is_non_catno_prefix(&letters))
        || is_non_catno_word(&catno)
    {
        return None;
    }
    Some((catno, index))
}

fn digit_catno_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    if !chars[start].is_ascii_digit() {
        return None;
    }
    let mut index = start;
    while index < chars.len()
        && index - start < 12
        && (chars[index].is_ascii_uppercase()
            || chars[index].is_ascii_digit()
            || chars[index] == '-')
    {
        index += 1;
    }
    if index < chars.len() && chars[index].is_alphanumeric() {
        return None;
    }
    let token: String = chars[start..index].iter().collect();
    let trimmed = token.trim_end_matches('-');
    let letters = trimmed.chars().filter(char::is_ascii_uppercase).count();
    let digits = trimmed.chars().filter(char::is_ascii_digit).count();
    let digit_prefix_len = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    let (leading_digits, tail) = trimmed.split_at(digit_prefix_len);
    let tail_clean = tail.trim_start_matches('-');
    if !(4..=12).contains(&trimmed.len())
        || letters == 0
        || digits < 2
        || is_audio_unit_suffix(tail_clean)
        || (is_likely_year(leading_digits) && is_release_suffix(tail_clean))
        || is_non_catno_word(trimmed)
    {
        return None;
    }
    Some((trimmed.to_owned(), index))
}

fn is_likely_year(text: &str) -> bool {
    text.len() == 4
        && text
            .parse::<u16>()
            .is_ok_and(|year| (1950..=2099).contains(&year))
}

fn is_non_catno_prefix(letters: &str) -> bool {
    matches!(
        letters,
        "STUDIO"
            | "VOL"
            | "VOLUME"
            | "PART"
            | "PT"
            | "TRACK"
            | "DISC"
            | "SIDE"
            | "CHAPTER"
            | "EPISODE"
            | "SESSION"
            | "ISSUE"
            | "MIX"
            | "SET"
            | "NO"
            | "NR"
            | "NUM"
            | "JAN"
            | "FEB"
            | "MAR"
            | "APR"
            | "MAY"
            | "JUN"
            | "JUL"
            | "AUG"
            | "SEP"
            | "SEPT"
            | "OCT"
            | "NOV"
            | "DEC"
            | "SPRING"
            | "SUMMER"
            | "AUTUMN"
            | "FALL"
            | "WINTER"
    )
}

fn is_audio_unit_suffix(suffix: &str) -> bool {
    matches!(
        suffix,
        "BPM" | "BIT" | "KHZ" | "HZ" | "DB" | "INCH" | "KBPS" | "MB" | "GB" | "SEC" | "MIN"
    )
}

fn is_release_suffix(suffix: &str) -> bool {
    matches!(suffix, "EP" | "LP" | "CD" | "MIX" | "RMX" | "EDIT" | "VIP")
}

fn is_non_catno_word(text: &str) -> bool {
    let clean: String = text.chars().filter(char::is_ascii_alphanumeric).collect();
    matches!(
        clean.as_str(),
        "320KBPS"
            | "256KBPS"
            | "192KBPS"
            | "128KBPS"
            | "44KHZ"
            | "48KHZ"
            | "MP3"
            | "M4A"
            | "CD1"
            | "CD2"
            | "LP1"
            | "EP1"
    )
}

fn take_bracketed_catno(text: &str) -> (String, Option<String>) {
    take_bracketed(text, find_catno_exact)
}

fn take_bracketed_year(text: &str) -> (String, Option<u16>) {
    take_bracketed(text, |inner| {
        (inner.len() == 4).then(|| parse_year(inner)).flatten()
    })
}

fn find_catno_exact(inner: &str) -> Option<String> {
    let trimmed = inner.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    catno_at(&chars, 0)
        .or_else(|| digit_catno_at(&chars, 0))
        .filter(|(_, end)| *end == chars.len())
        .map(|(catno, _)| catno)
}

fn take_bracketed<T>(text: &str, read: impl Fn(&str) -> Option<T>) -> (String, Option<T>) {
    for (open, close) in [('[', ']'), ('(', ')'), ('{', '}')] {
        let mut from = 0;
        while let Some(offset) = text[from..].find(open) {
            let start = from + offset;
            let Some(length) = text[start + 1..].find(close) else {
                break;
            };
            let end = start + 1 + length;
            if let Some(value) = read(text[start + 1..end].trim()) {
                let rest = format!("{} {}", &text[..start], &text[end + 1..]);
                return (rest, Some(value));
            }
            from = end;
        }
    }
    (text.to_owned(), None)
}

fn parse_year(text: &str) -> Option<u16> {
    let digits: String = text.trim().chars().take(4).collect();
    let year: u16 = digits.parse().ok()?;
    (1900..=2100).contains(&year).then_some(year)
}

/// A release title as a file's Album tag should hold it. Sellers often title a release
/// "Artist - Name EP - CAT001" or "Name EP [CAT001]" so it reads well in a shop list; the artist
/// and catalogue number have their own tags, so both come off when they match.
pub fn clean_release_title(title: &str, artist: Option<&str>, catno: Option<&str>) -> String {
    let mut name = title.trim().to_owned();
    if let Some(catno) = catno.map(str::trim).filter(|catno| !catno.is_empty()) {
        let lower = name.to_lowercase();
        let wanted = catno.to_lowercase();
        for suffix in [
            format!(" - {wanted}"),
            format!(" [{wanted}]"),
            format!(" ({wanted})"),
        ] {
            if lower.ends_with(&suffix) {
                name.truncate(name.len() - suffix.len());
                break;
            }
        }
    }
    if let Some(artist) = artist.map(str::trim).filter(|artist| !artist.is_empty()) {
        let prefix = format!("{} - ", artist.to_lowercase());
        if name.to_lowercase().starts_with(&prefix) && name.len() > prefix.len() {
            name = name[prefix.len()..].to_owned();
        }
    }
    name.trim().to_owned()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_in_the_title_becomes_an_album_hint() {
        assert_eq!(
            take_release_hint("Glow Line [kit sounds Vol.4]"),
            ("Glow Line".to_owned(), Some("kit sounds Vol.4".to_owned()))
        );
        assert_eq!(
            take_release_hint("Glow Line (Dub)"),
            ("Glow Line (Dub)".to_owned(), None)
        );
        assert_eq!(
            take_release_hint("Glow Line (feat. Ada)"),
            ("Glow Line (feat. Ada)".to_owned(), None)
        );
        assert_eq!(
            release_seller("kit sounds Vol.4").as_deref(),
            Some("kit sounds")
        );
        assert_eq!(release_seller("Night Owl EP").as_deref(), Some("Night Owl"));
        let known = normalise_tags(&[
            (TagField::Artist, "Ada Marsh".to_owned()),
            (TagField::Title, "Glow Line [kit sounds Vol.4]".to_owned()),
        ]);
        assert_eq!(known.title.as_deref(), Some("Glow Line"));
        assert_eq!(known.album.as_deref(), Some("kit sounds Vol.4"));
    }
    #[test]
    fn a_seller_release_title_loses_the_artist_and_catalogue_number() {
        assert_eq!(
            clean_release_title(
                "Ada Marsh - Blue Hour EP - KIT004",
                Some("Ada Marsh"),
                Some("KIT004")
            ),
            "Blue Hour EP"
        );
        assert_eq!(
            clean_release_title("The EP [CAT009]", None, Some("CAT009")),
            "The EP"
        );
        assert_eq!(
            clean_release_title("Ada Marsh - Live", Some("Someone"), None),
            "Ada Marsh - Live"
        );
    }
    use crate::identify::ReleaseUrl;

    fn tags(pairs: &[(TagField, &str)]) -> Vec<(TagField, String)> {
        pairs
            .iter()
            .map(|(field, value)| (*field, (*value).to_owned()))
            .collect()
    }

    #[test]
    fn tags_are_trimmed_collapsed_and_lose_web_spam() {
        let known = normalise_tags(&tags(&[
            (
                TagField::Artist,
                "  Some   Artist  www.freemusic.example.com",
            ),
            (TagField::Title, "Night Drive (Extended Mix) [promo-dl.net]"),
            (TagField::Year, "2019-05-03"),
            (TagField::Comment, "Label release DEF023, 128 BPM"),
        ]));
        assert_eq!(known.artist.as_deref(), Some("Some Artist"));
        assert_eq!(known.title.as_deref(), Some("Night Drive"));
        assert_eq!(known.mix.as_deref(), Some("Extended Mix"));
        assert_eq!(known.year, Some(2019));
        assert_eq!(known.catno.as_deref(), Some("DEF023"));
    }

    #[test]
    fn a_bracket_that_is_not_a_mix_stays_in_the_title() {
        assert_eq!(
            split_mix("Tune (feat. Singer)"),
            ("Tune (feat. Singer)".to_owned(), None)
        );
        assert_eq!(
            split_mix("Tune - Radio Edit"),
            ("Tune".to_owned(), Some("Radio Edit".to_owned()))
        );
        assert_eq!(
            split_mix("Tune (Someone Remix)"),
            ("Tune".to_owned(), Some("Someone Remix".to_owned()))
        );
    }

    #[test]
    fn web_addresses_go_but_names_with_dots_stay() {
        assert_eq!(clean_text("Mr. Oizo").as_deref(), Some("Mr. Oizo"));
        assert_eq!(
            clean_text("Title - www.site.example").as_deref(),
            Some("Title")
        );
        assert_eq!(
            clean_text("Title (https://x.example/a)").as_deref(),
            Some("Title")
        );
        assert_eq!(clean_text("   "), None);
    }

    #[test]
    fn catalogue_numbers_are_found_and_counts_are_not() {
        assert_eq!(find_catno("[CAT001]").as_deref(), Some("CAT001"));
        assert_eq!(
            find_catno("out on ABC-123 soon").as_deref(),
            Some("ABC-123")
        );
        assert_eq!(find_catno("CMC218D2").as_deref(), Some("CMC218D2"));
        assert_eq!(find_catno("100M012").as_deref(), Some("100M012"));
        assert_eq!(find_catno("128 BPM 320 KBPS").as_deref(), None);
        assert_eq!(find_catno("MP3 file").as_deref(), None);
        assert_eq!(find_catno("lowercase abc123").as_deref(), None);
    }

    #[test]
    fn file_names_give_track_artist_title_and_mix() {
        let known = parse_path("Music/01 - Some Artist - Night Drive (Extended Mix).flac");
        assert_eq!(known.track_number.as_deref(), Some("01"));
        assert_eq!(known.artist.as_deref(), Some("Some Artist"));
        assert_eq!(known.title.as_deref(), Some("Night Drive"));
        assert_eq!(known.mix.as_deref(), Some("Extended Mix"));
        assert_eq!(known.album, None);

        let dotted = parse_path("01. Some Artist - Tune.flac");
        assert_eq!(dotted.track_number.as_deref(), Some("01"));
        assert_eq!(dotted.artist.as_deref(), Some("Some Artist"));

        let vinyl = parse_path("A1 Some Artist - Tune.wav");
        assert_eq!(vinyl.track_number.as_deref(), Some("A1"));
        assert_eq!(vinyl.artist.as_deref(), Some("Some Artist"));

        let underscores = parse_path("some_artist_-_tune.mp3");
        assert_eq!(underscores.artist.as_deref(), Some("some artist"));
        assert_eq!(underscores.title.as_deref(), Some("tune"));
    }

    #[test]
    fn numbers_that_are_part_of_a_name_are_kept() {
        assert_eq!(
            parse_path("2 Unlimited - Tune.mp3").artist.as_deref(),
            Some("2 Unlimited")
        );
        assert_eq!(
            parse_path("808 State - Tune.mp3").artist.as_deref(),
            Some("808 State")
        );
        assert_eq!(parse_path("Tune.mp3").title.as_deref(), Some("Tune"));
    }

    #[test]
    fn release_folders_give_catalogue_label_album_and_year() {
        let known = parse_path("[CAT001] Some Label - The Release (2023)/02 Artist - Tune.flac");
        assert_eq!(known.catno.as_deref(), Some("CAT001"));
        assert_eq!(known.label.as_deref(), Some("Some Label"));
        assert_eq!(known.album.as_deref(), Some("The Release"));
        assert_eq!(known.year, Some(2023));
        assert_eq!(known.artist.as_deref(), Some("Artist"));

        let nested = parse_path("Some Artist/The Album (2001)/03 Tune.flac");
        assert_eq!(nested.artist.as_deref(), Some("Some Artist"));
        assert_eq!(nested.album.as_deref(), Some("The Album"));
        assert_eq!(nested.title.as_deref(), Some("Tune"));

        let plain = parse_path("Downloads/Tune.flac");
        assert_eq!(plain.album, None);
        assert_eq!(plain.artist, None);
    }

    #[test]
    fn tags_win_over_the_path() {
        let mut known = normalise_tags(&tags(&[(TagField::Title, "Tagged")]));
        known.fill_from(&parse_path("Artist - From Name.flac"));
        assert_eq!(known.title.as_deref(), Some("Tagged"));
        assert_eq!(known.artist.as_deref(), Some("Artist"));
        assert!(known.is_searchable());
    }

    #[test]
    fn utf8_vinyl_only_stripping_and_comment_url_extraction_never_panic() {
        let known = normalise_tags(&tags(&[
            (TagField::Title, "Café del Mar (Extended Mix) (Vinyl Only)"),
            (
                TagField::Comment,
                "From https://artist.bandcamp.com/album/sunset ISRC USRC12345678",
            ),
            (TagField::CatNo, "none"),
        ]));
        assert_eq!(known.title.as_deref(), Some("Café del Mar"));
        assert_eq!(known.mix.as_deref(), Some("Extended Mix"));
        assert_eq!(known.isrc.as_deref(), Some("USRC12345678"));
        assert_eq!(known.catno, None);
        assert_eq!(
            known.embedded_urls,
            vec![ReleaseUrl::Bandcamp(
                "https://artist.bandcamp.com/album/sunset".to_owned()
            )]
        );

        let turkish = normalise_tags(&tags(&[(TagField::Title, "İstanbul İşi [Vinyl Only]")]));
        assert_eq!(turkish.title.as_deref(), Some("İstanbul İşi"));
    }

    #[test]
    fn normalise_label_never_collapses_to_empty_and_promo_prefix_requires_delimiter() {
        assert_eq!(normalise_label("Music Records"), "music");
        assert_eq!(normalise_label("Records"), "records");
        assert!(is_generic_distributor_label("DistroKid"));
        assert!(is_generic_distributor_label("Self-Released"));
        assert!(!is_generic_distributor_label("Defected Records"));

        let bare = normalise_tags(&tags(&[(TagField::Title, "Premiere Night Drive")]));
        assert_eq!(bare.title.as_deref(), Some("Premiere Night Drive"));

        let prefixed = normalise_tags(&tags(&[(TagField::Title, "[PREMIERE] Night Drive")]));
        assert_eq!(prefixed.title.as_deref(), Some("Night Drive"));

        assert_eq!(
            search_title_clean("Night Drive [Sampler Vol.4]"),
            "Night Drive"
        );
        assert_eq!(
            normalise_tokens("Devon King's Deep Mix"),
            vec!["devon", "king", "deep", "mix"]
        );
    }

    #[test]
    fn fold_latin_and_catno_and_dj_prefix_and_artist_split_guardrails_hold() {
        assert_eq!(normalise_tokens("Tie\u{0308}sto"), vec!["tiesto"]);

        for not_catno in [
            "SUMMER 2024",
            "MAY 2024",
            "STUDIO 54",
            "128BPM",
            "24BIT",
            "16BIT",
            "96KHZ",
            "12INCH",
            "2024EP",
        ] {
            assert_eq!(find_catno(not_catno), None, "{not_catno}");
        }
        assert_eq!(find_catno("Track [100M012]"), Some("100M012".to_owned()));

        let artist_4b = parse_path("4B - Pop Dat.mp3");
        assert_eq!(artist_4b.artist.as_deref(), Some("4B"));
        assert_eq!(artist_4b.title.as_deref(), Some("Pop Dat"));

        let artist_112 = parse_path("112 - Peaches & Cream.mp3");
        assert_eq!(artist_112.artist.as_deref(), Some("112"));
        assert_eq!(artist_112.title.as_deref(), Some("Peaches & Cream"));

        let prefixed = parse_path("8A - 124 - Some Artist - Night Drive.mp3");
        assert_eq!(prefixed.artist.as_deref(), Some("Some Artist"));
        assert_eq!(prefixed.title.as_deref(), Some("Night Drive"));

        let bpm_prefixed = parse_path("124 BPM - Some Artist - Night Drive.mp3");
        assert_eq!(bpm_prefixed.artist.as_deref(), Some("Some Artist"));
        assert_eq!(bpm_prefixed.title.as_deref(), Some("Night Drive"));

        let split_ext = normalise_tags(&tags(&[(
            TagField::Title,
            "Some Artist - Night Drive (Extended Mix)",
        )]));
        assert_eq!(split_ext.artist.as_deref(), Some("Some Artist"));
        assert_eq!(split_ext.title.as_deref(), Some("Night Drive"));
        assert_eq!(split_ext.mix.as_deref(), Some("Extended Mix"));

        let split_club =
            normalise_tags(&tags(&[(TagField::Title, "Some Artist - Club Tropicana")]));
        assert_eq!(split_club.artist.as_deref(), Some("Some Artist"));
        assert_eq!(split_club.title.as_deref(), Some("Club Tropicana"));

        let unsplit_part = normalise_tags(&tags(&[(TagField::Title, "Prelude - Part 1")]));
        assert_eq!(unsplit_part.artist, None);
        assert_eq!(unsplit_part.title.as_deref(), Some("Prelude - Part 1"));

        let dup_prefix = normalise_tags(&tags(&[
            (TagField::Artist, "Kaden"),
            (TagField::Title, "Kaden - Night Drive"),
        ]));
        assert_eq!(dup_prefix.artist.as_deref(), Some("Kaden"));
        assert_eq!(dup_prefix.title.as_deref(), Some("Night Drive"));
    }

    #[test]
    fn normalise_tokens_handles_contractions_ampersand_and_diacritics() {
        assert_eq!(normalise_tokens("It's Deep"), vec!["its", "deep"]);
        assert_eq!(normalise_tokens("Its Deep"), vec!["its", "deep"]);
        assert_eq!(normalise_tokens("What's Left"), vec!["whats", "left"]);
        assert_eq!(normalise_tokens("I Don't Know"), vec!["i", "dont", "know"]);
        assert_eq!(normalise_tokens("A & B"), vec!["a", "and", "b"]);
        assert_eq!(normalise_tokens("A and B"), vec!["a", "and", "b"]);
        assert_eq!(normalise_tokens("Nora Öst"), vec!["nora", "ost"]);
        assert_eq!(normalise_tokens("Nora O\u{0308}st"), vec!["nora", "ost"]);
        assert_eq!(normalise_tokens("İstanbul İşi"), vec!["istanbul", "isi"]);
        assert_eq!(
            artist_search_terms("Artist A and Artist B"),
            vec!["Artist A and Artist B", "Artist A", "Artist B"]
        );
        assert_eq!(
            search_fallback_text("Nora Öst & Friend - It's Fine"),
            "nora ost and friend its fine"
        );
    }

    #[test]
    fn strip_promo_and_catno_cleans_premiere_prefixes_and_bracketed_catnos() {
        assert_eq!(
            strip_promo_and_catno("PREMIERE: Some Artist - Night Cut (Dub Mix) [CAT008]"),
            (
                "Some Artist - Night Cut (Dub Mix)".to_owned(),
                Some("CAT008".to_owned())
            )
        );
        assert_eq!(
            strip_promo_and_catno("[Premiere] [FREE DL] Label Act - Tune (CAT-009)"),
            ("Label Act - Tune".to_owned(), Some("CAT-009".to_owned()))
        );
        assert_eq!(
            strip_promo_and_catno("PREMIERE Artist - Track AB12 [CAT008]"),
            ("Artist - Track AB12".to_owned(), Some("CAT008".to_owned()))
        );
        assert_eq!(
            strip_promo_and_catno("Premiere Night Drive"),
            ("Premiere Night Drive".to_owned(), None)
        );
        assert_eq!(
            strip_promo_and_catno("Exclusive Love"),
            ("Exclusive Love".to_owned(), None)
        );
    }
}
