//! Other DJ apps' libraries. Each kind has one [`ExternalLibraryAdapter`], and every write
//! through one goes through [`ExternalLibraryAdapter::apply_changes`]: back up, write, read
//! back, and put the backup back if the result is not what was planned.

pub mod m3u;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_core::playlist::{LineEdit, PathChange, PlaylistTextError, same_path};

/// The kinds of library Resonance can read and update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExternalLibraryKind {
    M3u,
}

impl ExternalLibraryKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::M3u => "m3u",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "m3u" => Some(Self::M3u),
            _ => None,
        }
    }

    pub fn adapter(self) -> &'static dyn ExternalLibraryAdapter {
        match self {
            Self::M3u => &m3u::M3u,
        }
    }
}

/// A file a playlist names, by its full path.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalEntry {
    pub path: String,
    pub title: Option<String>,
    pub duration_s: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalPlaylist {
    pub name: String,
    pub entries: Vec<ExternalEntry>,
    /// Lines that name a stream or other address rather than a file.
    pub skipped: u32,
}

/// What a library holds, as far as Resonance reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalLibrary {
    pub kind: ExternalLibraryKind,
    pub location: PathBuf,
    pub playlists: Vec<ExternalPlaylist>,
}

/// One file of a library and the exact edits a change makes to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub edits: Vec<LineEdit>,
    before: Vec<u8>,
    after: Vec<u8>,
}

impl PlannedFile {
    pub fn new(path: PathBuf, edits: Vec<LineEdit>, before: Vec<u8>, after: Vec<u8>) -> Self {
        Self {
            path,
            edits,
            before,
            after,
        }
    }
}

/// A dry run: what applying the changes would write, and which changes the library has no
/// file for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangePlan {
    pub files: Vec<PlannedFile>,
    pub not_found: Vec<String>,
}

impl ChangePlan {
    pub fn edit_count(&self) -> usize {
        self.files.iter().map(|file| file.edits.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChanges {
    pub backups: Vec<PathBuf>,
    pub edits: usize,
}

#[derive(Debug)]
pub enum IntegrationError {
    File {
        path: PathBuf,
        source: io::Error,
    },
    Text {
        path: PathBuf,
        source: PlaylistTextError,
    },
    NotAPlaylist {
        path: PathBuf,
    },
    /// Another app wrote the file between the dry run and the write.
    ChangedSinceRead {
        path: PathBuf,
    },
    /// What was written did not read back as planned, and the backup was put back.
    VerifyFailed {
        path: PathBuf,
        restored: bool,
    },
    AlreadyExists {
        path: PathBuf,
    },
}

impl fmt::Display for IntegrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Text { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NotAPlaylist { path } => {
                write!(f, "{} is not an M3U or M3U8 playlist.", path.display())
            }
            Self::ChangedSinceRead { path } => write!(
                f,
                "{} changed while it was being updated, so it was left as it is. Try again.",
                path.display()
            ),
            Self::VerifyFailed {
                path,
                restored: true,
            } => write!(
                f,
                "{} did not read back as planned, so its backup was put back.",
                path.display()
            ),
            Self::VerifyFailed {
                path,
                restored: false,
            } => write!(
                f,
                "{} did not read back as planned and its backup could not be put back. The \
                 backup is in the backups folder.",
                path.display()
            ),
            Self::AlreadyExists { path } => {
                write!(f, "{} already exists. Choose another name.", path.display())
            }
        }
    }
}

impl std::error::Error for IntegrationError {}

pub(crate) fn file_error(path: &Path) -> impl FnOnce(io::Error) -> IntegrationError + '_ {
    move |source| IntegrationError::File {
        path: path.to_owned(),
        source,
    }
}

/// Every library kind does the same six things: detect, read, plan, back up, apply, verify.
pub trait ExternalLibraryAdapter: Sync {
    fn kind(&self) -> ExternalLibraryKind;

    /// The libraries of this kind inside `folder`, by the location each is read from.
    fn detect(&self, folder: &Path) -> Result<Vec<PathBuf>, IntegrationError>;

    fn read_library(&self, location: &Path) -> Result<ExternalLibrary, IntegrationError>;

    /// The dry run of `changes`: nothing is written.
    fn preview_changes(
        &self,
        location: &Path,
        changes: &[PathChange],
    ) -> Result<ChangePlan, IntegrationError>;

    /// Applies `changes`. Each file is copied into `backup_folder` first, then written, then
    /// read back through [`Self::read_library`]. If any check fails, every file written is
    /// put back from its backup.
    fn apply_changes(
        &self,
        location: &Path,
        changes: &[PathChange],
        backup_folder: &Path,
    ) -> Result<AppliedChanges, IntegrationError> {
        let plan = self.preview_changes(location, changes)?;
        let mut backups = Vec::new();
        for file in &plan.files {
            backups.push(back_up(file, backup_folder)?);
        }
        let mut written = Vec::new();
        for file in &plan.files {
            written.push(file);
            if let Err(error) = write_whole(&file.path, &file.after) {
                restore(&written);
                return Err(error);
            }
        }
        for file in &plan.files {
            let read_back = fs::read(&file.path).map_err(file_error(&file.path));
            if !matches!(read_back, Ok(ref bytes) if *bytes == file.after) {
                return Err(IntegrationError::VerifyFailed {
                    path: file.path.clone(),
                    restored: restore(&written),
                });
            }
        }
        let reread = self.read_library(location);
        let named_now = |path: &str| {
            reread.as_ref().is_ok_and(|library| {
                library.playlists.iter().any(|playlist| {
                    playlist
                        .entries
                        .iter()
                        .any(|entry| same_path(&entry.path, path))
                })
            })
        };
        let found = changes
            .iter()
            .filter(|change| !plan.not_found.iter().any(|gone| gone == &change.old_path));
        for change in found {
            if !named_now(&change.new_path) {
                return Err(IntegrationError::VerifyFailed {
                    path: location.to_owned(),
                    restored: restore(&written),
                });
            }
        }
        Ok(AppliedChanges {
            backups,
            edits: plan.edit_count(),
        })
    }
}

/// Copies the file as the plan read it into `folder` under a name no earlier backup has, and
/// checks the copy. Refuses when the file changed since the plan read it.
fn back_up(file: &PlannedFile, folder: &Path) -> Result<PathBuf, IntegrationError> {
    let current = fs::read(&file.path).map_err(file_error(&file.path))?;
    if current != file.before {
        return Err(IntegrationError::ChangedSinceRead {
            path: file.path.clone(),
        });
    }
    fs::create_dir_all(folder).map_err(file_error(folder))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis());
    let name = file
        .path
        .file_name()
        .map_or_else(|| "library".into(), |name| name.to_string_lossy());
    let mut attempt = 0u32;
    loop {
        let candidate = if attempt == 0 {
            folder.join(format!("{stamp}-{name}"))
        } else {
            folder.join(format!("{stamp}-{attempt}-{name}"))
        };
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut out) => {
                io::Write::write_all(&mut out, &current).map_err(file_error(&candidate))?;
                out.sync_all().map_err(file_error(&candidate))?;
                drop(out);
                let copy = fs::read(&candidate).map_err(file_error(&candidate))?;
                if copy != current {
                    return Err(IntegrationError::VerifyFailed {
                        path: candidate,
                        restored: true,
                    });
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => attempt += 1,
            Err(error) => return Err(file_error(&candidate)(error)),
        }
    }
}

/// Writes beside the file and moves the result over it, so a crash leaves the old file or the
/// new one and never half of either.
pub(crate) fn write_whole(path: &Path, bytes: &[u8]) -> Result<(), IntegrationError> {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".resonance-tmp");
    let staged = PathBuf::from(staged);
    let written = (|| {
        let mut out = fs::File::create(&staged)?;
        io::Write::write_all(&mut out, bytes)?;
        out.sync_all()?;
        drop(out);
        fs::rename(&staged, path)
    })();
    written.map_err(|source| {
        let _ = fs::remove_file(&staged);
        IntegrationError::File {
            path: path.to_owned(),
            source,
        }
    })
}

/// Puts back what the plan read for each file written. True when every file went back.
fn restore(written: &[&PlannedFile]) -> bool {
    written
        .iter()
        .all(|file| write_whole(&file.path, &file.before).is_ok())
}
