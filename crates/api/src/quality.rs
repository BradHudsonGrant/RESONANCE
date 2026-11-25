//! A track's quality check as the Inspector and Prep show it.

use resonance_core::quality::{FlagCertainty, QualityFlag, QualityKind};
use resonance_db::Database;

use crate::Api;
use crate::command::{QualityCertainty, QualityFlagKind, TrackQuality, TrackQualityFlag};
use crate::error::ApiError;

pub(crate) fn quality_flag(flag: &QualityFlag) -> TrackQualityFlag {
    TrackQualityFlag {
        kind: match flag.kind {
            QualityKind::Damaged => QualityFlagKind::Damaged,
            QualityKind::Clipping => QualityFlagKind::Clipping,
            QualityKind::Dropout => QualityFlagKind::Dropout,
            QualityKind::Transcode => QualityFlagKind::Transcode,
        },
        label: flag.kind.label().to_owned(),
        certainty: match flag.certainty {
            FlagCertainty::Confirmed => QualityCertainty::Confirmed,
            FlagCertainty::Likely => QualityCertainty::Likely,
        },
        reason: flag.reason.clone(),
    }
}

pub(crate) fn quality_of(library: &Database, track_id: i64) -> Result<TrackQuality, ApiError> {
    let stored = library
        .track_quality(track_id)
        .map_err(ApiError::library_unreadable)?;
    let Some(stored) = stored else {
        return Ok(TrackQuality {
            track_id,
            checked: false,
            ..TrackQuality::default()
        });
    };
    let measured = stored.measurements;
    Ok(TrackQuality {
        track_id,
        checked: true,
        integrated_lufs: measured.integrated_lufs,
        loudness_range_lu: measured.loudness_range_lu,
        sample_peak_dbfs: measured.sample_peak_dbfs,
        true_peak_dbtp: measured.true_peak_dbtp,
        crest_low_db: measured.crest_low_db,
        crest_mid_db: measured.crest_mid_db,
        spectral_cutoff_hz: measured.spectral_cutoff_hz,
        flags: stored.flags.iter().map(quality_flag).collect(),
    })
}

impl Api {
    pub(crate) fn track_quality(&self, track_id: i64) -> Result<TrackQuality, ApiError> {
        quality_of(&self.locked_library(), track_id)
    }
}
