mod bench;
mod library_file;
mod scan;
mod serve;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use resonance_core::key::Key;

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
        Command::Bench {
            page,
            audio_seconds,
        } => finished(bench::run(&page, audio_seconds)),
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
