//! Convert and Repair through an FFmpeg found on the machine. FFmpeg runs as its own process and
//! is never linked ([ADR 0005](../../../docs/adr/0005-licence-policy.md)). Every file it writes
//! is decoded and fingerprinted against its source before anything else may use it, and the
//! source is never touched.

use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use resonance_core::convert::{ConvertPreset, gain_for_ceiling};
use resonance_core::duplicates::{
    SAME_RECORDING_MAX_DURATION_DELTA_MS, SAME_RECORDING_MIN_SIMILARITY,
};
use resonance_core::identity::{
    AssetIdentity, RELINK_MAX_FINGERPRINT_SHIFT, fingerprint_similarity,
};
use resonance_core::library::AudioFormat;

use crate::file_tags::audio_start;
use crate::identity::compute_identity;

/// An FFmpeg program that answered `-version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ffmpeg {
    pub program: PathBuf,
    /// The first line FFmpeg prints about itself, such as `ffmpeg version 8.1`.
    pub version: String,
}

#[derive(Debug)]
pub enum FfmpegError {
    NotInstalled,
    NotAudio(PathBuf),
    File {
        path: PathBuf,
        source: io::Error,
    },
    /// FFmpeg ran and failed. `detail` is the end of what it printed.
    Failed {
        detail: String,
    },
    /// The result did not decode cleanly or did not sound like its source, so it was deleted.
    NotVerified {
        reason: String,
    },
}

impl fmt::Display for FfmpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => f.write_str(
                "FFmpeg was not found. Install it and make sure the ffmpeg command works in a terminal, then try again.",
            ),
            Self::NotAudio(path) => write!(f, "{} is not an audio file.", path.display()),
            Self::File { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Failed { detail } => write!(f, "FFmpeg stopped with an error: {detail}"),
            Self::NotVerified { reason } => {
                write!(f, "The new file was not kept because {reason}.")
            }
        }
    }
}

impl std::error::Error for FfmpegError {}

fn file_error(path: &Path) -> impl FnOnce(io::Error) -> FfmpegError + '_ {
    move |source| FfmpegError::File {
        path: path.to_owned(),
        source,
    }
}

const PROGRAM: &str = if cfg!(windows) {
    "ffmpeg.exe"
} else {
    "ffmpeg"
};

/// Folders package managers put FFmpeg in. An app started from the desktop on macOS does not
/// see the shell's PATH, so Homebrew's folders are looked in as well.
fn well_known_folders() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        folders
            .extend(["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"].map(PathBuf::from));
    } else if cfg!(windows) {
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            folders.push(
                PathBuf::from(local)
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links"),
            );
        }
        if let Some(home) = env::var_os("USERPROFILE") {
            folders.push(PathBuf::from(home).join("scoop").join("shims"));
        }
        folders.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin"));
    } else {
        folders.extend(["/usr/bin", "/usr/local/bin", "/snap/bin"].map(PathBuf::from));
    }
    folders
}

fn version_of(program: &Path) -> Option<String> {
    let output = Command::new(program)
        .arg("-version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().next()?.trim();
    first
        .starts_with("ffmpeg version")
        .then(|| first.split(" Copyright").next().unwrap_or(first).to_owned())
}

/// Looks for FFmpeg on the PATH, then where package managers put it.
pub fn locate() -> Option<Ffmpeg> {
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .chain(well_known_folders())
        .map(|folder| folder.join(PROGRAM))
        .filter(|program| program.is_file())
        .find_map(|program| {
            version_of(&program).map(|version| Ffmpeg {
                program: dunce::simplified(&program).to_owned(),
                version,
            })
        })
}

/// A file FFmpeg wrote and the checks passed.
#[derive(Debug, Clone)]
pub struct VerifiedFile {
    pub path: PathBuf,
    pub identity: AssetIdentity,
    /// How closely its fingerprint matches the source's.
    pub similarity: f32,
}

fn extension_of(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::Aiff => "aiff",
        other => other.name(),
    }
}

impl Ffmpeg {
    fn run(&self, args: &[OsString]) -> Result<(), FfmpegError> {
        let output = Command::new(&self.program)
            .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-y"])
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(file_error(&self.program))?;
        if output.status.success() {
            return Ok(());
        }
        let printed = String::from_utf8_lossy(&output.stderr);
        let detail: Vec<&str> = printed.lines().rev().take(3).collect();
        Err(FfmpegError::Failed {
            detail: detail.into_iter().rev().collect::<Vec<_>>().join(" "),
        })
    }

    /// Makes a copy of `source` in `out_dir` from `preset`, checked against the source.
    pub fn convert(
        &self,
        source: &Path,
        preset: ConvertPreset,
        out_dir: &Path,
    ) -> Result<VerifiedFile, FfmpegError> {
        let source_identity = compute_identity(source).map_err(file_error(source))?;
        let output = output_path(source, out_dir, preset.format())?;
        let mut gain_db = preset
            .true_peak_ceiling_dbtp()
            .map(|ceiling| {
                let peak = source_identity
                    .quality
                    .as_ref()
                    .and_then(|quality| quality.measurements.true_peak_dbtp);
                gain_for_ceiling(peak, ceiling)
            })
            .unwrap_or(0.0);
        // A second pass, turned down by the overshoot the first one measured, when the encoder
        // overshot more than allowed for.
        for _ in 0..2 {
            self.run(&convert_args(source, &output, preset, gain_db))?;
            let verified = verify(&output, &source_identity)?;
            let Some(ceiling) = preset.true_peak_ceiling_dbtp() else {
                return Ok(verified);
            };
            let peak = verified
                .identity
                .quality
                .as_ref()
                .and_then(|quality| quality.measurements.true_peak_dbtp);
            match peak {
                Some(peak) if peak >= ceiling => gain_db -= peak - ceiling + 0.2,
                _ => return Ok(verified),
            }
        }
        let _ = fs::remove_file(&output);
        Err(FfmpegError::NotVerified {
            reason: "its true peak stayed above the preset's ceiling".to_owned(),
        })
    }

    /// Remuxes `source` into a fresh container of the same format in `out_dir`, without
    /// re-encoding. An ID3v2 block in front of an MP4 is left behind, because MP4 players do
    /// not expect one there.
    pub fn repair(&self, source: &Path, out_dir: &Path) -> Result<VerifiedFile, FfmpegError> {
        let format =
            AudioFormat::of(source).ok_or_else(|| FfmpegError::NotAudio(source.to_owned()))?;
        let source_identity = compute_identity(source).map_err(file_error(source))?;
        let output = output_path(source, out_dir, format)?;
        let skip = audio_start(source).map_err(file_error(source))?;
        let stripped = if skip > 0 {
            let mut name = output.file_name().unwrap_or_default().to_os_string();
            name.push(".input");
            let stripped = output.with_file_name(name);
            copy_from(source, skip, &stripped)?;
            Some(stripped)
        } else {
            None
        };
        let input = stripped.as_deref().unwrap_or(source);
        let mut args: Vec<OsString> = vec!["-i".into(), input.into()];
        args.extend(
            [
                "-map",
                "0:a:0",
                "-map",
                "0:v?",
                "-map_metadata",
                "0",
                "-c",
                "copy",
            ]
            .map(OsString::from),
        );
        if format == AudioFormat::M4a {
            args.extend(["-movflags", "+faststart"].map(OsString::from));
        }
        args.push(output.clone().into());
        let ran = self.run(&args);
        if let Some(stripped) = &stripped {
            let _ = fs::remove_file(stripped);
        }
        ran?;
        verify(&output, &source_identity)
    }
}

fn output_path(source: &Path, out_dir: &Path, format: AudioFormat) -> Result<PathBuf, FfmpegError> {
    fs::create_dir_all(out_dir).map_err(file_error(out_dir))?;
    let stem = source
        .file_stem()
        .ok_or_else(|| FfmpegError::NotAudio(source.to_owned()))?;
    let mut name = stem.to_os_string();
    name.push(".");
    name.push(extension_of(format));
    Ok(out_dir.join(name))
}

fn copy_from(source: &Path, skip: u64, to: &Path) -> Result<(), FfmpegError> {
    let mut input = File::open(source).map_err(file_error(source))?;
    input
        .seek(SeekFrom::Start(skip))
        .map_err(file_error(source))?;
    let mut output = File::create(to).map_err(file_error(to))?;
    io::copy(&mut input.by_ref(), &mut output).map_err(file_error(to))?;
    Ok(())
}

fn convert_args(
    source: &Path,
    output: &Path,
    preset: ConvertPreset,
    gain_db: f64,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-i".into(), source.into()];
    args.extend(["-map", "0:a:0", "-map_metadata", "0"].map(OsString::from));
    let mut filters: Vec<String> = Vec::new();
    if gain_db < 0.0 {
        filters.push(format!("volume={gain_db:.2}dB"));
    }
    match preset {
        ConvertPreset::Mp3Cbr320 => {
            args.extend(
                ["-c:a", "libmp3lame", "-b:a", "320k", "-id3v2_version", "3"].map(OsString::from),
            );
        }
        ConvertPreset::Flac24 => {
            args.extend(
                [
                    "-c:a",
                    "flac",
                    "-sample_fmt",
                    "s32",
                    "-bits_per_raw_sample",
                    "24",
                ]
                .map(OsString::from),
            );
        }
        ConvertPreset::AiffCdj => {
            filters.push("aresample=44100:osf=s16:dither_method=triangular".to_owned());
            args.extend(["-c:a", "pcm_s16be", "-write_id3v2", "1"].map(OsString::from));
        }
    }
    if !filters.is_empty() {
        args.push("-af".into());
        args.push(filters.join(",").into());
    }
    args.push(output.into());
    args
}

/// Decodes `output` and compares it with its source. A file that fails is deleted.
fn verify(output: &Path, source: &AssetIdentity) -> Result<VerifiedFile, FfmpegError> {
    let refuse = |reason: &str| {
        let _ = fs::remove_file(output);
        Err(FfmpegError::NotVerified {
            reason: reason.to_owned(),
        })
    };
    let identity = match compute_identity(output) {
        Ok(identity) => identity,
        Err(_) => return refuse("it could not be read back"),
    };
    let Some(facts) = identity.decoded_facts else {
        return refuse("it did not decode");
    };
    if facts.decode_errors > 0 {
        return refuse("it did not decode cleanly");
    }
    if let Some(source_facts) = source.decoded_facts
        && facts.duration_ms().abs_diff(source_facts.duration_ms())
            > SAME_RECORDING_MAX_DURATION_DELTA_MS
    {
        return refuse("its length differs from the source");
    }
    let similarity = match (&identity.chromaprint, &source.chromaprint) {
        (Some(made), Some(original)) => {
            fingerprint_similarity(&made.raw, &original.raw, RELINK_MAX_FINGERPRINT_SHIFT)
        }
        _ => return refuse("its fingerprint could not be compared with the source"),
    };
    if similarity < SAME_RECORDING_MIN_SIMILARITY {
        return refuse("it does not sound like the source");
    }
    Ok(VerifiedFile {
        path: output.to_owned(),
        identity,
        similarity,
    })
}
