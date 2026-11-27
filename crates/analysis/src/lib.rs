pub mod frontend;
pub mod registry;
pub mod runtime;

use std::fmt;
use std::io;
use std::path::PathBuf;

use resonance_core::analysis::AnalysisKey;

/// What an analyser needs to run, so the job queue can schedule it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyserTier {
    /// Plain DSP on the CPU, quick enough to run on every track as it arrives.
    Cpu,
    /// A model on the GPU, or the bar features built on its output. One runs at a time.
    Gpu,
    /// Stem separation, the heaviest work, run only when asked for.
    Stems,
}

/// One kind of analysis. Its name, version and settings hash make the key its results are
/// cached under, next to the audio hash of the recording.
pub trait Analyser: Send + Sync {
    fn name(&self) -> &'static str;
    /// Changes whenever the algorithm or the model it runs changes.
    fn version(&self) -> &'static str;
    /// BLAKE3 of the settings the analyser runs with.
    fn param_hash(&self) -> [u8; 32];
    fn tier(&self) -> AnalyserTier;

    fn key(&self) -> AnalysisKey {
        AnalysisKey {
            analyser: self.name().to_owned(),
            version: self.version().to_owned(),
            param_hash: self.param_hash(),
        }
    }
}

#[derive(Debug)]
pub enum AnalysisError {
    UnknownModel {
        id: String,
    },
    /// The model is free for non-commercial use only and the user has not allowed such models.
    NonCommercial {
        id: String,
        licence: &'static str,
    },
    Download {
        url: &'static str,
        source: Box<ureq::Error>,
    },
    WrongSize {
        id: String,
        expected: u64,
        found: u64,
    },
    WrongChecksum {
        id: String,
        expected: &'static str,
        found: String,
    },
    File {
        path: PathBuf,
        source: io::Error,
    },
    Runtime(ort::Error),
    /// A model's output was not the shape its spec says.
    UnexpectedOutput {
        output: String,
    },
    /// The front end could not resample from the file's rate.
    ResamplerSetup(rubato::ResamplerConstructionError),
    Resample(rubato::ResampleError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownModel { id } => write!(f, "No model is registered as {id}."),
            Self::NonCommercial { id, licence } => write!(
                f,
                "{id} is licensed {licence}, for non-commercial use only. Turn on non-commercial \
                 models in Settings to fetch it."
            ),
            Self::Download { url, source } => write!(f, "Could not fetch {url}: {source}"),
            Self::WrongSize {
                id,
                expected,
                found,
            } => write!(
                f,
                "{id} should be {expected} bytes but {found} arrived, so it was not kept."
            ),
            Self::WrongChecksum {
                id,
                expected,
                found,
            } => write!(
                f,
                "{id} has SHA-256 {found} where {expected} was expected, so it was not kept."
            ),
            Self::File { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Runtime(source) => write!(f, "ONNX Runtime: {source}"),
            Self::UnexpectedOutput { output } => {
                write!(f, "The model's output {output} is not a float tensor.")
            }
            Self::ResamplerSetup(source) => write!(f, "Could not set up resampling: {source}"),
            Self::Resample(source) => write!(f, "Resampling failed: {source}"),
        }
    }
}

impl std::error::Error for AnalysisError {}

impl From<ort::Error> for AnalysisError {
    fn from(source: ort::Error) -> Self {
        Self::Runtime(source)
    }
}
