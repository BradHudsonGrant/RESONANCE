use std::fmt;
use std::path::{Path, PathBuf};

use resonance_library::scan::{ScanError, scan};

use crate::library_file::{self, OpenError};

#[derive(Debug)]
pub enum ScanCommandError {
    Open(OpenError),
    Scan(ScanError),
}

impl fmt::Display for ScanCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(source) => source.fmt(f),
            Self::Scan(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for ScanCommandError {}

pub fn run(folder: &Path, data_folder: Option<&Path>) -> Result<(), ScanCommandError> {
    let data_folder = library_file::data_folder(data_folder).map_err(ScanCommandError::Open)?;
    let mut database = library_file::open(&data_folder).map_err(ScanCommandError::Open)?;

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
