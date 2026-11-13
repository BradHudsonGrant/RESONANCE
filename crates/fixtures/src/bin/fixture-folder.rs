use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use resonance_fixtures::signal::{ClickTrack, Signal};
use resonance_fixtures::silence::{write_silent_m4a, write_silent_mp3};

/// One made-up track in each format, named the way a real library names them, with no tags. The
/// browser tests scan it; it holds nothing anyone owns, so it can be made on any machine.
fn write_folder(folder: &Path) -> io::Result<usize> {
    let _ = fs::remove_dir_all(folder);
    fs::create_dir_all(folder.join("House"))?;
    fs::create_dir_all(folder.join("Garage"))?;
    let clicks = ClickTrack::at_tempo(124.0, 32, 44_100).signal;
    clicks.write_wav(&folder.join("House/Ada Marsh - First Light.wav"))?;
    clicks.write_flac(&folder.join("House/Ada Marsh - Second Wind.flac"))?;
    Signal::tone_at_loudness(-14.0, 8.0, 44_100)
        .write_aiff(&folder.join("Garage/Kofi Bell - Third Rail.aiff"))?;
    write_silent_mp3(&folder.join("Garage/Nia Okoro - Fourth Wall.mp3"), 6.0)?;
    write_silent_m4a(&folder.join("Nia Okoro - Fifth Floor.m4a"), 6.0)?;
    Ok(5)
}

fn main() -> ExitCode {
    let Some(folder) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Give the folder to write: fixture-folder <folder>");
        return ExitCode::from(2);
    };
    match write_folder(&folder) {
        Ok(count) => {
            println!("{count} tracks written to {}", folder.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{} could not be written: {error}", folder.display());
            ExitCode::FAILURE
        }
    }
}
