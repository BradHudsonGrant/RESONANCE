//! Convert and Repair: what each named preset makes, and how loud a converted file may be.

use crate::library::AudioFormat;

/// A conversion that says exactly what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConvertPreset {
    Mp3Cbr320,
    Flac24,
    AiffCdj,
}

impl ConvertPreset {
    pub const ALL: [Self; 3] = [Self::Mp3Cbr320, Self::Flac24, Self::AiffCdj];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Mp3Cbr320 => "mp3_cbr_320",
            Self::Flac24 => "flac_24",
            Self::AiffCdj => "aiff_cdj",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Mp3Cbr320 => "MP3 320 kbps CBR, true peak held under -1 dBTP",
            Self::Flac24 => "FLAC 24-bit lossless",
            Self::AiffCdj => "AIFF 16-bit 44.1 kHz for CDJs, dithered",
        }
    }

    pub const fn format(self) -> AudioFormat {
        match self {
            Self::Mp3Cbr320 => AudioFormat::Mp3,
            Self::Flac24 => AudioFormat::Flac,
            Self::AiffCdj => AudioFormat::Aiff,
        }
    }

    /// The loudest true peak the result may have, where the preset holds one.
    pub const fn true_peak_ceiling_dbtp(self) -> Option<f64> {
        match self {
            Self::Mp3Cbr320 => Some(-1.0),
            Self::Flac24 | Self::AiffCdj => None,
        }
    }
}

/// An MP3 encoder adds up to about half a decibel of overshoot, so the level going in aims
/// below the ceiling by that much.
pub const ENCODER_OVERSHOOT_DB: f64 = 0.5;

/// The gain in dB that keeps a file whose true peak is `true_peak_dbtp` under `ceiling_dbtp`
/// once encoded: none when it is already quiet enough, never a boost.
pub fn gain_for_ceiling(true_peak_dbtp: Option<f64>, ceiling_dbtp: f64) -> f64 {
    let target = ceiling_dbtp - ENCODER_OVERSHOOT_DB;
    match true_peak_dbtp {
        Some(peak) if peak > target => target - peak,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_round_trip() {
        for preset in ConvertPreset::ALL {
            assert_eq!(ConvertPreset::from_name(preset.name()), Some(preset));
        }
    }

    #[test]
    fn only_a_loud_file_is_turned_down() {
        assert_eq!(gain_for_ceiling(Some(-6.0), -1.0), 0.0);
        assert_eq!(gain_for_ceiling(None, -1.0), 0.0);
        assert!((gain_for_ceiling(Some(0.3), -1.0) + 1.8).abs() < 1e-12);
    }
}
