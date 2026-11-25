//! Operations made of file steps. The whole plan, and what puts each step back, is journaled
//! before the first file is touched. Steps run in order. When one fails, the ones before it are
//! put back. When the app stops in the middle, `recover` finishes or rolls back the operation
//! the next time the library opens, so the library and the disk never stay half changed.

use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use resonance_core::journal::{OperationKind, OperationState, Recovery, StepState, recovery};
use resonance_db::{Database, DbError, StoredOperation, StoredStep};
use serde::{Deserialize, Serialize};

use crate::{rename, write_tags};

/// One change to the disk or the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum Step {
    /// Copies a file to a name nothing has yet and checks the copy's bytes against `byte_hash`,
    /// a BLAKE3 hash in hex.
    CopyFile {
        from: PathBuf,
        to: PathBuf,
        byte_hash: String,
    },
    /// Moves a file to a name nothing has yet.
    MoveFile { from: PathBuf, to: PathBuf },
    /// Leaves a file out of the library while it is in the bin.
    HideAsset { asset_id: i64 },
    /// Moves a file to the Recycle Bin, or the desktop's trash.
    TrashFile { path: PathBuf, size_bytes: u64 },
    /// Deletes a file for good, where there is no bin to move it to.
    RemoveFile { path: PathBuf, size_bytes: u64 },
}

/// What puts one step back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "inverse", rename_all = "snake_case")]
pub enum Inverse {
    RemoveCopy {
        path: PathBuf,
    },
    MoveBack {
        from: PathBuf,
        to: PathBuf,
    },
    ShowAsset {
        asset_id: i64,
    },
    /// Finds the file in the bin by its old path and size, deleted no earlier than
    /// `deleted_from_s` in Unix seconds, and puts it back.
    RestoreFromBin {
        path: PathBuf,
        size_bytes: u64,
        deleted_from_s: i64,
    },
    /// A file deleted for good cannot come back.
    Nothing,
}

impl Step {
    /// What puts this step back, fixed when the plan is written. `planned_s` is when, in Unix
    /// seconds.
    pub fn inverse(&self, planned_s: i64) -> Inverse {
        match self {
            Self::CopyFile { to, .. } => Inverse::RemoveCopy { path: to.clone() },
            Self::MoveFile { from, to } => Inverse::MoveBack {
                from: to.clone(),
                to: from.clone(),
            },
            Self::HideAsset { asset_id } => Inverse::ShowAsset {
                asset_id: *asset_id,
            },
            Self::TrashFile { path, size_bytes } => Inverse::RestoreFromBin {
                path: path.clone(),
                size_bytes: *size_bytes,
                deleted_from_s: planned_s,
            },
            Self::RemoveFile { .. } => Inverse::Nothing,
        }
    }

    /// Whether the disk shows the step happened, for a step the journal never recorded as
    /// done. A change to the library is recorded in the same transaction as the step, so it
    /// never needs asking.
    fn took_effect_on_disk(&self) -> bool {
        match self {
            Self::CopyFile { to, .. } => exists(to),
            Self::MoveFile { from, to } => exists(to) && !exists(from),
            Self::HideAsset { .. } => false,
            Self::TrashFile { path, .. } | Self::RemoveFile { path, .. } => !exists(path),
        }
    }
}

#[derive(Debug)]
pub enum JournalError {
    Database(DbError),
    Plan(serde_json::Error),
    NoOperation(i64),
    CannotUndo {
        id: i64,
        state: OperationState,
    },
    /// The file was gone or had changed size since the plan was made.
    ChangedOnDisk(PathBuf),
    /// Another file already has the name a step needs.
    NameTaken(PathBuf),
    /// The copy's bytes do not match the file it was made from.
    CopyDiffers(PathBuf),
    /// The OS has no bin for this file, or refused to take it.
    NoBin {
        path: PathBuf,
        reason: String,
    },
    /// The file is no longer in the bin, so it cannot be put back.
    NotInBin(PathBuf),
    /// This system cannot restore from its trash. The file is still there to restore by hand.
    RestoreUnsupported(PathBuf),
    DeletedForGood(PathBuf),
    /// None of the tracks has a file in the library.
    NothingToDelete,
    File {
        path: PathBuf,
        source: io::Error,
    },
    /// A step failed, and the steps before it were put back.
    RolledBack {
        operation_id: i64,
        cause: Box<JournalError>,
    },
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(source) => source.fmt(f),
            Self::Plan(source) => write!(
                f,
                "The journal holds a step this build cannot read: {source}"
            ),
            Self::NoOperation(id) => write!(f, "There is no operation {id} in the journal."),
            Self::CannotUndo { id, state } => write!(
                f,
                "Operation {id} is {}, and only an applied operation can be undone.",
                state.name().replace('_', " ")
            ),
            Self::ChangedOnDisk(path) => {
                write!(f, "{} changed after the plan was made.", path.display())
            }
            Self::NameTaken(path) => write!(f, "Another file is already at {}.", path.display()),
            Self::CopyDiffers(path) => write!(
                f,
                "The copy at {} does not match the file it was made from.",
                path.display()
            ),
            Self::NoBin { path, reason } => write!(
                f,
                "{} could not be moved to the bin: {reason}",
                path.display()
            ),
            Self::NotInBin(path) => write!(
                f,
                "{} is no longer in the bin, so it cannot be put back.",
                path.display()
            ),
            Self::RestoreUnsupported(path) => write!(
                f,
                "{} is in the Trash. Put it back from there, then scan the folder.",
                path.display()
            ),
            Self::DeletedForGood(path) => write!(
                f,
                "{} was deleted for good and cannot be put back.",
                path.display()
            ),
            Self::NothingToDelete => f.write_str("None of these tracks has a file to delete."),
            Self::File { path, source } => write!(f, "{}: {source}", path.display()),
            Self::RolledBack { cause, .. } => {
                write!(f, "{cause} Nothing was changed.")
            }
        }
    }
}

impl std::error::Error for JournalError {}

impl From<DbError> for JournalError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

impl From<serde_json::Error> for JournalError {
    fn from(source: serde_json::Error) -> Self {
        Self::Plan(source)
    }
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn now_s() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

fn file_error(path: &Path) -> impl FnOnce(io::Error) -> JournalError + '_ {
    move |source| JournalError::File {
        path: path.to_owned(),
        source,
    }
}

/// The BLAKE3 hash of a file's bytes, in hex.
pub fn byte_hash_hex(path: &Path) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Journals `steps` as one operation before anything changes, and returns its id.
pub fn begin(
    database: &mut Database,
    kind: OperationKind,
    summary: &str,
    forward_json: &str,
    steps: &[Step],
) -> Result<i64, JournalError> {
    let planned_s = now_s();
    let mut plan = Vec::with_capacity(steps.len());
    for step in steps {
        plan.push((
            serde_json::to_string(step)?,
            serde_json::to_string(&step.inverse(planned_s))?,
        ));
    }
    Ok(database.begin_planned_operation(kind, summary, forward_json, &plan)?)
}

fn read_step(stored: &StoredStep) -> Result<(Step, Inverse), JournalError> {
    Ok((
        serde_json::from_str(&stored.forward_json)?,
        serde_json::from_str(&stored.inverse_json)?,
    ))
}

fn stored_steps(
    database: &Database,
    operation_id: i64,
) -> Result<Vec<(StoredStep, Step, Inverse)>, JournalError> {
    database
        .operation_steps(operation_id)?
        .into_iter()
        .map(|stored| {
            let (step, inverse) = read_step(&stored)?;
            Ok((stored, step, inverse))
        })
        .collect()
}

fn check_size(path: &Path, size_bytes: u64) -> Result<(), JournalError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() == size_bytes => Ok(()),
        _ => Err(JournalError::ChangedOnDisk(path.to_owned())),
    }
}

fn copy_file(from: &Path, to: &Path, byte_hash: &str) -> Result<(), JournalError> {
    let mut target = File::create_new(to).map_err(|error| match error.kind() {
        io::ErrorKind::AlreadyExists => JournalError::NameTaken(to.to_owned()),
        _ => JournalError::File {
            path: to.to_owned(),
            source: error,
        },
    })?;
    let copied = File::open(from)
        .and_then(|mut source| io::copy(&mut source, &mut target))
        .and_then(|_| target.sync_all());
    drop(target);
    let checked = copied.and_then(|()| byte_hash_hex(to));
    match checked {
        Ok(hash) if hash == byte_hash => Ok(()),
        Ok(_) => {
            let _ = fs::remove_file(to);
            Err(JournalError::CopyDiffers(to.to_owned()))
        }
        Err(source) => {
            let _ = fs::remove_file(to);
            Err(JournalError::File {
                path: from.to_owned(),
                source,
            })
        }
    }
}

fn move_to_bin(path: &Path, size_bytes: u64) -> Result<(), JournalError> {
    check_size(path, size_bytes)?;
    trash::delete(path).map_err(|error| JournalError::NoBin {
        path: path.to_owned(),
        reason: error.to_string(),
    })?;
    if exists(path) {
        return Err(JournalError::NoBin {
            path: path.to_owned(),
            reason: "the file is still in its folder".to_owned(),
        });
    }
    Ok(())
}

/// Runs step `position` of operation `operation_id` and records it done.
pub fn run_step(
    database: &mut Database,
    operation_id: i64,
    position: u32,
) -> Result<(), JournalError> {
    let stored = database
        .operation_steps(operation_id)?
        .into_iter()
        .find(|stored| stored.position == position)
        .ok_or(JournalError::NoOperation(operation_id))?;
    if stored.state == StepState::Done {
        return Ok(());
    }
    let (step, _) = read_step(&stored)?;
    database.start_step(operation_id, position)?;
    match &step {
        Step::CopyFile {
            from,
            to,
            byte_hash,
        } => copy_file(from, to, byte_hash)?,
        Step::MoveFile { from, to } => {
            if exists(to) {
                return Err(JournalError::NameTaken(to.clone()));
            }
            if !exists(from) {
                return Err(JournalError::ChangedOnDisk(from.clone()));
            }
            fs::rename(from, to).map_err(file_error(from))?;
        }
        Step::HideAsset { asset_id } => {
            return Ok(database.hide_trashed_asset(operation_id, position, *asset_id)?);
        }
        Step::TrashFile { path, size_bytes } => move_to_bin(path, *size_bytes)?,
        Step::RemoveFile { path, size_bytes } => {
            check_size(path, *size_bytes)?;
            fs::remove_file(path).map_err(file_error(path))?;
        }
    }
    Ok(database.set_step(operation_id, position, StepState::Done)?)
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn restore_from_bin(path: &Path, size_bytes: u64, deleted_from_s: i64) -> Result<(), JournalError> {
    use trash::os_limited;

    let wanted = comparable(path);
    let bin_error = |error: trash::Error| JournalError::File {
        path: path.to_owned(),
        source: io::Error::other(error.to_string()),
    };
    // A clock that steps back a little between planning and deleting should not hide the file.
    let earliest = deleted_from_s.saturating_sub(60);
    let item = os_limited::list()
        .map_err(bin_error)?
        .into_iter()
        .filter(|item| comparable(&item.original_path()) == wanted)
        .filter(|item| item.time_deleted >= earliest)
        .filter(|item| {
            os_limited::metadata(item)
                .ok()
                .and_then(|metadata| metadata.size.size())
                .is_none_or(|size| size == size_bytes)
        })
        .max_by_key(|item| item.time_deleted)
        .ok_or_else(|| JournalError::NotInBin(path.to_owned()))?;
    os_limited::restore_all([item]).map_err(bin_error)?;
    if exists(path) {
        Ok(())
    } else {
        Err(JournalError::NotInBin(path.to_owned()))
    }
}

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn restore_from_bin(path: &Path, _: u64, _: i64) -> Result<(), JournalError> {
    Err(JournalError::RestoreUnsupported(path.to_owned()))
}

/// A path as the bin reports it, for matching. The bin gives the folder in its usual form.
#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn comparable(path: &Path) -> String {
    let folder = path
        .parent()
        .and_then(|folder| dunce::canonicalize(folder).ok())
        .unwrap_or_else(|| path.parent().unwrap_or(Path::new("")).to_owned());
    let full = folder.join(path.file_name().unwrap_or_default());
    let text = dunce::simplified(&full).to_string_lossy().into_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

/// Puts one step back. Each inverse checks the disk first, so running it again after a crash
/// does no harm.
fn put_back(
    database: &mut Database,
    operation_id: i64,
    position: u32,
    inverse: &Inverse,
) -> Result<(), JournalError> {
    match inverse {
        Inverse::RemoveCopy { path } => {
            if exists(path) {
                fs::remove_file(path).map_err(file_error(path))?;
            }
        }
        Inverse::MoveBack { from, to } => match (exists(from), exists(to)) {
            (true, false) => fs::rename(from, to).map_err(file_error(from))?,
            (false, true) => {}
            (true, true) => return Err(JournalError::NameTaken(to.clone())),
            (false, false) => return Err(JournalError::ChangedOnDisk(from.clone())),
        },
        Inverse::ShowAsset { asset_id } => {
            return Ok(database.show_trashed_asset(operation_id, position, *asset_id)?);
        }
        Inverse::RestoreFromBin {
            path,
            size_bytes,
            deleted_from_s,
        } => {
            if !exists(path) {
                restore_from_bin(path, *size_bytes, *deleted_from_s)?;
            }
        }
        Inverse::Nothing => {}
    }
    Ok(database.set_step(operation_id, position, StepState::Undone)?)
}

fn deleted_for_good(step: &Step) -> Option<&Path> {
    match step {
        Step::RemoveFile { path, .. } => Some(path),
        _ => None,
    }
}

/// Puts back every step recorded done, newest first.
fn roll_back(database: &mut Database, operation_id: i64) -> Result<(), JournalError> {
    for (stored, _, inverse) in stored_steps(database, operation_id)?.into_iter().rev() {
        if stored.state == StepState::Done {
            put_back(database, operation_id, stored.position, &inverse)?;
        }
    }
    Ok(())
}

/// Journals `steps` as one operation and runs them. When a step fails, the ones done are put
/// back and the operation is recorded as rolled back.
pub fn apply(
    database: &mut Database,
    kind: OperationKind,
    summary: &str,
    forward_json: &str,
    steps: &[Step],
) -> Result<StoredOperation, JournalError> {
    let _operation = crate::file_operation();
    let operation_id = begin(database, kind, summary, forward_json, steps)?;
    for position in 0..steps.len() {
        let position = u32::try_from(position).unwrap_or(u32::MAX);
        if let Err(cause) = run_step(database, operation_id, position) {
            let (state, note) = match roll_back(database, operation_id) {
                Ok(()) => (
                    OperationState::RolledBack,
                    format!("Stopped and put back: {cause}"),
                ),
                Err(problem) => (
                    OperationState::Failed,
                    format!("Stopped: {cause} Putting it back failed: {problem}"),
                ),
            };
            database.finish_operation(operation_id, state, &note)?;
            return Err(JournalError::RolledBack {
                operation_id,
                cause: Box::new(cause),
            });
        }
    }
    database.finish_operation(operation_id, OperationState::Applied, summary)?;
    database
        .operation(operation_id)?
        .ok_or(JournalError::NoOperation(operation_id))
}

/// Puts back every step of an applied operation, newest first. A step already put back is
/// skipped, so an undo that stopped part way can be run again.
pub fn undo(database: &mut Database, operation_id: i64) -> Result<StoredOperation, JournalError> {
    let _operation = crate::file_operation();
    let operation = database
        .operation(operation_id)?
        .ok_or(JournalError::NoOperation(operation_id))?;
    if !operation.state.can_undo() {
        return Err(JournalError::CannotUndo {
            id: operation_id,
            state: operation.state,
        });
    }
    let steps = stored_steps(database, operation_id)?;
    if let Some(path) = steps.iter().find_map(|(_, step, _)| deleted_for_good(step)) {
        return Err(JournalError::DeletedForGood(path.to_owned()));
    }
    for (stored, _, inverse) in steps.iter().rev() {
        if stored.state == StepState::Done {
            put_back(database, operation_id, stored.position, inverse)?;
        }
    }
    database.finish_operation(operation_id, OperationState::Undone, &operation.summary)?;
    database
        .operation(operation_id)?
        .ok_or(JournalError::NoOperation(operation_id))
}

/// How one unfinished operation was brought to rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    pub operation_id: i64,
    pub state: OperationState,
    pub note: String,
}

fn recover_steps(
    database: &mut Database,
    operation: &StoredOperation,
) -> Result<(OperationState, String), JournalError> {
    let steps = stored_steps(database, operation.id)?;
    let took_effect: Vec<bool> = steps
        .iter()
        .map(|(stored, step, _)| match stored.state {
            StepState::Done => true,
            StepState::Undone => false,
            StepState::Planned => step.took_effect_on_disk(),
        })
        .collect();
    let mut decision = recovery(&took_effect);
    if decision == Recovery::Finish {
        for (stored, step, _) in &steps {
            if let (StepState::Planned, Step::CopyFile { to, byte_hash, .. }) = (stored.state, step)
                && byte_hash_hex(to).ok().as_deref() != Some(byte_hash.as_str())
            {
                decision = Recovery::RollBack((0..steps.len()).rev().collect());
            }
        }
    }
    match decision {
        Recovery::Finish => {
            for (stored, _, _) in &steps {
                if stored.state == StepState::Planned {
                    database.set_step(operation.id, stored.position, StepState::Done)?;
                }
            }
            Ok((
                OperationState::Applied,
                "Every step had happened before the app stopped, so it was finished.".to_owned(),
            ))
        }
        Recovery::RollBack(positions) => {
            for position in positions {
                let (stored, _, inverse) = &steps[position];
                put_back(database, operation.id, stored.position, inverse)?;
            }
            Ok((
                OperationState::RolledBack,
                "The app stopped part way, so the steps that had happened were put back."
                    .to_owned(),
            ))
        }
    }
}

/// Brings every operation the app stopped in the middle of to rest: finished when every step
/// had happened, otherwise rolled back. Run it when the library opens, before anything else
/// touches the files. A problem is noted on the operation, which stays unfinished so the next
/// start tries again.
pub fn recover(database: &mut Database) -> Result<Vec<Recovered>, JournalError> {
    let _operation = crate::file_operation();
    let mut recovered = Vec::new();
    for operation in database.unfinished_operations()? {
        let outcome = match operation.kind {
            OperationKind::WriteTags => write_tags::recover(database, &operation),
            OperationKind::RenameFiles => rename::recover(database, &operation),
            OperationKind::DeleteFiles | OperationKind::DeleteFilesForGood => {
                recover_steps(database, &operation)
            }
        };
        match outcome {
            Ok((state, note)) => {
                database.record_recovery(operation.id, state, &note)?;
                recovered.push(Recovered {
                    operation_id: operation.id,
                    state,
                    note,
                });
            }
            Err(JournalError::Database(error)) => return Err(error.into()),
            Err(problem) => {
                database
                    .note_recovery_problem(operation.id, &format!("Recovery stopped: {problem}"))?;
            }
        }
    }
    Ok(recovered)
}
