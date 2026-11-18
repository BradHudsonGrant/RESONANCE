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

use resonance_engine::{EngineError, OutputMode};

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
    // The budget is set for exclusive use of the device; the record says which mode it got.
    let audio = audio::play(&page.mix.tracks, audio_seconds, OutputMode::Exclusive)
        .map_err(BenchError::Audio)?;

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

/// Plays two tracks for `seconds` and prints what the audio callback counted, without touching
/// the records. This is how a long stability run is taken.
pub fn play_audio(tracks: &[PathBuf; 2], seconds: u32, mode: OutputMode) -> Result<(), BenchError> {
    println!("Playing both decks through the audio device for {seconds} s.");
    let run = audio::play(tracks, seconds, mode).map_err(BenchError::Audio)?;
    let device = &run.device;
    println!(
        "{}: {:?} mode, {} Hz, {}-frame period ({} asked for), {}-frame buffer",
        device.name,
        device.mode,
        device.sample_rate,
        device.period_frames,
        device.requested_buffer_frames,
        run.buffer_frames
    );
    println!(
        "{:.0} s, {} callbacks of {} to {} frames, longest {:.2} ms",
        run.seconds,
        run.callbacks,
        run.fewest_callback_frames,
        run.most_callback_frames,
        run.longest_callback_ms
    );
    println!(
        "Underruns {} ({} frames), late callbacks {}, starved callbacks {}, stream errors {}",
        run.underruns,
        run.missing_frames,
        run.late_callbacks,
        run.starved_callbacks,
        run.stream_errors
    );
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
