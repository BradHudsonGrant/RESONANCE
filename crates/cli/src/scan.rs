use std::fmt;
use std::path::{Path, PathBuf};

use resonance_api::data_folder::default_data_folder;
use resonance_db::{Database, DbError};
use resonance_library::scan::{ScanError, scan};

pub const DATABASE_NAME: &str = "library.db";

#[derive(Debug)]
pub enum ScanCommandError {
    NoDataFolder,
    Database(DbError),
    Scan(ScanError),
}

impl fmt::Display for ScanCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDataFolder => write!(
                f,
                "This machine has no home folder to keep the library in. Name one with --data-folder."
            ),
            Self::Database(source) => source.fmt(f),
            Self::Scan(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for ScanCommandError {}

pub fn run(folder: &Path, data_folder: Option<&Path>) -> Result<(), ScanCommandError> {
    let data_folder = match data_folder {
        Some(folder) => folder.to_owned(),
        None => default_data_folder().ok_or(ScanCommandError::NoDataFolder)?,
    };
    let mut database =
        Database::open(&data_folder.join(DATABASE_NAME)).map_err(ScanCommandError::Database)?;

    let mut own_files: Vec<PathBuf> = vec![data_folder];
    own_files.extend(std::env::current_exe().ok());
    let report = scan(&mut database, folder, &own_files).map_err(ScanCommandError::Scan)?;

    println!("Scanned {}", report.source_folder);
    println!(
        "{} files: {} added, {} changed, {} unchanged",
        report.found, report.added, report.changed, report.unchanged
    );
    if !report.unreadable.is_empty() {
        eprintln!("{} could not be read:", report.unreadable.len());
        for unreadable in &report.unreadable {
            eprintln!("  {}: {}", unreadable.path, unreadable.reason);
        }
    }
    Ok(())
}
