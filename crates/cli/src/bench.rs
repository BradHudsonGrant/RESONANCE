//! `resonance bench`: the audio run and the record. The page measurements come from
//! `ui/dev/bench.ts`, which `just bench` runs first; ADR 0011 gives the split.

mod audio;
mod machine;
mod page;
mod record;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use resonance_engine::EngineError;

use crate::bench::page::PageResults;
use crate::bench::record::Record;

const RECORDS: &str = "docs/benchmarks";

#[derive(Debug)]
pub enum BenchError {
    NotInRepository,
    PageUnreadable {
        path: PathBuf,
        source: io::Error,
    },
    PageMalformed {
        path: PathBuf,
        source: serde_json::Error,
    },
    Audio(EngineError),
    Write {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInRepository => write!(
                f,
                "Run resonance bench from the root of the repository, where {RECORDS} is"
            ),
            Self::PageUnreadable { path, source } => write!(
                f,
                "The page measurements in {} could not be read: {source}. `just bench` makes them",
                path.display()
            ),
            Self::PageMalformed { path, source } => write!(
                f,
                "The page measurements in {} are not in the shape expected: {source}",
                path.display()
            ),
            Self::Audio(source) => write!(f, "The audio run stopped: {source}"),
            Self::Write { path, source } => {
                write!(f, "{} could not be written: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for BenchError {}

pub fn run(page: &Path, audio_seconds: u32) -> Result<(), BenchError> {
    let records = Path::new(RECORDS);
    if !records.is_dir() {
        return Err(BenchError::NotInRepository);
    }
    let page = read_page(page)?;
    let machine = machine::machine();
    let versions = machine::versions();

    println!("Playing both decks through the audio device for {audio_seconds} s.");
    let audio = audio::play(&page.mix.tracks, audio_seconds).map_err(BenchError::Audio)?;

    let record = Record {
        machine: &machine,
        versions: &versions,
        page: &page,
        audio: &audio,
    };
    for outcome in record.outcomes() {
        println!(
            "{}: {}. {}",
            outcome.budget, outcome.verdict, outcome.measured
        );
    }
    let path = records.join(format!("budgets-v{}.md", versions.resonance));
    fs::write(&path, record.to_string()).map_err(|source| BenchError::Write {
        path: path.clone(),
        source,
    })?;
    println!("Wrote {}", path.display());
    Ok(())
}

fn read_page(path: &Path) -> Result<PageResults, BenchError> {
    let text = fs::read_to_string(path).map_err(|source| BenchError::PageUnreadable {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| BenchError::PageMalformed {
        path: path.to_owned(),
        source,
    })
}
