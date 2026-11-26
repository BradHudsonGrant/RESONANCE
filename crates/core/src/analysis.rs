use crate::identity::AssetAlignment;

/// What an analysis result is cached under. The audio hash it pairs with ignores tags, paths and
/// lossless containers, so renaming, moving or retagging a file finds the same result, while a
/// new analyser version or a changed setting misses and runs again.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnalysisKey {
    pub analyser: String,
    pub version: String,
    /// BLAKE3 of the analyser's settings.
    pub param_hash: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisStatus {
    Done,
    Failed,
}

impl AnalysisStatus {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "done" => Some(Self::Done),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisRun {
    pub key: AnalysisKey,
    pub status: AnalysisStatus,
    /// How sure the analyser is of its result, from 0 to 1. Empty for a failed run.
    pub confidence: Option<f32>,
    pub completed_ns: i64,
}

/// Where the value shown for a track came from. The user's correction always wins over an
/// estimate, and a new estimate never replaces it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalysisProvenance {
    Estimated { confidence: f32 },
    ConfirmedByUser,
}

/// A model may be fetched when its licence allows commercial use, or when the user has turned
/// on models that are free for non-commercial use only (ADR 0005).
pub const fn may_fetch_model(commercial_ok: bool, non_commercial_allowed: bool) -> bool {
    commercial_ok || non_commercial_allowed
}

/// Beat positions in seconds. An automatic grid is in the time of the decoded audio it was made
/// from, and a user's grid is in track time, which is what the library shows.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatGrid {
    pub bpm: f64,
    pub beats_s: Vec<f64>,
    /// Index into `beats_s` of bar 1, beat 1. Beats before it are the pickup.
    pub first_downbeat: u32,
}

impl BeatGrid {
    /// The same grid on a track whose file maps onto track time through `alignment`.
    pub fn asset_to_track(&self, alignment: &AssetAlignment, sample_rate: u32) -> Self {
        Self {
            bpm: self.bpm / alignment.tempo_ratio(),
            beats_s: self
                .beats_s
                .iter()
                .map(|beat| alignment.asset_to_track_seconds(*beat, sample_rate))
                .collect(),
            first_downbeat: self.first_downbeat,
        }
    }
}

/// Beat positions as little-endian `f64` seconds, for a BLOB column.
pub fn encode_beats(beats_s: &[f64]) -> Vec<u8> {
    beats_s.iter().flat_map(|beat| beat.to_le_bytes()).collect()
}

pub fn decode_beats(bytes: &[u8]) -> Option<Vec<f64>> {
    let (chunks, rest) = bytes.as_chunks::<8>();
    if !rest.is_empty() {
        return None;
    }
    Some(
        chunks
            .iter()
            .map(|chunk| f64::from_le_bytes(*chunk))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commercial_models_fetch_by_default() {
        assert!(may_fetch_model(true, false));
        assert!(!may_fetch_model(false, false));
        assert!(may_fetch_model(false, true));
    }

    #[test]
    fn beats_round_trip_through_bytes() {
        let beats = vec![0.0, 0.468_75, 0.937_5, 1e-9];
        assert_eq!(decode_beats(&encode_beats(&beats)), Some(beats));
        assert_eq!(decode_beats(&[0; 7]), None);
    }

    #[test]
    fn a_grid_follows_the_file_offset_and_tempo_into_track_time() {
        let grid = BeatGrid {
            bpm: 128.0,
            beats_s: vec![1.0, 1.468_75],
            first_downbeat: 0,
        };
        let alignment = AssetAlignment::new(44_100, 1.01).expect("alignment");
        let track = grid.asset_to_track(&alignment, 44_100);
        assert!((track.beats_s[0] - 0.0).abs() < 1e-12);
        assert!((track.beats_s[1] - 0.468_75 * 1.01).abs() < 1e-12);
        assert!((track.bpm - 128.0 / 1.01).abs() < 1e-12);
    }
}
