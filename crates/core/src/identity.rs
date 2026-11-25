use std::fmt;

use crate::quality::{QualityFlag, QualityMeasurements};

/// Maximum seconds of audio fed into Chromaprint, matching `fpcalc`'s default `-length 120`.
pub const CHROMAPRINT_MAX_SECONDS: u32 = 120;

/// Domain prefix hashed ahead of sample rate, channel count and `i32` little-endian samples.
pub const AUDIO_HASH_DOMAIN: &[u8] = b"resonance-audio-i32le-v1\0";

/// A 32-byte BLAKE3 digest used for `byte_hash` and `audio_hash`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; Self::LEN]);

impl ContentHash {
    pub const LEN: usize = 32;

    pub const fn from_bytes(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Option<Self> {
        let array: [u8; Self::LEN] = bytes.try_into().ok()?;
        Some(Self(array))
    }

    pub const fn as_bytes(&self) -> &[u8; Self::LEN] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(Self::LEN * 2);
        for &byte in &self.0 {
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        out
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        if hex.len() != Self::LEN * 2 {
            return None;
        }
        let mut bytes = [0u8; Self::LEN];
        let (pairs, _) = hex.as_bytes().as_chunks::<2>();
        for (index, &[high_ch, low_ch]) in pairs.iter().enumerate() {
            let high = hex_nibble(high_ch)?;
            let low = hex_nibble(low_ch)?;
            bytes[index] = (high << 4) | low;
        }
        Some(Self(bytes))
    }
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({self})")
    }
}

/// Hash of the normalised decoded audio stream and the decoder version that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioHash {
    pub hash: ContentHash,
    pub decoder: String,
}

/// Chromaprint fingerprint in both AcoustID wire text and raw 32-bit subfingerprints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chromaprint {
    pub encoded: String,
    pub raw: Vec<u32>,
    pub duration_s: u32,
}

impl Chromaprint {
    pub fn raw_le_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.raw.len() * 4);
        for &word in &self.raw {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes
    }

    pub fn raw_from_le_bytes(bytes: &[u8]) -> Option<Vec<u32>> {
        let (words, remainder) = bytes.as_chunks::<4>();
        if !remainder.is_empty() {
            return None;
        }
        Some(words.iter().copied().map(u32::from_le_bytes).collect())
    }
}

/// Stream facts measured directly by decoding every audio packet rather than trusting headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedAudioFacts {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub decode_errors: u32,
    /// `Some(true)` or `Some(false)` when the codec carries a stream checksum such as FLAC MD5.
    pub checksum_ok: Option<bool>,
    /// The length the container states, which a cut-short file no longer reaches.
    pub header_frames: Option<u64>,
    /// Whether the codec is lossless (PCM, FLAC, ALAC), whatever the container.
    pub lossless: bool,
}

impl DecodedAudioFacts {
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        ((u128::from(self.frames) * 1000 + u128::from(self.sample_rate) / 2)
            / u128::from(self.sample_rate)) as u64
    }

    pub fn header_duration_ms(&self) -> Option<u64> {
        let frames = self.header_frames.filter(|&frames| frames > 0)?;
        Some(Self { frames, ..*self }.duration_ms())
    }

    pub fn effective_bitrate_kbps(&self, size_bytes: u64) -> Option<u32> {
        let ms = self.duration_ms();
        if ms == 0 {
            return None;
        }
        u32::try_from((u128::from(size_bytes) * 8) / u128::from(ms)).ok()
    }
}

/// Full identity computed for an audio file on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetIdentity {
    pub byte_hash: ContentHash,
    pub audio: Option<AudioHash>,
    pub chromaprint: Option<Chromaprint>,
    pub decoded_facts: Option<DecodedAudioFacts>,
    /// What the same decode found about the file's quality. `None` when it was not checked.
    pub quality: Option<AssetQuality>,
}

/// A file's quality numbers and the flags they earned.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetQuality {
    pub measurements: QualityMeasurements,
    pub flags: Vec<QualityFlag>,
}

/// Maps between canonical track sample positions and a specific asset's sample positions.
///
/// `offset_samples` is the asset sample index where track sample `0` sits (positive when the
/// asset has leading silence or encoder padding). `tempo_ratio` is track frames per asset frame
/// (`1.0` for digital encodes of the same master).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetAlignment {
    offset_samples: i64,
    tempo_ratio: f64,
}

impl AssetAlignment {
    pub const IDENTITY: Self = Self {
        offset_samples: 0,
        tempo_ratio: 1.0,
    };

    pub fn new(offset_samples: i64, tempo_ratio: f64) -> Option<Self> {
        if !tempo_ratio.is_finite() || tempo_ratio <= 0.0 {
            return None;
        }
        Some(Self {
            offset_samples,
            tempo_ratio,
        })
    }

    pub const fn offset_samples(&self) -> i64 {
        self.offset_samples
    }

    pub const fn tempo_ratio(&self) -> f64 {
        self.tempo_ratio
    }

    pub fn track_to_asset_sample(&self, track_sample: i64) -> i64 {
        let scaled = (track_sample as f64 / self.tempo_ratio).round() as i64;
        self.offset_samples.saturating_add(scaled)
    }

    pub fn asset_to_track_sample(&self, asset_sample: i64) -> i64 {
        let shifted = asset_sample.saturating_sub(self.offset_samples);
        (shifted as f64 * self.tempo_ratio).round() as i64
    }
}

impl Default for AssetAlignment {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// Encodes bytes as unpadded URL-safe Base64 (`A-Za-z0-9-_`), matching `fpcalc` and AcoustID.
pub fn encode_base64_url_unpadded(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let (triples, remainder) = bytes.as_chunks::<3>();
    for &[b0, b1, b2] in triples {
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(char::from(ALPHABET[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 6) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[(n & 0x3f) as usize]));
    }
    if remainder.len() == 1 {
        let n = u32::from(remainder[0]) << 16;
        out.push(char::from(ALPHABET[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 0x3f) as usize]));
    } else if remainder.len() == 2 {
        let n = (u32::from(remainder[0]) << 16) | (u32::from(remainder[1]) << 8);
        out.push(char::from(ALPHABET[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 6) & 0x3f) as usize]));
    }
    out
}

/// Encodes bytes as standard RFC 4648 padded Base64 (`+`, `/`, `=`).
pub fn encode_base64_standard(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let (triples, remainder) = bytes.as_chunks::<3>();
    for &[b0, b1, b2] in triples {
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(char::from(ALPHABET[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 6) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[(n & 0x3f) as usize]));
    }
    if remainder.len() == 1 {
        let n = u32::from(remainder[0]) << 16;
        out.push(char::from(ALPHABET[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 0x3f) as usize]));
        out.push_str("==");
    } else if remainder.len() == 2 {
        let n = (u32::from(remainder[0]) << 16) | (u32::from(remainder[1]) << 8);
        out.push(char::from(ALPHABET[((n >> 18) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 12) & 0x3f) as usize]));
        out.push(char::from(ALPHABET[((n >> 6) & 0x3f) as usize]));
        out.push('=');
    }
    out
}

/// Sliding-window Chromaprint comparison of a 30 s preview excerpt against a full track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewMatch {
    pub similarity: f32,
    pub best_offset_frames: usize,
    pub background_median: f32,
    pub peak_margin: f32,
    pub confirmed: bool,
}

pub const PREVIEW_MIN_SIMILARITY: f32 = 0.82;
pub const PREVIEW_MIN_PEAK_MARGIN: f32 = 0.14;
const PREVIEW_TRIM_START: usize = 12;
const PREVIEW_TRIM_END: usize = 16;
const PREVIEW_MIN_CORE_FRAMES: usize = 60;
const PREVIEW_GUARD_FRAMES: usize = 20;
const PREVIEW_MIN_FRAME_ENTROPY: f32 = 2.0;

/// Slides a fade-trimmed 30 s `preview_raw` fingerprint over `track_raw`, verifying both peak
/// similarity (`>= 0.82`) and separation above the track's own non-aligned background median
/// (`median + 0.14`) so repeated drops do not suppress a true match while flat drones fail.
pub fn match_preview_fingerprint(track_raw: &[u32], preview_raw: &[u32]) -> Option<PreviewMatch> {
    if preview_raw.len() < PREVIEW_TRIM_START + PREVIEW_TRIM_END + PREVIEW_MIN_CORE_FRAMES {
        return None;
    }
    let core = &preview_raw[PREVIEW_TRIM_START..preview_raw.len() - PREVIEW_TRIM_END];
    if core.len() < PREVIEW_MIN_CORE_FRAMES || track_raw.len() < core.len() {
        return None;
    }

    let transitions: u64 = core
        .windows(2)
        .map(|pair| u64::from((pair[0] ^ pair[1]).count_ones()))
        .sum();
    let mean_entropy = transitions as f32 / (core.len() - 1) as f32;
    if mean_entropy < PREVIEW_MIN_FRAME_ENTROPY {
        return None;
    }

    let max_offset = track_raw.len() - core.len();
    let total_bits = (core.len() as u64) * 32;
    let mut scores = Vec::with_capacity(max_offset + 1);
    let mut best_sim = 0.0_f32;
    let mut best_offset = 0usize;

    for offset in 0..=max_offset {
        let diff_bits: u64 = track_raw[offset..offset + core.len()]
            .iter()
            .zip(core)
            .map(|(&a, &b)| u64::from((a ^ b).count_ones()))
            .sum();
        let sim = 1.0 - (diff_bits as f32 / total_bits as f32);
        if sim > best_sim {
            best_sim = sim;
            best_offset = offset;
        }
        scores.push(sim);
    }

    let mut background: Vec<f32> = scores
        .into_iter()
        .enumerate()
        .filter(|(offset, _)| offset.abs_diff(best_offset) > PREVIEW_GUARD_FRAMES)
        .map(|(_, score)| score)
        .collect();
    let background_median = if background.len() >= 10 {
        background.sort_by(f32::total_cmp);
        background[background.len() / 2]
    } else {
        0.55
    };
    let peak_margin = best_sim - background_median;
    let confirmed = best_sim >= PREVIEW_MIN_SIMILARITY && peak_margin >= PREVIEW_MIN_PEAK_MARGIN;

    Some(PreviewMatch {
        similarity: best_sim,
        best_offset_frames: best_offset,
        background_median,
        peak_margin,
        confirmed,
    })
}

/// Best bitwise agreement in `[0.0, 1.0]` between two raw Chromaprint subfingerprint sequences,
/// searching relative shifts up to `±max_shift` items.
pub fn fingerprint_similarity(left: &[u32], right: &[u32], max_shift: usize) -> f32 {
    const MIN_OVERLAP: usize = 16;
    if left.len() < MIN_OVERLAP || right.len() < MIN_OVERLAP {
        return 0.0;
    }
    let mut best = 0.0_f32;
    let limit = isize::try_from(max_shift).unwrap_or(isize::MAX);
    for shift in -limit..=limit {
        let (left_start, right_start) = if shift >= 0 {
            (shift as usize, 0usize)
        } else {
            (0usize, (-shift) as usize)
        };
        if left_start >= left.len() || right_start >= right.len() {
            continue;
        }
        let overlap = (left.len() - left_start).min(right.len() - right_start);
        if overlap < MIN_OVERLAP {
            continue;
        }
        let differing_bits: u64 = left[left_start..left_start + overlap]
            .iter()
            .zip(&right[right_start..right_start + overlap])
            .map(|(&a, &b)| u64::from((a ^ b).count_ones()))
            .sum();
        let total_bits = (overlap as u64) * 32;
        let similarity = 1.0 - (differing_bits as f32 / total_bits as f32);
        if similarity > best {
            best = similarity;
        }
    }
    best
}

/// Minimum Chromaprint bitwise agreement required to treat another encode as the same recording.
pub const RELINK_MIN_FINGERPRINT_SIMILARITY: f32 = 0.85;

/// Maximum duration difference allowed when matching another encode by Chromaprint.
pub const RELINK_MAX_DURATION_DELTA_MS: u64 = 3_000;

/// Maximum Chromaprint subfingerprint shift searched during relink comparison (~4 s at ~0.124 s/item).
pub const RELINK_MAX_FINGERPRINT_SHIFT: usize = 32;

/// Identity facts compared between a missing asset and a candidate file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelinkIdentityRef<'a> {
    pub byte_hash: Option<ContentHash>,
    pub audio_hash: Option<ContentHash>,
    pub chromaprint_raw: Option<&'a [u32]>,
    pub duration_ms: Option<u64>,
}

/// Match verdict between a missing track's stored identity and a candidate file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RelinkMatchKind {
    ExactBytes,
    ExactAudio,
    SameRecording { similarity: f32 },
    DifferentRecording { similarity: Option<f32> },
}

impl RelinkMatchKind {
    pub fn confidence(self) -> f32 {
        match self {
            Self::ExactBytes | Self::ExactAudio => 1.0,
            Self::SameRecording { similarity } => similarity,
            Self::DifferentRecording { similarity } => similarity.unwrap_or(0.0),
        }
    }

    pub fn is_auto_candidate(self) -> bool {
        matches!(
            self,
            Self::ExactBytes | Self::ExactAudio | Self::SameRecording { .. }
        )
    }

    pub fn requires_confirmation(self) -> bool {
        matches!(self, Self::DifferentRecording { .. })
    }

    pub fn tier(self) -> u8 {
        match self {
            Self::ExactBytes => 0,
            Self::ExactAudio => 1,
            Self::SameRecording { .. } => 2,
            Self::DifferentRecording { .. } => 3,
        }
    }
}

/// Evaluates how closely `candidate` matches `missing` along the identity ladder:
/// exact `byte_hash`, exact `audio_hash`, or `fingerprint_similarity >= 0.85` within `±3.0 s`.
pub fn evaluate_relink_match(
    missing: RelinkIdentityRef<'_>,
    candidate: RelinkIdentityRef<'_>,
) -> RelinkMatchKind {
    if let (Some(left), Some(right)) = (missing.byte_hash, candidate.byte_hash)
        && left == right
    {
        return RelinkMatchKind::ExactBytes;
    }
    if let (Some(left), Some(right)) = (missing.audio_hash, candidate.audio_hash)
        && left == right
    {
        return RelinkMatchKind::ExactAudio;
    }

    let similarity = match (missing.chromaprint_raw, candidate.chromaprint_raw) {
        (Some(left), Some(right)) if !left.is_empty() && !right.is_empty() => Some(
            fingerprint_similarity(left, right, RELINK_MAX_FINGERPRINT_SHIFT),
        ),
        _ => None,
    };

    let duration_close = match (missing.duration_ms, candidate.duration_ms) {
        (Some(left), Some(right)) => left.abs_diff(right) <= RELINK_MAX_DURATION_DELTA_MS,
        _ => true,
    };

    if let Some(score) = similarity
        && score >= RELINK_MIN_FINGERPRINT_SIMILARITY
        && duration_close
    {
        return RelinkMatchKind::SameRecording { similarity: score };
    }

    RelinkMatchKind::DifferentRecording { similarity }
}

/// Orders two relink verdicts from strongest (`ExactBytes`) to weakest (`DifferentRecording`).
pub fn compare_relink_matches(left: RelinkMatchKind, right: RelinkMatchKind) -> std::cmp::Ordering {
    left.tier()
        .cmp(&right.tier())
        .then_with(|| right.confidence().total_cmp(&left.confidence()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_round_trips_through_hex_and_slices() {
        let hash = ContentHash::from_bytes([0x5a; 32]);
        let hex = hash.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(ContentHash::from_hex(&hex), Some(hash));
        assert_eq!(ContentHash::from_hex(&hex.to_uppercase()), Some(hash));
        assert_eq!(ContentHash::from_slice(hash.as_bytes()), Some(hash));
        assert_eq!(ContentHash::from_hex("short"), None);
        assert_eq!(ContentHash::from_slice(&[0u8; 31]), None);
    }

    #[test]
    fn alignment_maps_samples_both_ways_with_offset_and_tempo_ratio() {
        assert_eq!(AssetAlignment::new(0, 0.0), None);
        assert_eq!(AssetAlignment::new(0, -1.0), None);
        assert_eq!(AssetAlignment::new(0, f64::NAN), None);

        let padded = AssetAlignment::new(528, 1.0).expect("valid alignment");
        assert_eq!(padded.track_to_asset_sample(0), 528);
        assert_eq!(padded.asset_to_track_sample(528), 0);
        assert_eq!(padded.track_to_asset_sample(44_100), 44_628);
        assert_eq!(padded.asset_to_track_sample(44_628), 44_100);

        let pitched = AssetAlignment::new(100, 1.02).expect("valid alignment");
        let asset_pos = pitched.track_to_asset_sample(102_000);
        assert_eq!(asset_pos, 100_100);
        assert_eq!(pitched.asset_to_track_sample(asset_pos), 102_000);
    }

    #[test]
    fn url_safe_base64_omits_padding_for_every_remainder_length() {
        assert_eq!(encode_base64_url_unpadded(b""), "");
        assert_eq!(encode_base64_url_unpadded(&[0xfb]), "-w");
        assert_eq!(encode_base64_url_unpadded(&[0xfb, 0xff]), "-_8");
        assert_eq!(encode_base64_url_unpadded(&[0x01, 0x00, 0x00]), "AQAA");
    }

    #[test]
    fn fingerprint_similarity_finds_shifted_matches_and_rejects_unrelated_noise() {
        let base: Vec<u32> = (0..120)
            .map(|i: u32| i.wrapping_mul(0x9e37_79b9).rotate_left(7))
            .collect();
        let mut shifted_slightly_noisy = vec![0xdead_beef, 0x1234_5678, 0xCAFE_BABE];
        shifted_slightly_noisy.extend(base.iter().map(|&word| word ^ 0b101));

        let same_score = fingerprint_similarity(&base, &shifted_slightly_noisy, 8);
        assert!(
            same_score > 0.90,
            "expected high similarity, got {same_score}"
        );

        let unrelated: Vec<u32> = (0..120)
            .map(|i: u32| (i ^ 0x55aa_55aa).wrapping_mul(0x85eb_ca6b).rotate_right(11))
            .collect();
        let diff_score = fingerprint_similarity(&base, &unrelated, 8);
        assert!(
            diff_score < 0.65,
            "expected near-chance similarity, got {diff_score}"
        );
    }

    #[test]
    fn relink_ladder_ranks_bytes_then_audio_then_fingerprint_and_flags_mismatches() {
        let fp_a: Vec<u32> = (0..120)
            .map(|i: u32| i.wrapping_mul(0x9e37_79b9).rotate_left(7))
            .collect();
        let fp_a_lossy: Vec<u32> = fp_a.iter().map(|&w| w ^ 0b11).collect();
        let fp_other: Vec<u32> = (0..120)
            .map(|i: u32| (i ^ 0x55aa_55aa).wrapping_mul(0x85eb_ca6b).rotate_right(11))
            .collect();

        let missing = RelinkIdentityRef {
            byte_hash: Some(ContentHash::from_bytes([0x11; 32])),
            audio_hash: Some(ContentHash::from_bytes([0x22; 32])),
            chromaprint_raw: Some(&fp_a),
            duration_ms: Some(180_000),
        };

        let exact_bytes = evaluate_relink_match(
            missing,
            RelinkIdentityRef {
                byte_hash: Some(ContentHash::from_bytes([0x11; 32])),
                audio_hash: Some(ContentHash::from_bytes([0x99; 32])),
                chromaprint_raw: None,
                duration_ms: Some(180_000),
            },
        );
        assert_eq!(exact_bytes, RelinkMatchKind::ExactBytes);
        assert!(exact_bytes.is_auto_candidate());
        assert!(!exact_bytes.requires_confirmation());

        let exact_audio = evaluate_relink_match(
            missing,
            RelinkIdentityRef {
                byte_hash: Some(ContentHash::from_bytes([0x33; 32])),
                audio_hash: Some(ContentHash::from_bytes([0x22; 32])),
                chromaprint_raw: None,
                duration_ms: Some(180_000),
            },
        );
        assert_eq!(exact_audio, RelinkMatchKind::ExactAudio);

        let same_recording = evaluate_relink_match(
            missing,
            RelinkIdentityRef {
                byte_hash: Some(ContentHash::from_bytes([0x44; 32])),
                audio_hash: Some(ContentHash::from_bytes([0x55; 32])),
                chromaprint_raw: Some(&fp_a_lossy),
                duration_ms: Some(182_400),
            },
        );
        assert!(matches!(
            same_recording,
            RelinkMatchKind::SameRecording { similarity } if similarity > 0.90
        ));
        assert!(same_recording.is_auto_candidate());

        let duration_too_far = evaluate_relink_match(
            missing,
            RelinkIdentityRef {
                byte_hash: Some(ContentHash::from_bytes([0x44; 32])),
                audio_hash: Some(ContentHash::from_bytes([0x55; 32])),
                chromaprint_raw: Some(&fp_a_lossy),
                duration_ms: Some(184_500),
            },
        );
        assert!(duration_too_far.requires_confirmation());

        let different = evaluate_relink_match(
            missing,
            RelinkIdentityRef {
                byte_hash: Some(ContentHash::from_bytes([0x66; 32])),
                audio_hash: Some(ContentHash::from_bytes([0x77; 32])),
                chromaprint_raw: Some(&fp_other),
                duration_ms: Some(180_000),
            },
        );
        assert!(different.requires_confirmation());
        assert!(!different.is_auto_candidate());

        let mut ranked = [different, same_recording, exact_audio, exact_bytes];
        ranked.sort_by(|a, b| compare_relink_matches(*a, *b));
        assert_eq!(ranked[0], exact_bytes);
        assert_eq!(ranked[1], exact_audio);
        assert_eq!(ranked[2], same_recording);
        assert_eq!(ranked[3], different);
    }

    #[test]
    fn preview_sliding_matcher_finds_offset_slices_and_rejects_flat_or_random() {
        let track: Vec<u32> = (0..1800)
            .map(|i: u32| {
                i.wrapping_mul(0x9e37_79b9).rotate_left((i % 17) + 1)
                    ^ (i.wrapping_mul(0x517c_c1b7))
            })
            .collect();

        // Embedded 240-frame slice at offset 640 with mild lossy bit noise.
        let mut preview = vec![0x1111_2222; 12];
        preview.extend(track[640..860].iter().map(|&w| w ^ 0b101));
        preview.extend(std::iter::repeat_n(0x3333_4444, 16));

        let matched = match_preview_fingerprint(&track, &preview).expect("slice analysed");
        assert!(matched.confirmed);
        assert_eq!(matched.best_offset_frames, 640);
        assert!(matched.similarity >= 0.90);
        assert!(matched.peak_margin >= PREVIEW_MIN_PEAK_MARGIN);

        // Flat / zero-entropy preview returns None.
        let flat = vec![0xaaaa_5555u32; 220];
        assert_eq!(match_preview_fingerprint(&track, &flat), None);

        // Unrelated high-entropy preview never confirms.
        let unrelated: Vec<u32> = (0..220)
            .map(|i: u32| (i ^ 0xDEAD_BEEF).wrapping_mul(0x85eb_ca6b).rotate_right(9))
            .collect();
        let rejected = match_preview_fingerprint(&track, &unrelated).expect("scored");
        assert!(!rejected.confirmed);
    }

    #[test]
    fn standard_base64_pads_every_remainder() {
        assert_eq!(encode_base64_standard(b"a"), "YQ==");
        assert_eq!(encode_base64_standard(b"ab"), "YWI=");
        assert_eq!(encode_base64_standard(b"abc"), "YWJj");
    }
}
