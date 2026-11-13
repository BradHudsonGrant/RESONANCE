use std::fmt;
use std::path::{Path, PathBuf};

use resonance_api::data_folder::default_data_folder;
use resonance_db::{Database, DbError, FILE_NAME};

#[derive(Debug)]
pub enum OpenError {
    NoDataFolder,
    Database(DbError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDataFolder => write!(
                f,
                "This machine has no home folder to keep the library in. Name one with --data-folder."
            ),
            Self::Database(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for OpenError {}

/// The data folder given on the command line, or the app's usual one.
pub fn data_folder(given: Option<&Path>) -> Result<PathBuf, OpenError> {
    match given {
        Some(folder) => Ok(folder.to_owned()),
        None => default_data_folder().ok_or(OpenError::NoDataFolder),
    }
}

/// Opens the library in the data folder, making it if this is the first time.
pub fn open(data_folder: &Path) -> Result<Database, OpenError> {
    Database::open(&data_folder.join(FILE_NAME)).map_err(OpenError::Database)
}
