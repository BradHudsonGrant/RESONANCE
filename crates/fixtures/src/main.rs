use std::path::{Path, PathBuf};
use std::process::ExitCode;

use resonance_fixtures::local_set;

fn main() -> ExitCode {
    let Some(source) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Give the music folder to copy from: resonance-fixtures <folder>");
        return ExitCode::from(2);
    };
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let destination = repo.join("fixtures-local");

    match local_set::copy(&source, &destination, &repo) {
        Ok(manifest) => {
            let bytes: u64 = manifest.tracks.iter().map(|track| track.bytes).sum();
            println!(
                "{} tracks, {} MB, are in fixtures-local with a manifest of where each came from.",
                manifest.tracks.len(),
                bytes / 1_000_000
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
