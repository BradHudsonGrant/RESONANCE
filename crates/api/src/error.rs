use std::fmt;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    BadRequest,
    LibraryUnreadable,
    ScanFailed,
    AudioFailed,
    CredentialsUnavailable,
    Internal,
}

/// The one place a failure becomes a message for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
}

impl ApiError {
    pub fn bad_request(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::BadRequest,
            message: format!("The app sent a request the core could not read: {detail}"),
        }
    }

    pub fn invalid_input(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::BadRequest,
            message: detail.to_string(),
        }
    }

    pub fn library_unreadable(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::LibraryUnreadable,
            message: format!("The library could not be read. {detail}"),
        }
    }

    pub fn library_unavailable(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::LibraryUnreadable,
            message: format!(
                "The library could not be opened. {detail}. Check that the data folder is writable, then restart Resonance."
            ),
        }
    }

    pub fn data_folder_unavailable() -> Self {
        Self {
            code: ErrorCode::LibraryUnreadable,
            message: "This machine has no home folder to keep the library in. Name one with --data-folder."
                .to_owned(),
        }
    }

    pub fn scan_failed(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::ScanFailed,
            message: format!(
                "The folder could not be scanned. {detail}. Check that the folder is readable and the library is writable, then try again."
            ),
        }
    }

    pub fn audio_failed(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::AudioFailed,
            message: format!(
                "{detail}. Check that an audio output is connected and that the file plays elsewhere, then try again."
            ),
        }
    }

    pub fn credentials_unavailable(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::CredentialsUnavailable,
            message: format!(
                "{detail}. Provider keys could not be saved or read, so steps that need them are skipped."
            ),
        }
    }

    pub fn internal(detail: impl fmt::Display) -> Self {
        Self {
            code: ErrorCode::Internal,
            message: format!(
                "The core could not finish this request. Restart Resonance, then try again. {detail}"
            ),
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}
