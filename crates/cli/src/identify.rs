use std::fmt;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use resonance_identify::credentials::OsCredentialStore;
use resonance_identify::http::NetworkClient;
use resonance_identify::library::run_online_pass;
use resonance_identify::pipeline::Pipeline;
use resonance_identify::provider::IdentifyError;
use resonance_library::identity::{IdentityError, identify_pending};

use crate::library_file::{self, OpenError};

#[derive(Debug)]
pub enum IdentifyCommandError {
    Open(OpenError),
    Identify(IdentityError),
    Online(IdentifyError),
}

impl fmt::Display for IdentifyCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(source) => source.fmt(f),
            Self::Identify(source) => source.fmt(f),
            Self::Online(source) => source.fmt(f),
        }
    }
}

impl std::error::Error for IdentifyCommandError {}

pub fn run(data_folder: Option<&Path>, online: bool) -> Result<(), IdentifyCommandError> {
    let data_folder = library_file::data_folder(data_folder).map_err(IdentifyCommandError::Open)?;
    let mut database = library_file::open(&data_folder).map_err(IdentifyCommandError::Open)?;
    let report = identify_pending(&mut database).map_err(IdentifyCommandError::Identify)?;

    println!(
        "Identified {} files: {} decoded, {} moved tracks kept, {} unreadable",
        report.processed, report.decoded, report.reconciled_moves, report.skipped_unreadable
    );
    if !online {
        return Ok(());
    }

    let database = Arc::new(Mutex::new(database));
    let pipeline = Pipeline::with_free_providers(
        Arc::new(NetworkClient::new()),
        Arc::clone(&database),
        Arc::new(OsCredentialStore::new()),
    );
    let report = run_online_pass(
        &database,
        &pipeline,
        &[],
        &AtomicBool::new(false),
        |done, total| eprint!("\rLooked up {done} of {total}"),
    )
    .map_err(IdentifyCommandError::Online)?;
    eprintln!();
    println!(
        "Looked up {} tracks online: {} confident, {} need a release link, {} to try again. \
         {} requests, {} answers from the cache",
        report.looked_up,
        report.confident,
        report.needs_url,
        report.incomplete,
        report.network_requests,
        report.cached_answers
    );
    Ok(())
}
