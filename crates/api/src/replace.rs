//! The compare sheet, Replace and its undo, and Convert and Repair through FFmpeg.

use std::fs;
use std::path::{Path, PathBuf};

use resonance_core::convert::ConvertPreset;
use resonance_core::replace::{ReplaceChoice, ReplaceVerdict};
use resonance_core::template::Template;
use resonance_library::ffmpeg::{self, FfmpegError, VerifiedFile};
use resonance_library::journal::{self, JournalError};
use resonance_library::replace::{
    self, FileSummary, ReplaceComparison as Comparison, ReplaceError,
};

use crate::Api;
use crate::command::{
    ConvertPresetInfo, ConvertPresetName, ConvertedFile, FfmpegStatus, ReplaceChoiceName,
    ReplaceComparison, ReplaceFileFacts, ReplaceResult, ReplaceVerdictName, UndoReport,
};
use crate::error::ApiError;
use crate::tag_writes::journal_operation;

/// Where Convert and Repair put the files they make, inside the app's data folder.
const CONVERTED_FOLDER: &str = "converted";

fn replace_error(error: ReplaceError) -> ApiError {
    match error {
        ReplaceError::Database(source) => ApiError::library_unreadable(source),
        ReplaceError::Journal(JournalError::Database(source)) => {
            ApiError::library_unreadable(source)
        }
        other => ApiError::invalid_input(other),
    }
}

fn ffmpeg_error(error: FfmpegError) -> ApiError {
    ApiError::invalid_input(error)
}

fn facts(summary: &FileSummary) -> ReplaceFileFacts {
    ReplaceFileFacts {
        path: summary.path.display().to_string(),
        format: summary.format.name().to_owned(),
        bitrate_kbps: summary.bitrate_kbps,
        sample_rate: summary.sample_rate,
        bit_depth: summary.bit_depth,
        duration_ms: summary.duration_ms,
        integrated_lufs: summary.measurements.integrated_lufs,
        true_peak_dbtp: summary.measurements.true_peak_dbtp,
        spectral_cutoff_hz: summary.measurements.spectral_cutoff_hz,
        flags: summary
            .flags
            .iter()
            .map(|flag| flag.reason.clone())
            .collect(),
    }
}

const fn choice_name(choice: ReplaceChoice) -> ReplaceChoiceName {
    match choice {
        ReplaceChoice::Replace => ReplaceChoiceName::Replace,
        ReplaceChoice::AddSeparateTrack => ReplaceChoiceName::AddSeparateTrack,
    }
}

const fn verdict_name(verdict: ReplaceVerdict) -> ReplaceVerdictName {
    match verdict {
        ReplaceVerdict::SameRecordingAndBetter => ReplaceVerdictName::SameRecordingAndBetter,
        ReplaceVerdict::SameRecordingNotBetter => ReplaceVerdictName::SameRecordingNotBetter,
        ReplaceVerdict::DifferentVersion => ReplaceVerdictName::DifferentVersion,
    }
}

const fn preset_of(name: ConvertPresetName) -> ConvertPreset {
    match name {
        ConvertPresetName::Mp3Cbr320 => ConvertPreset::Mp3Cbr320,
        ConvertPresetName::Flac24 => ConvertPreset::Flac24,
        ConvertPresetName::AiffCdj => ConvertPreset::AiffCdj,
    }
}

const fn preset_name(preset: ConvertPreset) -> ConvertPresetName {
    match preset {
        ConvertPreset::Mp3Cbr320 => ConvertPresetName::Mp3Cbr320,
        ConvertPreset::Flac24 => ConvertPresetName::Flac24,
        ConvertPreset::AiffCdj => ConvertPresetName::AiffCdj,
    }
}

fn view(comparison: &Comparison) -> ReplaceComparison {
    ReplaceComparison {
        track_id: comparison.current.track_id,
        old: facts(&comparison.old),
        new: facts(&comparison.new),
        relation: comparison.kind.label().to_owned(),
        fingerprint_similarity: comparison.fingerprint_similarity,
        offset_ms: comparison.offset_ms(),
        tempo_ratio: comparison.alignment.map(|alignment| alignment.old_per_new),
        verdict: verdict_name(comparison.verdict),
        verdict_label: comparison.verdict.label().to_owned(),
        default_choice: choice_name(comparison.verdict.default_choice()),
        replace_name: comparison.replace_name.clone(),
        separate_name: comparison.separate_name.clone(),
        cues_to_carry: u32::try_from(comparison.cues_to_carry).unwrap_or(u32::MAX),
    }
}

fn parse_template(template: Option<&str>) -> Result<Option<Template>, ApiError> {
    template
        .filter(|text| !text.trim().is_empty())
        .map(Template::parse)
        .transpose()
        .map_err(ApiError::invalid_input)
}

impl Api {
    /// Reads the track under the library's lock, then decodes and lines up both files
    /// without it.
    fn comparison(
        &self,
        track_id: i64,
        path: &str,
        template: Option<&str>,
    ) -> Result<Comparison, ApiError> {
        let template = parse_template(template)?;
        let current =
            replace::current_file(&self.locked_library(), track_id).map_err(replace_error)?;
        replace::compare(current, Path::new(path), template.as_ref()).map_err(replace_error)
    }

    pub(crate) fn compare_replacement(
        &self,
        track_id: i64,
        path: &str,
        template: Option<&str>,
    ) -> Result<ReplaceComparison, ApiError> {
        Ok(view(&self.comparison(track_id, path, template)?))
    }

    /// Measures again before applying, so Apply acts on the files as they are now.
    pub(crate) fn apply_replacement(
        &self,
        track_id: i64,
        path: &str,
        template: Option<&str>,
        choice: ReplaceChoiceName,
    ) -> Result<ReplaceResult, ApiError> {
        let comparison = self.comparison(track_id, path, template)?;
        let choice = match choice {
            ReplaceChoiceName::Replace => ReplaceChoice::Replace,
            ReplaceChoiceName::AddSeparateTrack => ReplaceChoice::AddSeparateTrack,
        };
        let report = replace::apply(&mut self.locked_library(), &comparison, choice)
            .map_err(replace_error)?;
        self.library_edited();
        Ok(ReplaceResult {
            operation: journal_operation(report.operation),
            track_id: report.track_id,
        })
    }

    pub(crate) fn undo_replacement(&self, operation_id: i64) -> Result<UndoReport, ApiError> {
        let operation = journal::undo(&mut self.locked_library(), operation_id).map_err(
            |error| match error {
                JournalError::Database(source) => ApiError::library_unreadable(source),
                other => ApiError::invalid_input(other),
            },
        )?;
        self.library_edited();
        Ok(UndoReport {
            operation: journal_operation(operation),
            restored: 1,
        })
    }

    pub(crate) fn ffmpeg_status(&self) -> FfmpegStatus {
        let found = ffmpeg::locate();
        FfmpegStatus {
            program: found
                .as_ref()
                .map(|ffmpeg| ffmpeg.program.display().to_string()),
            version: found.map(|ffmpeg| ffmpeg.version),
            presets: ConvertPreset::ALL
                .into_iter()
                .map(|preset| ConvertPresetInfo {
                    preset: preset_name(preset),
                    label: preset.label().to_owned(),
                })
                .collect(),
        }
    }

    /// The track's file, and an empty folder in the data folder for what FFmpeg makes. Files
    /// from an earlier Convert or Repair are cleared out first.
    fn ffmpeg_job(&self, track_id: i64) -> Result<(PathBuf, PathBuf), ApiError> {
        let library = self.locked_library();
        let current = replace::current_file(&library, track_id).map_err(replace_error)?;
        let out_dir = library
            .data_folder()
            .ok_or_else(ApiError::data_folder_unavailable)?
            .join(CONVERTED_FOLDER);
        drop(library);
        let _ = fs::remove_dir_all(&out_dir);
        Ok((current.full_path(), out_dir))
    }

    fn converted(made: VerifiedFile) -> ConvertedFile {
        ConvertedFile {
            path: made.path.display().to_string(),
            similarity: made.similarity,
        }
    }

    pub(crate) fn convert_track(
        &self,
        track_id: i64,
        preset: ConvertPresetName,
    ) -> Result<ConvertedFile, ApiError> {
        let ffmpeg = ffmpeg::locate().ok_or_else(|| ffmpeg_error(FfmpegError::NotInstalled))?;
        let (source, out_dir) = self.ffmpeg_job(track_id)?;
        ffmpeg
            .convert(&source, preset_of(preset), &out_dir)
            .map(Self::converted)
            .map_err(ffmpeg_error)
    }

    pub(crate) fn repair_track(&self, track_id: i64) -> Result<ConvertedFile, ApiError> {
        let ffmpeg = ffmpeg::locate().ok_or_else(|| ffmpeg_error(FfmpegError::NotInstalled))?;
        let (source, out_dir) = self.ffmpeg_job(track_id)?;
        ffmpeg
            .repair(&source, &out_dir)
            .map(Self::converted)
            .map_err(ffmpeg_error)
    }
}
