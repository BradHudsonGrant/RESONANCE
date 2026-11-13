use std::process::ExitCode;

use clap::{Parser, Subcommand};
use resonance_core::key::Key;

#[derive(Parser)]
#[command(name = "resonance", version, about = "Resonance from the command line")]
struct Cli {
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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Key { tags } => show_keys(&tags),
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
