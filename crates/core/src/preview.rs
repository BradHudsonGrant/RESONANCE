//! Where a library preview starts, and what moving through the table plays.

/// The furthest into a track a fixed start can be set.
pub const MAX_START_SECONDS: u32 = 600;

/// Where a preview started by auto-preview begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewStart {
    /// The first frame of the track.
    Beginning,
    /// 30 % of the way in, past most intros and before most breakdowns.
    #[default]
    ThirtyPercent,
    /// A fixed time in, for a DJ who knows how long their intros run.
    Seconds(u32),
}

impl PreviewStart {
    /// The start time for a track of `duration_ms`, where 0 is an unknown length.
    ///
    /// A fixed start past the end of a short track plays it from the beginning instead of
    /// starting on silence.
    pub fn position_ms(self, duration_ms: u64) -> u64 {
        match self {
            Self::Beginning => 0,
            Self::ThirtyPercent => duration_ms * 3 / 10,
            Self::Seconds(seconds) => {
                let at = u64::from(seconds.min(MAX_START_SECONDS)) * 1000;
                if duration_ms > 0 && at >= duration_ms {
                    0
                } else {
                    at
                }
            }
        }
    }

    /// The same start with a fixed time held inside what the setting allows.
    pub fn clamped(self) -> Self {
        match self {
            Self::Seconds(seconds) => Self::Seconds(seconds.min(MAX_START_SECONDS)),
            other => other,
        }
    }
}

/// How library previews behave. Auto-preview is off until the user turns it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PreviewSettings {
    pub auto_preview: bool,
    pub start: PreviewStart,
}

/// The row the user has just moved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusedTrack {
    pub track_id: i64,
    pub missing: bool,
    /// The length the library holds for it, 0 when unknown.
    pub duration_ms: u64,
}

/// The track on the preview channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadedPreview {
    pub track_id: i64,
    pub playing: bool,
}

/// What a move to a row does to the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPreview {
    /// Leave the preview as it is.
    Stay,
    /// Play the loaded track on from where it was paused.
    Resume,
    /// Swap the preview to the focused track and play it from `start_ms`.
    Play { start_ms: u64 },
}

/// Moving onto a row makes it heard when auto-preview is on. The loaded track carries on, or
/// resumes if paused, rather than starting again. A missing file keeps the last track playing,
/// so arrowing past one does not cut the sound.
pub fn on_focus(
    settings: &PreviewSettings,
    focused: &FocusedTrack,
    loaded: Option<LoadedPreview>,
) -> FocusPreview {
    if !settings.auto_preview || focused.missing {
        return FocusPreview::Stay;
    }
    match loaded {
        Some(loaded) if loaded.track_id == focused.track_id && loaded.playing => FocusPreview::Stay,
        Some(loaded) if loaded.track_id == focused.track_id => FocusPreview::Resume,
        _ => FocusPreview::Play {
            start_ms: settings.start.position_ms(focused.duration_ms),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: PreviewSettings = PreviewSettings {
        auto_preview: true,
        start: PreviewStart::ThirtyPercent,
    };

    fn track(track_id: i64) -> FocusedTrack {
        FocusedTrack {
            track_id,
            missing: false,
            duration_ms: 300_000,
        }
    }

    #[test]
    fn auto_preview_is_off_and_starts_30_percent_in_by_default() {
        let settings = PreviewSettings::default();
        assert!(!settings.auto_preview);
        assert_eq!(settings.start, PreviewStart::ThirtyPercent);
    }

    #[test]
    fn each_start_puts_the_preview_where_it_says() {
        assert_eq!(PreviewStart::Beginning.position_ms(300_000), 0);
        assert_eq!(PreviewStart::ThirtyPercent.position_ms(300_000), 90_000);
        assert_eq!(PreviewStart::ThirtyPercent.position_ms(0), 0);
        assert_eq!(PreviewStart::Seconds(45).position_ms(300_000), 45_000);
        assert_eq!(PreviewStart::Seconds(45).position_ms(0), 45_000);
    }

    #[test]
    fn a_fixed_start_past_the_end_plays_from_the_beginning() {
        assert_eq!(PreviewStart::Seconds(60).position_ms(60_000), 0);
        assert_eq!(PreviewStart::Seconds(60).position_ms(30_000), 0);
    }

    #[test]
    fn a_fixed_start_is_held_to_ten_minutes() {
        assert_eq!(
            PreviewStart::Seconds(5_000).clamped(),
            PreviewStart::Seconds(MAX_START_SECONDS)
        );
        assert_eq!(PreviewStart::Seconds(5_000).position_ms(0), 600_000);
    }

    fn loaded(track_id: i64, playing: bool) -> Option<LoadedPreview> {
        Some(LoadedPreview { track_id, playing })
    }

    #[test]
    fn moving_to_a_row_plays_it_only_with_auto_preview_on() {
        let off = PreviewSettings::default();
        assert_eq!(
            on_focus(&off, &track(2), loaded(1, true)),
            FocusPreview::Stay
        );
        assert_eq!(
            on_focus(&ON, &track(2), loaded(1, true)),
            FocusPreview::Play { start_ms: 90_000 }
        );
        assert_eq!(
            on_focus(&ON, &track(2), None),
            FocusPreview::Play { start_ms: 90_000 }
        );
    }

    #[test]
    fn the_loaded_track_carries_on_or_resumes_instead_of_starting_again() {
        assert_eq!(
            on_focus(&ON, &track(1), loaded(1, true)),
            FocusPreview::Stay
        );
        assert_eq!(
            on_focus(&ON, &track(1), loaded(1, false)),
            FocusPreview::Resume
        );
    }

    #[test]
    fn a_missing_file_keeps_the_last_track_playing() {
        let missing = FocusedTrack {
            missing: true,
            ..track(2)
        };
        assert_eq!(on_focus(&ON, &missing, loaded(1, true)), FocusPreview::Stay);
    }
}
