use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use resonance_fixtures::local_set::{self, MANIFEST_NAME};
use serde_json::Value;

const MEGABYTE: usize = 1 << 20;
const SHA256_OF_A_MEGABYTE_OF_ZEROS: &str =
    "30e14955ebf1352266dc2ff8067e68104607e750abb9d3b36582b8af909fcb58";

struct Library {
    source: PathBuf,
    repo: PathBuf,
    destination: PathBuf,
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("the file has a folder"))
        .expect("the folder can be made");
    fs::write(path, bytes).expect("the file can be written");
}

/// A music folder with the repo inside it, as on the machine this is built on.
fn library(name: &str) -> Library {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("local_set")
        .join(name);
    let _ = fs::remove_dir_all(&root);
    let source = root.join("Music");
    let repo = source.join("RESONANCE");

    write(&source.join("House/first.mp3"), &vec![0; MEGABYTE]);
    write(&source.join("House/EP/second.flac"), &vec![1; MEGABYTE]);
    write(&source.join("Garage/third.wav"), &vec![2; MEGABYTE + 7]);
    write(&source.join("fourth.m4a"), &vec![3; MEGABYTE]);
    write(&source.join("House/jingle.mp3"), &[4; 100]);
    write(&source.join("House/cover.jpg"), &vec![5; MEGABYTE]);
    write(&repo.join("target/tmp/generated.aiff"), &vec![6; MEGABYTE]);

    Library {
        destination: repo.join("fixtures-local"),
        source,
        repo,
    }
}

fn files_under(folder: &Path) -> Vec<(PathBuf, Vec<u8>, SystemTime)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).expect("the folder lists") {
        let path = entry.expect("the entry reads").path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .expect("the file has a modified time");
            let bytes = fs::read(&path).expect("the file reads");
            found.push((path, bytes, modified));
        }
    }
    found.sort();
    found
}

fn copied_names(library: &Library) -> Vec<String> {
    files_under(&library.destination)
        .into_iter()
        .map(|(path, _, _)| {
            path.strip_prefix(&library.destination)
                .expect("the file is under the destination")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

#[test]
fn tracks_are_copied_byte_for_byte_and_small_files_other_files_and_the_repo_are_left_out() {
    let library = library("copies");

    let manifest = local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the copy succeeds");

    assert_eq!(manifest.tracks.len(), 4);
    assert_eq!(
        copied_names(&library),
        [
            "Garage/third.wav",
            "House/first.mp3",
            "House/second.flac",
            "fourth.m4a",
            MANIFEST_NAME
        ]
    );
    for track in &manifest.tracks {
        let copy = fs::read(library.destination.join(&track.to)).expect("the copy reads");
        let original = fs::read(&track.from).expect("the original reads");
        assert_eq!(copy, original, "{} differs from its original", track.to);
    }
}

#[test]
fn the_manifest_says_where_each_copy_came_from_with_its_size_and_hash() {
    let library = library("manifest");

    local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the copy succeeds");

    let text =
        fs::read_to_string(library.destination.join(MANIFEST_NAME)).expect("the manifest reads");
    let manifest: Value = serde_json::from_str(&text).expect("the manifest is JSON");
    assert_eq!(
        manifest["source"],
        library.source.to_string_lossy().as_ref()
    );
    let first = manifest["tracks"]
        .as_array()
        .expect("the manifest lists tracks")
        .iter()
        .find(|track| track["to"] == "House/first.mp3")
        .expect("the MP3 is in the manifest");
    assert_eq!(
        first["from"],
        library
            .source
            .join("House")
            .join("first.mp3")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(first["format"], "mp3");
    assert_eq!(first["bytes"], MEGABYTE);
    assert_eq!(first["sha256"], SHA256_OF_A_MEGABYTE_OF_ZEROS);
}

#[test]
fn the_originals_keep_their_bytes_and_their_modified_times() {
    let library = library("originals");
    let originals = |library: &Library| -> Vec<_> {
        files_under(&library.source)
            .into_iter()
            .filter(|(path, _, _)| !path.starts_with(&library.destination))
            .collect()
    };
    let before = originals(&library);

    local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the copy succeeds");

    assert_eq!(originals(&library), before);
}

#[test]
fn a_second_run_leaves_the_copies_it_already_made_alone() {
    let library = library("second_run");
    local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the first copy succeeds");
    let tracks = |library: &Library| -> Vec<_> {
        files_under(&library.destination)
            .into_iter()
            .filter(|(path, _, _)| !path.ends_with(MANIFEST_NAME))
            .collect()
    };
    let before = tracks(&library);

    let manifest = local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the second copy succeeds");

    assert_eq!(manifest.tracks.len(), 4);
    assert_eq!(tracks(&library), before);
}

#[test]
fn two_tracks_with_one_name_in_one_top_folder_both_get_a_copy() {
    let library = library("same_name");
    write(
        &library.source.join("House/Other EP/second.flac"),
        &vec![9; MEGABYTE],
    );

    let manifest = local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the copy succeeds");

    let flacs: Vec<&str> = manifest
        .tracks
        .iter()
        .filter(|track| track.to.ends_with("second.flac"))
        .map(|track| track.to.as_str())
        .collect();
    assert_eq!(flacs.len(), 2);
    assert_ne!(flacs[0], flacs[1]);
    for name in flacs {
        assert!(library.destination.join(name).is_file());
    }
}

#[test]
fn a_folder_whose_name_ends_in_a_space_is_read_like_any_other() {
    let library = library("trailing_space");
    let source = fs::canonicalize(&library.source).expect("the source resolves");
    write(
        &source.join("House/Solid Kick /fifth.aiff"),
        &vec![8; MEGABYTE],
    );

    let manifest = local_set::copy(&library.source, &library.destination, &library.repo)
        .expect("the copy succeeds");

    let fifth = manifest
        .tracks
        .iter()
        .find(|track| track.to == "House/fifth.aiff")
        .expect("the track in the awkward folder is copied");
    assert_eq!(
        fs::read(library.destination.join(&fifth.to)).expect("the copy reads"),
        vec![8; MEGABYTE]
    );
}

#[test]
fn a_source_that_does_not_exist_is_reported_by_its_path() {
    let library = library("missing");
    let missing = library.source.join("Nowhere");

    let error = local_set::copy(&missing, &library.destination, &library.repo)
        .expect_err("a missing folder cannot be copied from");

    assert!(error.to_string().contains("Nowhere"));
}
