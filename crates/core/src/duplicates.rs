//! Sorting two files into copies of one recording or versions of one song.
//!
//! A copy is the same recording: the same bytes, the same decoded audio, or another encode of
//! the same master. A version is a different cut of a song: a radio edit, an extended mix, a dub
//! or a remix. Versions are never called duplicates, and one is never offered for removal.

use std::cmp::Ordering;

use crate::identity::{ContentHash, RELINK_MAX_FINGERPRINT_SHIFT, fingerprint_similarity};
use crate::library::AudioFormat;

/// Least Chromaprint agreement for another encode to count as the same recording.
pub const SAME_RECORDING_MIN_SIMILARITY: f32 = 0.88;

/// Most two encodes of one master may differ in length. Encoder padding and trimmed silence
/// stay well inside it.
pub const SAME_RECORDING_MAX_DURATION_DELTA_MS: u64 = 2_500;

/// Least difference in length that makes two cuts of a song different versions on its own.
pub const VERSION_MIN_DURATION_DELTA_MS: u64 = 3_500;

/// Least run of matching fingerprint, about 10 s, that shows two files share material.
pub const SHARED_SEGMENT_MIN_ITEMS: usize = 80;

/// Window, about 4 s, over which a shared run must agree.
const SEGMENT_WINDOW: usize = 32;

/// Agreement each window of a shared run must reach. Below the copy threshold, because a
/// version is often mastered or mixed differently.
const SEGMENT_MIN_AGREEMENT: f32 = 0.85;

/// Words in a title that mark a version. "Original mix" names no version, so it is not one.
const VERSION_WORDS: [&str; 15] = [
    "edit",
    "extended",
    "dub",
    "instrumental",
    "vip",
    "remix",
    "mix",
    "rework",
    "bootleg",
    "acapella",
    "acappella",
    "version",
    "club",
    "radio",
    "reprise",
];

/// How a file is encoded, enough to say which of two copies to keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Encoding {
    pub lossless: bool,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    /// The quality check found damage or a lossy source behind a lossless file.
    pub flagged: bool,
}

impl Encoding {
    /// An M4A file is lossless when it has a bit depth, which only ALAC reports.
    pub fn of(
        format: AudioFormat,
        bitrate_kbps: Option<u32>,
        sample_rate: Option<u32>,
        bit_depth: Option<u8>,
        flagged: bool,
    ) -> Self {
        let lossless = match format {
            AudioFormat::Mp3 => false,
            AudioFormat::M4a => bit_depth.is_some(),
            AudioFormat::Flac | AudioFormat::Aiff | AudioFormat::Wav => true,
        };
        Self {
            lossless,
            bitrate_kbps,
            sample_rate,
            bit_depth,
            flagged,
        }
    }
}

/// What is known about one file when comparing it with another.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CopyFacts<'a> {
    pub byte_hash: Option<ContentHash>,
    pub audio_hash: Option<ContentHash>,
    pub fingerprint: Option<&'a [u32]>,
    pub duration_ms: Option<u64>,
    pub artist: Option<&'a str>,
    pub title: Option<&'a str>,
    pub encoding: Encoding,
}

/// Why two files are versions rather than copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionEvidence {
    /// The titles name different mixes, such as a radio edit and an extended mix.
    MixNames,
    /// The lengths are more than 3.5 s apart.
    Length,
}

impl VersionEvidence {
    pub const fn label(self) -> &'static str {
        match self {
            Self::MixNames => "Different mix",
            Self::Length => "Different length",
        }
    }
}

/// How two files relate, from exact to fuzzy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PairKind {
    ByteIdentical,
    /// The same decoded audio in different bytes: a retag, new artwork, WAV against FLAC.
    AudioIdentical,
    /// Another encode of the same master.
    SameRecording {
        similarity: f32,
    },
    /// Another cut of the same song. Never a duplicate.
    DifferentVersion {
        evidence: VersionEvidence,
    },
    /// Nothing ties the two firmly enough to call them either.
    Unrelated,
}

impl PairKind {
    /// Copies are the same recording, so all but one of them can go.
    pub const fn is_copy(self) -> bool {
        matches!(
            self,
            Self::ByteIdentical | Self::AudioIdentical | Self::SameRecording { .. }
        )
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::ByteIdentical => "Same file",
            Self::AudioIdentical => "Same audio",
            Self::SameRecording { .. } => "Another encode",
            Self::DifferentVersion { evidence } => evidence.label(),
            Self::Unrelated => "Unrelated",
        }
    }

    pub const fn is_version(self) -> bool {
        matches!(self, Self::DifferentVersion { .. })
    }
}

/// Sorts two files along the identity ladder: equal bytes, equal decoded audio, another encode
/// of the same recording, then a different version of the same song.
///
/// Another encode needs Chromaprint agreement of at least 0.88, lengths within 2.5 s and titles
/// that name the same mix. A version needs shared material (a 10 s run of matching fingerprint)
/// or the same artist and title, plus lengths more than 3.5 s apart or different mix names.
pub fn classify_asset_pair(left: &CopyFacts<'_>, right: &CopyFacts<'_>) -> PairKind {
    if let (Some(a), Some(b)) = (left.byte_hash, right.byte_hash)
        && a == b
    {
        return PairKind::ByteIdentical;
    }
    if let (Some(a), Some(b)) = (left.audio_hash, right.audio_hash)
        && a == b
    {
        return PairKind::AudioIdentical;
    }

    let mixes_differ = left.title.and_then(mix_name) != right.title.and_then(mix_name);
    let delta = match (left.duration_ms, right.duration_ms) {
        (Some(a), Some(b)) => Some(a.abs_diff(b)),
        _ => None,
    };
    let fingerprints = match (left.fingerprint, right.fingerprint) {
        (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => Some((a, b)),
        _ => None,
    };

    if !mixes_differ
        && delta.is_some_and(|delta| delta <= SAME_RECORDING_MAX_DURATION_DELTA_MS)
        && let Some((a, b)) = fingerprints
    {
        let similarity = fingerprint_similarity(a, b, RELINK_MAX_FINGERPRINT_SHIFT);
        if similarity >= SAME_RECORDING_MIN_SIMILARITY {
            return PairKind::SameRecording { similarity };
        }
    }

    let evidence = if mixes_differ {
        VersionEvidence::MixNames
    } else if delta.is_some_and(|delta| delta > VERSION_MIN_DURATION_DELTA_MS) {
        VersionEvidence::Length
    } else {
        return PairKind::Unrelated;
    };
    let related =
        same_song(left, right) || fingerprints.is_some_and(|(a, b)| shared_segment(a, b).is_some());
    if related {
        PairKind::DifferentVersion { evidence }
    } else {
        PairKind::Unrelated
    }
}

/// The same artist and the same title once mix names are set aside.
pub fn same_song(left: &CopyFacts<'_>, right: &CopyFacts<'_>) -> bool {
    let (Some(left_artist), Some(right_artist)) = (left.artist, right.artist) else {
        return false;
    };
    let (Some(left_title), Some(right_title)) = (left.title, right.title) else {
        return false;
    };
    let left_artist = normalised(left_artist);
    let left_title = base_title(left_title);
    !left_artist.is_empty()
        && !left_title.is_empty()
        && left_artist == normalised(right_artist)
        && left_title == base_title(right_title)
}

/// A run of fingerprint two files share, in Chromaprint items (about 0.124 s each).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharedSegment {
    pub left_start: usize,
    pub right_start: usize,
    pub items: usize,
    pub agreement: f32,
}

/// The longest run, at any offset, where every 4 s window of the two fingerprints agrees on at
/// least 85% of bits. `None` unless it lasts about 10 s, so a shared drum loop is not enough.
pub fn shared_segment(left: &[u32], right: &[u32]) -> Option<SharedSegment> {
    if left.len() < SHARED_SEGMENT_MIN_ITEMS || right.len() < SHARED_SEGMENT_MIN_ITEMS {
        return None;
    }
    let threshold = (SEGMENT_MIN_AGREEMENT * (SEGMENT_WINDOW * 32) as f32).ceil() as u32;
    let mut best: Option<SharedSegment> = None;
    let mut agreeing: Vec<u32> = Vec::with_capacity(left.len().min(right.len()));

    let shifts = -(right.len() as isize - 1)..=(left.len() as isize - 1);
    for shift in shifts {
        let (left_start, right_start) = if shift >= 0 {
            (shift.unsigned_abs(), 0)
        } else {
            (0, shift.unsigned_abs())
        };
        let overlap = (left.len() - left_start).min(right.len() - right_start);
        if overlap < SHARED_SEGMENT_MIN_ITEMS
            || best.is_some_and(|segment| segment.items >= overlap)
        {
            continue;
        }
        agreeing.clear();
        agreeing.extend(
            left[left_start..left_start + overlap]
                .iter()
                .zip(&right[right_start..right_start + overlap])
                .map(|(&a, &b)| 32 - (a ^ b).count_ones()),
        );

        let mut window: u32 = agreeing[..SEGMENT_WINDOW].iter().sum();
        let mut run_start: Option<usize> = None;
        for start in 0..=overlap - SEGMENT_WINDOW {
            if start > 0 {
                window = window - agreeing[start - 1] + agreeing[start + SEGMENT_WINDOW - 1];
            }
            let good = window >= threshold;
            if good && run_start.is_none() {
                run_start = Some(start);
            }
            let run_ends = !good || start == overlap - SEGMENT_WINDOW;
            if let (true, Some(first)) = (run_ends, run_start) {
                let last = if good { start } else { start - 1 };
                let items = last - first + SEGMENT_WINDOW;
                if items >= SHARED_SEGMENT_MIN_ITEMS
                    && best.is_none_or(|segment| items > segment.items)
                {
                    let bits: u32 = agreeing[first..first + items].iter().sum();
                    best = Some(SharedSegment {
                        left_start: left_start + first,
                        right_start: right_start + first,
                        items,
                        agreement: bits as f32 / (items * 32) as f32,
                    });
                }
                run_start = None;
            }
        }
    }
    best
}

/// Orders two copies of one recording from the one to keep to the one to let go: unflagged
/// before flagged, lossless before lossy, then higher bitrate, sample rate and bit depth.
pub fn compare_copies(left: &Encoding, right: &Encoding) -> Ordering {
    left.flagged
        .cmp(&right.flagged)
        .then_with(|| right.lossless.cmp(&left.lossless))
        .then_with(|| {
            if left.lossless {
                Ordering::Equal
            } else {
                right.bitrate_kbps.cmp(&left.bitrate_kbps)
            }
        })
        .then_with(|| right.sample_rate.cmp(&left.sample_rate))
        .then_with(|| right.bit_depth.cmp(&left.bit_depth))
}

/// The mix a title names, such as `radio edit`, `extended` or `someone remix`, in lower case.
/// `None` for a title that names no mix or the original mix.
pub fn mix_name(title: &str) -> Option<String> {
    let (_, mixes) = split_title(title);
    let joined = mixes
        .into_iter()
        .filter_map(|part| {
            let mut words: Vec<&str> = part.split(' ').collect();
            if words.len() > 1 && words.last() == Some(&"mix") {
                words.pop();
            }
            let name = words.join(" ");
            (name != "original").then_some(name)
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!joined.is_empty()).then_some(joined)
}

/// The title with any mix names taken off, in lower case.
pub fn base_title(title: &str) -> String {
    split_title(title).0
}

/// Splits a title into its base and the bracketed or dashed parts that name a mix.
fn split_title(title: &str) -> (String, Vec<String>) {
    let mut base = String::new();
    let mut mixes = Vec::new();
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[']) {
        let close = if rest.as_bytes()[open] == b'(' {
            ')'
        } else {
            ']'
        };
        let Some(length) = rest[open + 1..].find(close) else {
            break;
        };
        let inner = normalised(&rest[open + 1..open + 1 + length]);
        base.push_str(&rest[..open]);
        if names_a_mix(&inner) {
            mixes.push(inner);
        } else {
            base.push(' ');
            base.push_str(&inner);
        }
        rest = &rest[open + 1 + length + 1..];
    }
    base.push_str(rest);

    if let Some(dash) = base.rfind(" - ") {
        let tail = normalised(&base[dash + 3..]);
        if names_a_mix(&tail) {
            mixes.insert(0, tail);
            base.truncate(dash);
        }
    }
    (normalised(&base), mixes)
}

fn names_a_mix(part: &str) -> bool {
    part.split(' ').any(|word| VERSION_WORDS.contains(&word))
}

/// Lower case, with punctuation other than `'` and `&` read as a space and runs of space closed.
fn normalised(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() || ch == '\'' || ch == '&' {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
    }
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fingerprint that does not repeat, so it only matches itself at offset 0.
    fn fingerprint(seed: u32, items: usize) -> Vec<u32> {
        let mut state = seed | 1;
        (0..items)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state
            })
            .collect()
    }

    /// The same fingerprint as another encode reads it: two bits in 32 flipped.
    fn reencoded(raw: &[u32]) -> Vec<u32> {
        raw.iter().map(|&word| word ^ 0x8000_0001).collect()
    }

    fn facts<'a>(fingerprint: &'a [u32], seconds: f64, title: &'a str) -> CopyFacts<'a> {
        CopyFacts {
            fingerprint: Some(fingerprint),
            duration_ms: Some((seconds * 1000.0) as u64),
            artist: Some("Ada Marsh"),
            title: Some(title),
            ..CopyFacts::default()
        }
    }

    #[test]
    fn equal_bytes_and_equal_audio_come_first() {
        let raw = fingerprint(7, 900);
        let mut left = facts(&raw, 360.0, "First Light");
        let mut right = facts(&raw, 360.0, "First Light");
        left.byte_hash = Some(ContentHash::from_bytes([1; 32]));
        right.byte_hash = Some(ContentHash::from_bytes([1; 32]));
        assert_eq!(classify_asset_pair(&left, &right), PairKind::ByteIdentical);

        right.byte_hash = Some(ContentHash::from_bytes([2; 32]));
        left.audio_hash = Some(ContentHash::from_bytes([3; 32]));
        right.audio_hash = Some(ContentHash::from_bytes([3; 32]));
        let kind = classify_asset_pair(&left, &right);
        assert_eq!(kind, PairKind::AudioIdentical);
        assert!(kind.is_copy());
    }

    #[test]
    fn another_encode_of_the_same_master_is_the_same_recording() {
        let flac = fingerprint(11, 968);
        let mp3 = reencoded(&flac);
        let kind = classify_asset_pair(
            &facts(&flac, 301.0, "First Light (Extended Mix)"),
            &facts(&mp3, 302.4, "First Light - Extended"),
        );
        assert!(
            matches!(kind, PairKind::SameRecording { similarity } if similarity > 0.9),
            "{kind:?}"
        );
        assert!(kind.is_copy());
    }

    #[test]
    fn the_same_audio_with_different_mix_names_is_a_version() {
        let raw = fingerprint(13, 968);
        let other = reencoded(&raw);
        let kind = classify_asset_pair(
            &facts(&raw, 240.0, "First Light (Dub)"),
            &facts(&other, 240.0, "First Light (Instrumental)"),
        );
        assert_eq!(
            kind,
            PairKind::DifferentVersion {
                evidence: VersionEvidence::MixNames
            }
        );
        assert!(!kind.is_copy());
    }

    #[test]
    fn a_radio_edit_and_an_extended_mix_of_one_stem_are_versions() {
        // The first 120 s of each: the extended mix from the top, the radio edit from 60 s in.
        let stem = fingerprint(17, 2_900);
        let extended = stem[..968].to_vec();
        let radio = reencoded(&stem[484..1_452]);
        let segment = shared_segment(&extended, &radio).expect("shared material");
        assert_eq!(segment.left_start, 484);
        assert_eq!(segment.right_start, 0);
        assert!(segment.items >= 480, "{segment:?}");

        // Tagged with mix names.
        let kind = classify_asset_pair(
            &facts(&radio, 180.0, "First Light (Radio Edit)"),
            &facts(&extended, 360.0, "First Light (Extended Mix)"),
        );
        assert_eq!(
            kind,
            PairKind::DifferentVersion {
                evidence: VersionEvidence::MixNames
            }
        );

        // Untagged: no artist, no mix names, only the shared material and the length.
        let untagged = |raw, seconds| CopyFacts {
            fingerprint: Some(raw),
            duration_ms: Some(seconds),
            ..CopyFacts::default()
        };
        assert_eq!(
            classify_asset_pair(&untagged(&radio, 180_000), &untagged(&extended, 360_000)),
            PairKind::DifferentVersion {
                evidence: VersionEvidence::Length
            }
        );
    }

    #[test]
    fn a_different_length_file_with_the_same_name_is_a_version_never_a_copy() {
        let full = fingerprint(19, 968);
        let other = fingerprint(23, 968);
        let kind = classify_asset_pair(
            &facts(&full, 300.0, "First Light"),
            &facts(&other, 305.0, "first light"),
        );
        assert_eq!(
            kind,
            PairKind::DifferentVersion {
                evidence: VersionEvidence::Length
            }
        );
    }

    #[test]
    fn unrelated_songs_and_the_gap_between_thresholds_are_neither() {
        let one = fingerprint(29, 968);
        let two = fingerprint(31, 968);
        assert_eq!(
            classify_asset_pair(
                &facts(&one, 300.0, "First Light"),
                &facts(&two, 300.0, "Low Tide")
            ),
            PairKind::Unrelated
        );
        // The same song 3 s longer is too long for an encode and too short for an edit.
        let longer = reencoded(&one);
        assert_eq!(
            classify_asset_pair(
                &facts(&one, 300.0, "First Light"),
                &facts(&longer, 303.0, "First Light")
            ),
            PairKind::Unrelated
        );
    }

    #[test]
    fn a_short_shared_loop_is_not_shared_material() {
        let one = fingerprint(37, 968);
        let mut two = fingerprint(41, 968);
        two[300..340].copy_from_slice(&one[100..140]);
        assert_eq!(shared_segment(&one, &two), None);
    }

    #[test]
    fn mix_names_are_read_from_brackets_and_dashes() {
        assert_eq!(
            mix_name("First Light (Radio Edit)").as_deref(),
            Some("radio edit")
        );
        assert_eq!(
            mix_name("First Light [Extended Mix]").as_deref(),
            Some("extended")
        );
        assert_eq!(
            mix_name("First Light - Extended").as_deref(),
            Some("extended")
        );
        assert_eq!(
            mix_name("First Light (Kai Rowe Remix)").as_deref(),
            Some("kai rowe remix")
        );
        assert_eq!(mix_name("First Light (VIP)").as_deref(), Some("vip"));
        assert_eq!(mix_name("First Light (Original Mix)"), None);
        assert_eq!(mix_name("First Light (feat. Jo Vale)"), None);
        assert_eq!(mix_name("Dub Theory"), None);
        assert_eq!(
            base_title("First Light (feat. Jo Vale) [Dub]"),
            "first light feat jo vale"
        );
        assert_eq!(base_title("First Light - Radio Edit"), "first light");
    }

    #[test]
    fn copies_rank_unflagged_lossless_then_bitrate() {
        let flac = Encoding {
            lossless: true,
            sample_rate: Some(44_100),
            bit_depth: Some(16),
            bitrate_kbps: Some(900),
            ..Encoding::default()
        };
        let hires = Encoding {
            sample_rate: Some(96_000),
            bit_depth: Some(24),
            ..flac
        };
        let mp3_320 = Encoding {
            lossless: false,
            bitrate_kbps: Some(320),
            sample_rate: Some(44_100),
            bit_depth: None,
            flagged: false,
        };
        let mp3_192 = Encoding {
            bitrate_kbps: Some(192),
            ..mp3_320
        };
        let fake_flac = Encoding {
            flagged: true,
            ..hires
        };
        let mut ranked = [fake_flac, mp3_192, flac, mp3_320, hires];
        ranked.sort_by(compare_copies);
        assert_eq!(ranked, [hires, flac, mp3_320, mp3_192, fake_flac]);
    }
}
