mod bench;
mod identify;
mod library_file;
mod scan;
mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use resonance_core::key::Key;
use resonance_engine::OutputMode;

#[derive(Parser)]
#[command(name = "resonance", version, about = "Resonance from the command line")]
struct Cli {
    /// Keep the library database in this folder instead of the usual place for app data
    #[arg(long, global = true, value_name = "FOLDER")]
    data_folder: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show how a key tag is read, as its Camelot code and musical name
    Key {
        /// Key tags as they appear in files, such as 8A, F#m or "Db maj"
        #[arg(required = true)]
        tags: Vec<String>,
    },
    /// Read the audio files under a folder into the library. Nothing in the folder is changed
    Scan {
        /// The music folder to read
        folder: PathBuf,
    },
    /// Hash and fingerprint any files in the library whose identity is still pending
    Identify {
        /// Then look every track without an online result up on AcoustID, MusicBrainz and
        /// Discogs, using the keys saved in Settings
        #[arg(long)]
        online: bool,
    },
    /// Play both decks through the audio device and write every performance budget to
    /// docs/benchmarks. `just bench` takes the page measurements first and then runs this
    Bench {
        /// The page measurements that ui/dev/bench.ts wrote
        #[arg(long, value_name = "PATH")]
        page: PathBuf,
        /// How long to play both decks
        #[arg(long, value_name = "SECONDS", default_value_t = 180)]
        audio_seconds: u32,
    },
    /// Play two tracks on both decks through the audio device and print what the audio thread
    /// counted: period, underruns, late and starved callbacks. A deck whose track ends starts it
    /// again, so a run can last as long as needed
    Audio {
        /// The tracks for deck A and deck B
        #[arg(num_args = 2, required = true)]
        tracks: Vec<PathBuf>,
        /// How long to play
        #[arg(long, value_name = "SECONDS", default_value_t = 180)]
        seconds: u32,
        /// Take the device for this program alone, at a 256-frame period where it allows that.
        /// Other programs go silent meanwhile. Windows only
        #[arg(long)]
        exclusive: bool,
    },
    /// Run the core as a local process for the browser UI to talk to
    Serve {
        /// Write the port and token to this file as JSON, for the dev server to read
        #[arg(long, value_name = "PATH")]
        connection_file: Option<PathBuf>,
    },
}

fn finished<E: std::fmt::Display>(result: Result<(), E>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Key { tags } => show_keys(&tags),
        Command::Scan { folder } => finished(scan::run(&folder, cli.data_folder.as_deref())),
        Command::Identify { online } => finished(identify::run(cli.data_folder.as_deref(), online)),
        Command::Bench {
            page,
            audio_seconds,
        } => finished(bench::run(&page, audio_seconds)),
        Command::Audio {
            tracks,
            seconds,
            exclusive,
        } => {
            let mode = if exclusive {
                OutputMode::Exclusive
            } else {
                OutputMode::Shared
            };
            match <[PathBuf; 2]>::try_from(tracks) {
                Ok(tracks) => finished(bench::play_audio(&tracks, seconds, mode)),
                Err(_) => ExitCode::FAILURE,
            }
        }
        Command::Serve { connection_file } => finished(serve::run(
            connection_file.as_deref(),
            cli.data_folder.as_deref(),
        )),
    }
}

fn show_keys(tags: &[String]) -> ExitCode {
    let mut every_tag_read = true;
    for tag in tags {
        match tag.parse::<Key>() {
            Ok(key) => println!("{tag} -> {key}"),
            Err(error) => {
                eprintln!("{error}");
                every_tag_read = false;
            }
        }
    }
    if every_tag_read {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
