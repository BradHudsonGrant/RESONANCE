use std::path::PathBuf;
use std::process::ExitCode;

use resonance_db::{Database, DbError, FILE_NAME};
use resonance_fixtures::synthetic_library;

/// Fills a library database with made-up tracks, for measuring the app at sizes no real folder
/// here has. Nothing is written outside the data folder named.
fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(data_folder) = args.next().map(PathBuf::from) else {
        eprintln!("Give the data folder to fill: synthetic-library <data-folder> [count]");
        return ExitCode::from(2);
    };
    let count = match args
        .next()
        .map(|count| count.to_string_lossy().parse::<usize>())
    {
        None => 100_000,
        Some(Ok(count)) => count,
        Some(Err(error)) => {
            eprintln!("The count is not a whole number: {error}");
            return ExitCode::from(2);
        }
    };
    match fill(&data_folder, count) {
        Ok(total) => {
            println!(
                "{} holds {total} tracks.",
                data_folder.join(FILE_NAME).display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn fill(data_folder: &std::path::Path, count: usize) -> Result<u32, DbError> {
    let mut database = Database::open(&data_folder.join(FILE_NAME))?;
    let source = database.source("synthetic")?;
    database.record_scan(source, &synthetic_library::tracks(count))?;
    database.track_count()
}
