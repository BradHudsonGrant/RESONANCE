use std::fmt;
use std::path::Path;

use resonance_library::identity::{IdentityError, identify_pending};

use crate::library_file::{self, OpenError};

#[derive(Debug)]
pub enum IdentifyCommandError {
    Open(OpenError),
    Identify(IdentityError),
}

impl fmt::Display for IdentifyCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(source) => source.fmt(f),
            Self::Identify(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for IdentifyCommandError {}

pub fn run(data_folder: Option<&Path>) -> Result<(), IdentifyCommandError> {
    let data_folder = library_file::data_folder(data_folder).map_err(IdentifyCommandError::Open)?;
    let mut database = library_file::open(&data_folder).map_err(IdentifyCommandError::Open)?;
    let report = identify_pending(&mut database).map_err(IdentifyCommandError::Identify)?;

    println!(
        "Identified {} files: {} decoded, {} moved tracks kept, {} unreadable",
        report.processed, report.decoded, report.reconciled_moves, report.skipped_unreadable
    );
    Ok(())
}
