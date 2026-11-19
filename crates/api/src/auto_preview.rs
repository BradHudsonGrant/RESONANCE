use std::sync::PoisonError;

use resonance_core::preview::{
    FocusPreview, FocusedTrack, LoadedPreview, MAX_START_SECONDS, PreviewSettings as CoreSettings,
    PreviewStart, on_focus,
};

use crate::Api;
use crate::command::{PreviewSettings, PreviewStartPoint, PreviewState};
use crate::error::ApiError;

impl Api {
    pub(crate) fn preview_focus(&self, track_id: i64) -> Result<PreviewState, ApiError> {
        let (settings, row) = {
            let library = self
                .core
                .library
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let settings = library
                .preview_settings()
                .map_err(ApiError::library_unreadable)?;
            let row = library
                .track_by_id(track_id)
                .map_err(ApiError::library_unreadable)?;
            (settings, row)
        };
        let Some(row) = row else {
            return Err(ApiError::bad_request(format!(
                "Track {track_id} is no longer in the library."
            )));
        };
        let status = self.mix.preview_status();
        let loaded = self
            .preview_meta
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|meta| status.path.as_deref() == Some(meta.path.as_path()))
            .map(|meta| LoadedPreview {
                track_id: meta.track_id,
                playing: status.playing,
            });
        let focused = FocusedTrack {
            track_id,
            missing: row.missing_since.is_some(),
            duration_ms: row.file.audio.map_or(0, |audio| audio.duration_ms),
        };
        match on_focus(&settings, &focused, loaded) {
            FocusPreview::Stay => Ok(self.preview_state(status)),
            FocusPreview::Resume => self
                .mix
                .preview_toggle_play()
                .map(|status| self.preview_state(status))
                .map_err(ApiError::audio_failed),
            FocusPreview::Play { start_ms } => self.preview_play_row(track_id, &row, start_ms),
        }
    }

    pub(crate) fn preview_settings(&self) -> Result<PreviewSettings, ApiError> {
        let library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        library
            .preview_settings()
            .map(described)
            .map_err(ApiError::library_unreadable)
    }

    pub(crate) fn set_preview_settings(
        &self,
        settings: PreviewSettings,
    ) -> Result<PreviewSettings, ApiError> {
        let settings = CoreSettings {
            auto_preview: settings.auto_preview,
            start: match settings.start {
                PreviewStartPoint::Beginning => PreviewStart::Beginning,
                PreviewStartPoint::ThirtyPercent => PreviewStart::ThirtyPercent,
                PreviewStartPoint::Seconds { seconds } => PreviewStart::Seconds(seconds),
            }
            .clamped(),
        };
        let mut library = self
            .core
            .library
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        library
            .save_preview_settings(&settings)
            .map_err(ApiError::library_unreadable)?;
        Ok(described(settings))
    }
}

fn described(settings: CoreSettings) -> PreviewSettings {
    PreviewSettings {
        auto_preview: settings.auto_preview,
        start: match settings.start {
            PreviewStart::Beginning => PreviewStartPoint::Beginning,
            PreviewStart::ThirtyPercent => PreviewStartPoint::ThirtyPercent,
            PreviewStart::Seconds(seconds) => PreviewStartPoint::Seconds { seconds },
        },
        max_start_seconds: MAX_START_SECONDS,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use resonance_db::Database;
    use resonance_fixtures::signal::Signal;

    use crate::Api;
    use crate::command::{PreviewSettings, PreviewStartPoint, PreviewState, Request, Response};

    fn library_of_two(name: &str) -> (Api, i64, i64) {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-tmp")
            .join(name);
        std::fs::create_dir_all(&folder).unwrap();
        Signal::tone(440.0, 2.0, -6.0, 44_100)
            .write_wav(&folder.join("a.wav"))
            .unwrap();
        Signal::tone(660.0, 4.0, -6.0, 44_100)
            .write_wav(&folder.join("b.wav"))
            .unwrap();
        let api = Api::new_silent(Database::open_in_memory().unwrap());
        api.handle(Request::Scan {
            folder: folder.to_string_lossy().into_owned(),
        })
        .unwrap();
        let Response::Tracks(page) = api
            .handle(Request::Tracks {
                offset: 0,
                limit: 10,
                query: None,
            })
            .unwrap()
        else {
            panic!("expected Tracks");
        };
        let by_name = |file: &str| {
            page.tracks
                .iter()
                .find(|track| track.location.ends_with(file))
                .map(|track| track.id)
                .unwrap()
        };
        (api, by_name("a.wav"), by_name("b.wav"))
    }

    fn focus(api: &Api, track_id: i64) -> PreviewState {
        let Response::PreviewFocus(state) = api.handle(Request::PreviewFocus { track_id }).unwrap()
        else {
            panic!("expected PreviewFocus");
        };
        state
    }

    fn set(api: &Api, auto_preview: bool, start: PreviewStartPoint) -> PreviewSettings {
        let Response::SetPreviewSettings(saved) = api
            .handle(Request::SetPreviewSettings {
                settings: PreviewSettings {
                    auto_preview,
                    start,
                    max_start_seconds: 0,
                },
            })
            .unwrap()
        else {
            panic!("expected SetPreviewSettings");
        };
        saved
    }

    #[test]
    fn focus_does_nothing_until_auto_preview_is_on() {
        let (api, first, _) = library_of_two("api-auto-preview-off");
        let Response::PreviewSettings(settings) = api.handle(Request::PreviewSettings).unwrap()
        else {
            panic!("expected PreviewSettings");
        };
        assert!(!settings.auto_preview);
        assert_eq!(settings.start, PreviewStartPoint::ThirtyPercent);
        assert_eq!(settings.max_start_seconds, 600);

        let state = focus(&api, first);
        assert_eq!(state.track_id, None);
        assert!(!state.playing);
    }

    #[test]
    fn focus_swaps_to_each_row_from_the_chosen_start() {
        let (api, first, second) = library_of_two("api-auto-preview-swap");
        set(&api, true, PreviewStartPoint::ThirtyPercent);

        let playing = focus(&api, first);
        assert_eq!(playing.track_id, Some(first));
        assert!(playing.playing);
        assert!(playing.position_ms >= 600, "{}", playing.position_ms);

        let swapped = focus(&api, second);
        assert_eq!(swapped.track_id, Some(second));
        assert!(swapped.generation > playing.generation);
        assert!(swapped.position_ms >= 1_200, "{}", swapped.position_ms);

        let again = focus(&api, second);
        assert_eq!(again.generation, swapped.generation);

        api.handle(Request::PreviewPause).unwrap();
        let resumed = focus(&api, second);
        assert_eq!(resumed.track_id, Some(second));
        assert!(resumed.playing);

        set(&api, true, PreviewStartPoint::Seconds { seconds: 1 });
        let fixed = focus(&api, first);
        assert_eq!(fixed.track_id, Some(first));
        assert!(fixed.position_ms >= 1_000, "{}", fixed.position_ms);
    }

    #[test]
    fn a_fixed_start_is_held_to_the_limit() {
        let (api, _, _) = library_of_two("api-auto-preview-limit");
        let saved = set(&api, true, PreviewStartPoint::Seconds { seconds: 90_000 });
        assert_eq!(saved.start, PreviewStartPoint::Seconds { seconds: 600 });
    }
}
