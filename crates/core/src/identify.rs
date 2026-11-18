//! The rules for working out what a file is: reading what the file and its path already say,
//! and ranking what providers propose against that. Nothing here touches the network or a disk.

use std::collections::HashSet;
use std::fmt;

use crate::library::TagField;
use crate::providers::ProviderKey;

/// The cascade stops as soon as the best candidate reaches this.
pub const STOP_THRESHOLD: f64 = 0.92;

/// What a release page the user pasted is worth, unless the audio plainly disagrees with it.
pub const USER_URL_CONFIDENCE: f64 = 0.95;

/// An AcoustID score at or above this means Chromaprint compared the audio and it matched.
pub const FINGERPRINT_MATCH_SCORE: f64 = 0.7;

/// Within this, a candidate's duration counts as the same edit.
const SAME_EDIT_SECONDS: f64 = 2.0;

/// Past this, a candidate is a different edit: a radio edit is not the extended mix.
const DIFFERENT_EDIT_SECONDS: f64 = 10.0;

/// The most a candidate of a different edit or mix can score, so it never wins on its own.
const DIFFERENT_VERSION_CAP: f64 = 0.3;

/// The six steps, in the order they run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Step {
    ExistingTags,
    FileName,
    Fingerprint,
    TextSearch,
    AudioConfirmation,
    UserUrl,
}

impl Step {
    pub const ALL: [Self; 6] = [
        Self::ExistingTags,
        Self::FileName,
        Self::Fingerprint,
        Self::TextSearch,
        Self::AudioConfirmation,
        Self::UserUrl,
    ];

    pub fn number(self) -> u8 {
        match self {
            Self::ExistingTags => 1,
            Self::FileName => 2,
            Self::Fingerprint => 3,
            Self::TextSearch => 4,
            Self::AudioConfirmation => 5,
            Self::UserUrl => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::ExistingTags => "existing_tags",
            Self::FileName => "file_name",
            Self::Fingerprint => "fingerprint",
            Self::TextSearch => "text_search",
            Self::AudioConfirmation => "audio_confirmation",
            Self::UserUrl => "user_url",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|step| step.name() == name)
    }
}

/// What a step did for one track, as the user sees it in the report.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Steps 1 and 2: how many fields the tags or the path gave.
    Read { fields: usize },
    /// The step ran. `best` is the best confidence among what it found.
    Ran {
        candidates: usize,
        best: Option<f64>,
    },
    /// The step needs a key the user has not entered in Settings.
    SkippedMissingKey(ProviderKey),
    /// The step had nothing to work with, such as no fingerprint or no artist and title.
    SkippedNoInput,
    /// An earlier step was already confident.
    SkippedConfident,
    /// The provider could not be reached or answered with an error. The run goes on.
    Failed(String),
    /// Nothing was confident, so the user is asked for a release URL.
    NeedsUrl,
}

/// What a file says about itself, through its tags or its path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KnownTags {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub mix: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub year: Option<u16>,
    pub track_number: Option<String>,
}

impl KnownTags {
    /// Takes each field `self` lacks from `other`. What the tags say wins over the path.
    pub fn fill_from(&mut self, other: &KnownTags) {
        fn fill(target: &mut Option<String>, source: &Option<String>) {
            if target.is_none() {
                target.clone_from(source);
            }
        }
        fill(&mut self.artist, &other.artist);
        fill(&mut self.title, &other.title);
        fill(&mut self.mix, &other.mix);
        fill(&mut self.album, &other.album);
        fill(&mut self.label, &other.label);
        fill(&mut self.catno, &other.catno);
        fill(&mut self.track_number, &other.track_number);
        if self.year.is_none() {
            self.year = other.year;
        }
    }

    /// Enough to search a provider's catalogue by text.
    pub fn is_searchable(&self) -> bool {
        (self.artist.is_some() && self.title.is_some()) || self.catno.is_some()
    }

    /// How many fields are known.
    pub fn field_count(&self) -> usize {
        [
            &self.artist,
            &self.title,
            &self.mix,
            &self.album,
            &self.label,
            &self.catno,
            &self.track_number,
        ]
        .into_iter()
        .filter(|field| field.is_some())
        .count()
            + usize::from(self.year.is_some())
    }
}

/// Step 1: the file's own tags, trimmed, without website spam, and with the mix name taken out
/// of the title.
pub fn normalise_tags(tags: &[(TagField, String)]) -> KnownTags {
    let field = |wanted: TagField| {
        tags.iter()
            .find(|(field, _)| *field == wanted)
            .and_then(|(_, value)| clean_text(value))
    };
    let mut known = KnownTags {
        artist: field(TagField::Artist).or_else(|| field(TagField::AlbumArtist)),
        album: field(TagField::Album),
        label: field(TagField::Label),
        year: field(TagField::Year).and_then(|year| parse_year(&year)),
        track_number: field(TagField::TrackNumber),
        ..KnownTags::default()
    };
    if let Some(title) = field(TagField::Title) {
        let (title, mix) = split_mix(&title);
        known.title = Some(title);
        known.mix = mix;
    }
    known.catno = [TagField::Comment, TagField::Album, TagField::Label]
        .into_iter()
        .filter_map(|wanted| tags.iter().find(|(field, _)| *field == wanted))
        .find_map(|(_, value)| find_catno(value));
    known
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
            known.artist = Some((*artist).to_owned());
            Some(rest.join(" - "))
        }
    };
    if let Some(title) = title {
        let (title, mix) = split_mix(&title);
        known.title = Some(title);
        known.mix = mix;
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
        } else if known.artist.is_none() {
            known.artist = folder.left;
        }
        if known.artist.is_none()
            && folder.year.is_some()
            && let Some(grandparent) = grandparent.and_then(clean_text)
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

// A folder only counts as a release when it carries a year, a catalogue number or an
// `X - Y` name, so `Downloads` or `Techno` never becomes an album.
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

// `01 - `, `01. `, `1. `, `01 `, `1-01 `, `A1 ` and `B2. `. A bare space only follows a two-digit
// number or a vinyl side, so `2 Unlimited` and `808 State` keep their names.
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

const WEB_SUFFIXES: [&str; 16] = [
    "com", "net", "org", "info", "biz", "ru", "io", "co", "uk", "to", "cc", "me", "fm", "tk", "ws",
    "su",
];

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

const MIX_WORDS: [&str; 12] = [
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
];

fn is_mix_name(text: &str) -> bool {
    tokens(text).any(|token| MIX_WORDS.contains(&token.as_str()))
}

/// Splits a mix name off the end of a title: `Title (Extended Mix)`, `Title [Dub]` or
/// `Title - Radio Edit`. A bracket that is not a mix, such as `(feat. X)`, stays in the title.
pub fn split_mix(title: &str) -> (String, Option<String>) {
    let trimmed = title.trim();
    if let Some(close) = trimmed.chars().last()
        && let Some(open) = match close {
            ')' => Some('('),
            ']' => Some('['),
            _ => None,
        }
        && let Some(start) = trimmed.rfind(open)
    {
        let inner = trimmed[start + 1..trimmed.len() - 1].trim();
        let before = trimmed[..start].trim();
        if !before.is_empty() && is_mix_name(inner) {
            return (before.to_owned(), Some(inner.to_owned()));
        }
    }
    if let Some((before, after)) = trimmed.rsplit_once(" - ") {
        let (before, after) = (before.trim(), after.trim());
        if !before.is_empty() && is_mix_name(after) {
            return (before.to_owned(), Some(after.to_owned()));
        }
    }
    (trimmed.to_owned(), None)
}

// Words in front of a number that are not a label's prefix.
const NOT_CATALOGUE: [&str; 17] = [
    "BPM", "KBPS", "KHZ", "HZ", "TRACK", "CD", "DISC", "VOL", "PT", "PART", "MIX", "EP", "LP",
    "FLAC", "WAV", "MP", "AIFF",
];

/// The first catalogue number in `text`, such as `DEF023` or `CAT-001`, in upper case.
pub fn find_catno(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        let boundary = start == 0 || !chars[start - 1].is_alphanumeric();
        if boundary && let Some((catno, _)) = catno_at(&chars, start) {
            return Some(catno);
        }
        start += 1;
    }
    None
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
    if index < chars.len() && matches!(chars[index], '-' | ' ') {
        separator = Some(chars[index]);
        index += 1;
    }
    let digits_start = index;
    while index < chars.len() && chars[index].is_ascii_digit() {
        index += 1;
    }
    let digits = index - digits_start;
    if !(2..=5).contains(&digits) {
        return None;
    }
    if index < chars.len() && chars[index].is_alphanumeric() {
        return None;
    }
    let number: String = chars[digits_start..index].iter().collect();
    let catno = match separator {
        Some('-') => format!("{letters}-{number}"),
        Some(_) => format!("{letters} {number}"),
        None => format!("{letters}{number}"),
    };
    Some((catno, index))
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
    let chars: Vec<char> = inner.trim().chars().collect();
    catno_at(&chars, 0)
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

/// One answer a provider gives for what a file might be.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub provider: String,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub mix: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub catno: Option<String>,
    pub year: Option<u16>,
    pub duration_ms: Option<u64>,
    /// MusicBrainz recording MBID, when the candidate is a MusicBrainz recording.
    pub recording_id: Option<String>,
    pub release_id: Option<String>,
    pub url: Option<String>,
    /// Set only when Chromaprint compared this file's audio with the candidate's.
    pub fingerprint_score: Option<f64>,
    /// From a release page the user pasted.
    pub user_supplied: bool,
}

/// A candidate with its confidence for one file.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedCandidate {
    pub candidate: Candidate,
    pub confidence: f64,
    /// True only when Chromaprint compared the audio and it matched.
    pub audio_matched: bool,
    pub duration_delta_ms: Option<u64>,
}

/// Step 5, the part that needs no network: a text-search candidate whose MusicBrainz recording
/// came back from the fingerprint lookup is confirmed by the audio.
pub fn confirm_by_fingerprint(candidates: &mut [Candidate], fingerprinted: &[Candidate]) {
    for candidate in candidates.iter_mut() {
        if candidate.fingerprint_score.is_some() {
            continue;
        }
        let Some(recording) = &candidate.recording_id else {
            continue;
        };
        candidate.fingerprint_score = fingerprinted
            .iter()
            .filter(|matched| matched.recording_id.as_ref() == Some(recording))
            .filter_map(|matched| matched.fingerprint_score)
            .reduce(f64::max);
    }
}

/// Scores every candidate against what the file says and how long it is, best first.
pub fn rank(
    known: &KnownTags,
    duration_ms: Option<u64>,
    candidates: Vec<Candidate>,
) -> Vec<RankedCandidate> {
    let mut ranked: Vec<_> = candidates
        .into_iter()
        .map(|candidate| score(known, duration_ms, candidate))
        .collect();
    ranked.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    ranked
}

/// Weighs duration, Chromaprint, title and artist, mix name, label and catalogue number.
pub fn score(known: &KnownTags, duration_ms: Option<u64>, candidate: Candidate) -> RankedCandidate {
    let duration_delta_ms = duration_ms
        .zip(candidate.duration_ms)
        .map(|(ours, theirs)| ours.abs_diff(theirs));
    let duration = duration_delta_ms.map(|delta| duration_score(delta as f64 / 1000.0));
    let different_edit =
        duration_delta_ms.is_some_and(|delta| delta as f64 / 1000.0 > DIFFERENT_EDIT_SECONDS);

    let (candidate_title, candidate_mix) = match (&candidate.title, &candidate.mix) {
        (Some(title), None) => {
            let (title, mix) = split_mix(title);
            (Some(title), mix)
        }
        (title, mix) => (title.clone(), mix.clone()),
    };
    let mix = mix_agreement(known.mix.as_deref(), candidate_mix.as_deref());
    let text = text_score(
        known,
        candidate_title.as_deref(),
        candidate.artist.as_deref(),
    );
    let release = release_score(known, &candidate);

    let parts = [(text, 0.5), (duration, 0.25), (release, 0.25)];
    let weight: f64 = parts
        .iter()
        .filter(|(v, _)| v.is_some())
        .map(|(_, w)| w)
        .sum();
    let mut evidence = if weight > 0.0 {
        parts
            .iter()
            .filter_map(|(value, w)| value.map(|value| value * w))
            .sum::<f64>()
            / weight
    } else {
        0.0
    };
    // A mix name on one side only is most likely `Original Mix` left off, which a matching
    // duration all but confirms.
    if mix == MixAgreement::OneSided {
        evidence *= if duration.is_some() { 0.97 } else { 0.85 };
    }

    let audio_matched = candidate
        .fingerprint_score
        .is_some_and(|score| score >= FINGERPRINT_MATCH_SCORE);
    // A matching fingerprint closes part of the gap the other evidence leaves, so confirming a
    // candidate by its audio never lowers it. A weak fingerprint score counts for nothing.
    let mut confidence = match candidate.fingerprint_score {
        Some(fingerprint) if audio_matched => evidence + (1.0 - evidence) * fingerprint,
        _ => evidence,
    };
    let different_version = different_edit || mix == MixAgreement::Different;
    if candidate.user_supplied && !different_version {
        confidence = confidence.max(USER_URL_CONFIDENCE);
    }
    if different_version {
        confidence = confidence.min(DIFFERENT_VERSION_CAP);
    }
    RankedCandidate {
        candidate,
        confidence: confidence.clamp(0.0, 1.0),
        audio_matched,
        duration_delta_ms,
    }
}

fn duration_score(delta_s: f64) -> f64 {
    if delta_s <= SAME_EDIT_SECONDS {
        1.0
    } else if delta_s <= DIFFERENT_EDIT_SECONDS {
        1.0 - 0.6 * (delta_s - SAME_EDIT_SECONDS) / (DIFFERENT_EDIT_SECONDS - SAME_EDIT_SECONDS)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixAgreement {
    Same,
    OneSided,
    Different,
}

// No mix name and `Original Mix` mean the same thing; any other pair of names must agree.
fn mix_agreement(ours: Option<&str>, theirs: Option<&str>) -> MixAgreement {
    let normalised = |mix: Option<&str>| {
        mix.map(|mix| tokens(mix).collect::<Vec<_>>())
            .filter(|words| {
                let words: Vec<&str> = words.iter().map(String::as_str).collect();
                !matches!(words.as_slice(), [] | ["original"] | ["original", "mix"])
            })
    };
    match (normalised(ours), normalised(theirs)) {
        (None, None) => MixAgreement::Same,
        (Some(a), Some(b)) if a == b => MixAgreement::Same,
        (Some(_), Some(_)) => MixAgreement::Different,
        _ => MixAgreement::OneSided,
    }
}

fn text_score(known: &KnownTags, title: Option<&str>, artist: Option<&str>) -> Option<f64> {
    let title = known
        .title
        .as_deref()
        .zip(title)
        .map(|(a, b)| similarity(a, b));
    let artist = known
        .artist
        .as_deref()
        .zip(artist)
        .map(|(a, b)| similarity(a, b));
    match (title, artist) {
        (Some(title), Some(artist)) => Some(0.6 * title + 0.4 * artist),
        (Some(title), None) => Some(title),
        (None, Some(artist)) => Some(0.5 * artist),
        (None, None) => None,
    }
}

fn release_score(known: &KnownTags, candidate: &Candidate) -> Option<f64> {
    let catno = known
        .catno
        .as_deref()
        .zip(candidate.catno.as_deref())
        .map(|(a, b)| catno_key(a) == catno_key(b));
    let label = known
        .label
        .as_deref()
        .zip(candidate.label.as_deref())
        .map(|(a, b)| similarity(a, b) >= 0.8);
    match (catno, label) {
        (Some(true), _) => Some(1.0),
        (Some(false), Some(true)) => Some(0.5),
        (Some(false), _) => Some(0.0),
        (None, Some(true)) => Some(0.8),
        (None, Some(false)) => Some(0.2),
        (None, None) => None,
    }
}

fn catno_key(catno: &str) -> String {
    catno
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// How alike two names are, from 0 to 1, by the words they share.
pub fn similarity(a: &str, b: &str) -> f64 {
    let a: HashSet<String> = tokens(a).collect();
    let b: HashSet<String> = tokens(b).collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(&b).count();
    2.0 * shared as f64 / (a.len() + b.len()) as f64
}

/// A release page the user pasted for step 6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseUrl {
    DiscogsRelease(u64),
    DiscogsMaster(u64),
    /// An album or track page, as `https://<artist>.bandcamp.com/<album|track>/<name>`.
    Bandcamp(String),
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
            Self::UnknownSite => "Paste a Discogs release or a Bandcamp album or track page.",
            Self::NotARelease => {
                "That page is not a release. Open the release itself and copy its address."
            }
        })
    }
}

impl std::error::Error for UrlRejected {}

/// Reads a Discogs release or master link, or a Bandcamp album or track page.
pub fn parse_release_url(input: &str) -> Result<ReleaseUrl, UrlRejected> {
    let input = input.trim();
    let rest = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .ok_or(UrlRejected::NotAUrl)?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(UrlRejected::NotAUrl);
    }
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    if host == "discogs.com" || host == "www.discogs.com" {
        let found = segments.windows(2).find_map(|pair| {
            let id: String = pair[1].chars().take_while(char::is_ascii_digit).collect();
            let id = id.parse::<u64>().ok()?;
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
    Err(UrlRejected::UnknownSite)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn known() -> KnownTags {
        KnownTags {
            artist: Some("Some Artist".to_owned()),
            title: Some("Night Drive".to_owned()),
            ..KnownTags::default()
        }
    }

    fn candidate(title: &str, duration_ms: u64) -> Candidate {
        Candidate {
            provider: "test".to_owned(),
            artist: Some("Some Artist".to_owned()),
            title: Some(title.to_owned()),
            duration_ms: Some(duration_ms),
            ..Candidate::default()
        }
    }

    #[test]
    fn a_radio_edit_never_matches_an_extended_mix() {
        let extended_track_ms = 412_000;
        let ranked = rank(
            &known(),
            Some(extended_track_ms),
            vec![
                candidate("Night Drive (Radio Edit)", 201_000),
                candidate("Night Drive (Extended Mix)", 411_200),
            ],
        );
        assert_eq!(ranked[0].candidate.mix, None);
        assert_eq!(
            ranked[0].candidate.title.as_deref(),
            Some("Night Drive (Extended Mix)")
        );
        assert!(ranked[0].confidence >= STOP_THRESHOLD);
        assert!(ranked[1].confidence <= DIFFERENT_VERSION_CAP);

        let mut fingerprinted = candidate("Night Drive (Radio Edit)", 201_000);
        fingerprinted.fingerprint_score = Some(0.99);
        let ranked = score(&known(), Some(extended_track_ms), fingerprinted);
        assert!(ranked.confidence <= DIFFERENT_VERSION_CAP);
    }

    #[test]
    fn mix_names_that_disagree_are_different_versions_even_at_the_same_length() {
        let mut ours = known();
        ours.mix = Some("Extended Mix".to_owned());
        let ranked = score(
            &ours,
            Some(300_000),
            candidate("Night Drive (Dub)", 300_500),
        );
        assert!(ranked.confidence <= DIFFERENT_VERSION_CAP);
        let original = score(
            &KnownTags {
                mix: None,
                ..known()
            },
            Some(300_000),
            candidate("Night Drive (Original Mix)", 300_500),
        );
        assert!(original.confidence >= STOP_THRESHOLD);
    }

    #[test]
    fn the_audio_matches_only_when_chromaprint_says_so() {
        let text_only = score(&known(), Some(300_000), candidate("Night Drive", 300_000));
        assert!(!text_only.audio_matched);
        let mut weak = candidate("Night Drive", 300_000);
        weak.fingerprint_score = Some(0.4);
        assert!(!score(&known(), Some(300_000), weak).audio_matched);
        let mut strong = candidate("Night Drive", 300_000);
        strong.fingerprint_score = Some(0.95);
        let strong = score(&known(), Some(300_000), strong);
        assert!(strong.audio_matched);
        assert!(strong.confidence >= STOP_THRESHOLD);
    }

    #[test]
    fn catalogue_agreement_lifts_a_candidate_and_disagreement_lowers_it() {
        let mut ours = known();
        ours.catno = Some("DEF-023".to_owned());
        let mut same = candidate("Night Drive", 300_000);
        same.catno = Some("DEF023".to_owned());
        let mut other = candidate("Night Drive", 300_000);
        other.catno = Some("XYZ001".to_owned());
        let ranked = rank(&ours, Some(300_000), vec![other, same]);
        assert_eq!(ranked[0].candidate.catno.as_deref(), Some("DEF023"));
        assert!(ranked[0].confidence > ranked[1].confidence + 0.2);
    }

    #[test]
    fn text_candidates_are_confirmed_by_the_fingerprint_lookup() {
        let mut found = candidate("Night Drive", 300_000);
        found.recording_id = Some("rec-1".to_owned());
        let mut fingerprinted = Candidate {
            recording_id: Some("rec-1".to_owned()),
            fingerprint_score: Some(0.9),
            ..Candidate::default()
        };
        let mut candidates = vec![found];
        confirm_by_fingerprint(&mut candidates, std::slice::from_ref(&fingerprinted));
        assert_eq!(candidates[0].fingerprint_score, Some(0.9));
        fingerprinted.recording_id = Some("rec-2".to_owned());
        let mut unrelated = vec![candidate("Night Drive", 300_000)];
        confirm_by_fingerprint(&mut unrelated, &[fingerprinted]);
        assert_eq!(unrelated[0].fingerprint_score, None);
    }

    #[test]
    fn a_pasted_release_is_trusted_unless_the_audio_disagrees() {
        let mut pasted = candidate("Something Else", 300_000);
        pasted.user_supplied = true;
        assert!(score(&known(), Some(300_000), pasted.clone()).confidence >= USER_URL_CONFIDENCE);
        pasted.duration_ms = Some(180_000);
        assert!(score(&known(), Some(300_000), pasted).confidence <= DIFFERENT_VERSION_CAP);
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

    #[test]
    fn steps_round_trip_by_name_and_number() {
        for (index, step) in Step::ALL.into_iter().enumerate() {
            assert_eq!(Step::from_name(step.name()), Some(step));
            assert_eq!(usize::from(step.number()), index + 1);
        }
    }
}
