//! A copy of real tracks for the tests that need real music. It stays on the machine that made
//! it, and the originals are only ever read.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Mp3,
    Flac,
    Aiff,
    Wav,
    M4a,
}

impl Format {
    const ALL: [Self; 5] = [Self::Mp3, Self::Flac, Self::Aiff, Self::Wav, Self::M4a];

    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "mp3" => Some(Self::Mp3),
            "flac" => Some(Self::Flac),
            "aif" | "aiff" => Some(Self::Aiff),
            "wav" => Some(Self::Wav),
            "m4a" => Some(Self::M4a),
            _ => None,
        }
    }

    /// How many of the set's 50 tracks are in this format. MP3 is most of a real library, and
    /// the lossless formats are where decoding and hashing have the most to get wrong.
    fn share(self) -> usize {
        match self {
            Self::Mp3 => 20,
            Self::Flac => 10,
            Self::Aiff => 8,
            Self::Wav => 7,
            Self::M4a => 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub bytes: u64,
}

/// Anything smaller is more likely a sample or a jingle than a track.
const SMALLEST_BYTES: u64 = 1 << 20;
/// Keeps the whole set to a gigabyte or so.
const LARGEST_BYTES: u64 = 64 << 20;

fn relative<'a>(source: &Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(source).unwrap_or(path)
}

/// The folder directly under the source that holds the file, or nothing for a file at the top.
fn top_folder(source: &Path, path: &Path) -> String {
    let mut parts = relative(source, path).components();
    match (parts.next(), parts.next()) {
        (Some(first), Some(_)) => first.as_os_str().to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

/// A fixed hash of the path. Ordering by it spreads the picks through a folder, where ordering
/// by name would take the first album every time, and it gives the same set on every run.
fn scatter(path: &Path) -> u64 {
    path.to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// Picks each format's share, taking turns between the source's top-level folders so that one
/// large folder cannot fill the set.
pub fn choose(source: &Path, candidates: &[Candidate]) -> Vec<Candidate> {
    let mut chosen = Vec::new();
    for format in Format::ALL {
        let mut by_folder: BTreeMap<String, Vec<&Candidate>> = BTreeMap::new();
        for candidate in candidates.iter().filter(|candidate| {
            Format::of(&candidate.path) == Some(format)
                && (SMALLEST_BYTES..=LARGEST_BYTES).contains(&candidate.bytes)
        }) {
            by_folder
                .entry(top_folder(source, &candidate.path))
                .or_default()
                .push(candidate);
        }
        for tracks in by_folder.values_mut() {
            tracks.sort_by(|a, b| {
                let key = |candidate: &Candidate| scatter(relative(source, &candidate.path));
                key(a).cmp(&key(b)).then_with(|| a.path.cmp(&b.path))
            });
        }
        let longest = by_folder.values().map(Vec::len).max().unwrap_or(0);
        let in_turn = (0..longest).flat_map(|turn| {
            by_folder
                .values()
                .filter_map(move |tracks| tracks.get(turn))
        });
        chosen.extend(in_turn.take(format.share()).map(|&track| track.clone()));
    }
    chosen
}

#[derive(Debug)]
pub enum LocalSetError {
    Read { path: PathBuf, source: io::Error },
    Write { path: PathBuf, source: io::Error },
    CopyDiffers { from: PathBuf, to: PathBuf },
}

impl fmt::Display for LocalSetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "{} could not be read: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(f, "{} could not be written: {source}", path.display())
            }
            Self::CopyDiffers { from, to } => write!(
                f,
                "The copy at {} does not match {}",
                to.display(),
                from.display()
            ),
        }
    }
}

impl std::error::Error for LocalSetError {}

fn read_error(path: &Path) -> impl FnOnce(io::Error) -> LocalSetError + '_ {
    move |source| LocalSetError::Read {
        path: path.to_owned(),
        source,
    }
}

fn write_error(path: &Path) -> impl FnOnce(io::Error) -> LocalSetError + '_ {
    move |source| LocalSetError::Write {
        path: path.to_owned(),
        source,
    }
}

fn audio_files(
    folder: &Path,
    skipping: &Path,
    found: &mut Vec<Candidate>,
) -> Result<(), LocalSetError> {
    for entry in fs::read_dir(folder).map_err(read_error(folder))? {
        let entry = entry.map_err(read_error(folder))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(read_error(&path))?;
        if kind.is_dir() {
            if path != skipping {
                audio_files(&path, skipping, found)?;
            }
        } else if kind.is_file() && Format::of(&path).is_some() {
            let bytes = entry.metadata().map_err(read_error(&path))?.len();
            found.push(Candidate { path, bytes });
        }
    }
    Ok(())
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sha256_of(path: &Path) -> Result<String, LocalSetError> {
    let mut file = File::open(path).map_err(read_error(path))?;
    let mut hasher = Sha256::new();
    let mut chunk = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut chunk).map_err(read_error(path))?;
        if read == 0 {
            return Ok(hex(hasher.finalize()));
        }
        hasher.update(&chunk[..read]);
    }
}

/// Copies the bytes and nothing else. `fs::copy` would carry a read-only flag across, and a
/// later run could not replace the file.
fn copy_bytes(from: &Path, to: &Path) -> Result<(), LocalSetError> {
    let mut source = File::open(from).map_err(read_error(from))?;
    let mut copy = File::create(to).map_err(write_error(to))?;
    let mut chunk = vec![0; 1 << 16];
    loop {
        let read = source.read(&mut chunk).map_err(read_error(from))?;
        if read == 0 {
            return copy.flush().map_err(write_error(to));
        }
        copy.write_all(&chunk[..read]).map_err(write_error(to))?;
    }
}

#[derive(Debug, Serialize)]
pub struct CopiedTrack {
    pub from: String,
    pub to: String,
    pub format: Format,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub source: String,
    pub tracks: Vec<CopiedTrack>,
}

pub const MANIFEST_NAME: &str = "manifest.json";

/// Copies a spread of tracks from `source` into `destination` and writes a manifest of what came
/// from where. Nothing under `skipping` is considered, which keeps the repo's own files out when
/// the repo sits inside the music folder.
pub fn copy(source: &Path, destination: &Path, skipping: &Path) -> Result<Manifest, LocalSetError> {
    // Walked in its canonical form because on Windows that is the only form that can open a
    // folder whose name ends in a space or a dot, and music folders have them.
    let root = fs::canonicalize(source).map_err(read_error(source))?;
    // A folder that does not exist yet cannot hold anything to skip.
    let skipping = fs::canonicalize(skipping).unwrap_or_else(|_| skipping.to_owned());
    let mut candidates = Vec::new();
    audio_files(&root, &skipping, &mut candidates)?;

    let mut used = HashSet::new();
    let mut tracks = Vec::new();
    for track in choose(&root, &candidates) {
        let Some(format) = Format::of(&track.path) else {
            continue;
        };
        let name = track
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = top_folder(&root, &track.path);
        let mut to = Path::new(&folder).join(&name);
        if !used.insert(to.clone()) {
            let tag = scatter(relative(&root, &track.path)) as u32;
            to = Path::new(&folder).join(format!("{tag:08x}-{name}"));
            used.insert(to.clone());
        }

        let target = destination.join(&to);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(write_error(parent))?;
        }
        let sha256 = sha256_of(&track.path)?;
        let already_there = target.is_file() && sha256_of(&target)? == sha256;
        if !already_there {
            copy_bytes(&track.path, &target)?;
            if sha256_of(&target)? != sha256 {
                return Err(LocalSetError::CopyDiffers {
                    from: track.path,
                    to: target,
                });
            }
        }
        tracks.push(CopiedTrack {
            from: source
                .join(relative(&root, &track.path))
                .to_string_lossy()
                .into_owned(),
            to: to.to_string_lossy().replace('\\', "/"),
            format,
            bytes: track.bytes,
            sha256,
        });
    }

    let manifest = Manifest {
        source: source.to_string_lossy().into_owned(),
        tracks,
    };
    let path = destination.join(MANIFEST_NAME);
    let text = serde_json::to_string_pretty(&manifest)
        .map_err(io::Error::other)
        .map_err(write_error(&path))?;
    fs::create_dir_all(destination).map_err(write_error(destination))?;
    fs::write(&path, text + "\n").map_err(write_error(&path))?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEGABYTE: u64 = 1 << 20;

    fn candidate(path: &str, bytes: u64) -> Candidate {
        Candidate {
            path: PathBuf::from(path),
            bytes,
        }
    }

    fn in_folder(chosen: &[Candidate], folder: &str) -> usize {
        chosen
            .iter()
            .filter(|track| track.path.starts_with(Path::new("music").join(folder)))
            .count()
    }

    #[test]
    fn a_format_gets_its_share_split_between_the_top_level_folders() {
        let candidates: Vec<Candidate> = (0..30)
            .flat_map(|n| {
                [
                    candidate(&format!("music/House/{n}.mp3"), 5 * MEGABYTE),
                    candidate(&format!("music/Garage/deep/{n}.mp3"), 5 * MEGABYTE),
                ]
            })
            .collect();

        let chosen = choose(Path::new("music"), &candidates);

        assert_eq!(chosen.len(), 20);
        assert_eq!(in_folder(&chosen, "House"), 10);
        assert_eq!(in_folder(&chosen, "Garage"), 10);
    }

    #[test]
    fn a_small_folder_gives_what_it_has_and_the_others_make_up_the_share() {
        let mut candidates: Vec<Candidate> = (0..30)
            .map(|n| candidate(&format!("music/House/{n}.flac"), 30 * MEGABYTE))
            .collect();
        candidates.push(candidate("music/Albums/one.flac", 30 * MEGABYTE));

        let chosen = choose(Path::new("music"), &candidates);

        assert_eq!(chosen.len(), 10);
        assert_eq!(in_folder(&chosen, "Albums"), 1);
    }

    #[test]
    fn every_format_is_taken_and_other_files_are_not() {
        let candidates = [
            candidate("music/a.mp3", MEGABYTE),
            candidate("music/b.FLAC", MEGABYTE),
            candidate("music/c.aif", MEGABYTE),
            candidate("music/d.aiff", MEGABYTE),
            candidate("music/e.wav", MEGABYTE),
            candidate("music/f.m4a", MEGABYTE),
            candidate("music/g.ogg", MEGABYTE),
            candidate("music/cover.jpg", MEGABYTE),
        ];

        let chosen = choose(Path::new("music"), &candidates);

        assert_eq!(chosen.len(), 6);
    }

    #[test]
    fn files_too_small_to_be_a_track_or_too_large_for_the_set_are_left_out() {
        let candidates = [
            candidate("music/jingle.wav", MEGABYTE - 1),
            candidate("music/track.wav", 40 * MEGABYTE),
            candidate("music/live-set.wav", 65 * MEGABYTE),
        ];

        let chosen = choose(Path::new("music"), &candidates);

        assert_eq!(chosen, [candidate("music/track.wav", 40 * MEGABYTE)]);
    }

    #[test]
    fn the_same_files_give_the_same_set_whatever_order_they_were_found_in() {
        let candidates: Vec<Candidate> = (0..40)
            .map(|n| candidate(&format!("music/House/{n}.mp3"), 5 * MEGABYTE))
            .collect();
        let reversed: Vec<Candidate> = candidates.iter().rev().cloned().collect();

        assert_eq!(
            choose(Path::new("music"), &candidates),
            choose(Path::new("music"), &reversed)
        );
    }
}
