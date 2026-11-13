use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum EngineError {
    NoOutputDevice,
    Device(cpal::Error),
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    Thread(io::Error),
    /// The output thread ended before it said whether the device opened.
    OutputGone,
    /// More commands are waiting for the audio thread than it has taken, so it has stopped.
    AudioStalled,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoOutputDevice => f.write_str("This computer has no audio output device"),
            Self::Device(source) => write!(f, "The audio device refused the stream: {source}"),
            Self::Unreadable { path, reason } => {
                write!(f, "{} could not be played: {reason}", path.display())
            }
            Self::Thread(source) => write!(f, "An audio thread could not start: {source}"),
            Self::OutputGone => f.write_str("The audio output stopped while it was opening"),
            Self::AudioStalled => f.write_str("The audio thread is not taking commands"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<cpal::Error> for EngineError {
    fn from(source: cpal::Error) -> Self {
        Self::Device(source)
    }
}
