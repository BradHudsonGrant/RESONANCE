use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use resonance_fixtures::signal::Signal;
use resonance_fixtures::silence::write_silent_mp3;

fn music_folder(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("scan_command")
        .join(name);
    // The data folder sits beside the music, and a database left by an earlier run would
    // already know these paths.
    let _ = fs::remove_dir_all(&root);
    let music = root.join("Music");
    fs::create_dir_all(music.join("House")).expect("the folder can be made");
    write_silent_mp3(&music.join("House/one.mp3"), 1.0).expect("the MP3 writes");
    Signal::tone(440.0, 1.0, -6.0, 44_100)
        .write_wav(&music.join("two.wav"))
        .expect("the WAV writes");
    music
}

fn scan(music: &Path, data_folder: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_resonance"))
        .arg("scan")
        .arg(music)
        .arg("--data-folder")
        .arg(data_folder)
        .output()
        .expect("the resonance binary starts")
}

fn lines(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn scan_reports_what_it_added_and_a_second_run_finds_nothing_changed() {
    let music = music_folder("twice");
    let data_folder = music.join("..").join("data");

    let first = scan(&music, &data_folder);
    let second = scan(&music, &data_folder);

    assert!(first.status.success());
    assert_eq!(
        lines(&first.stdout)[1],
        "2 files: 2 added, 0 changed, 0 unchanged"
    );
    assert!(second.status.success());
    assert_eq!(
        lines(&second.stdout)[1],
        "2 files: 0 added, 0 changed, 2 unchanged"
    );
    assert!(data_folder.join("library.db").is_file());
}

#[test]
fn scan_stays_out_of_the_folder_that_holds_its_own_data() {
    let music = music_folder("own_data");
    let own = music.join("Resonance");
    fs::create_dir_all(&own).expect("the folder can be made");
    Signal::tone(440.0, 1.0, -6.0, 44_100)
        .write_wav(&own.join("test-audio.wav"))
        .expect("the WAV writes");

    let output = scan(&music, &own.join("data"));

    assert!(output.status.success());
    assert_eq!(
        lines(&output.stdout)[1],
        "2 files: 2 added, 0 changed, 0 unchanged"
    );
}

#[test]
fn scan_fails_and_names_a_folder_that_is_not_there() {
    let music = music_folder("missing");

    let output = scan(&music.join("Nowhere"), &music.join("..").join("data"));

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Nowhere"));
}
