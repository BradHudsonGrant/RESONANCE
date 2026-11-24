//! Renames files from their tags inside their own folder, as one journaled operation with undo.
//!
//! The database lock is held while each file is renamed and its path recorded, so a scan the
//! watcher starts never sees a file on disk under a name the library does not know yet.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use resonance_core::journal::{OperationKind, OperationState};
use resonance_core::library::TagField;
use resonance_core::template::{
    NameClaims, NamePlan, RenameStatus, Template, is_case_only, plan_name,
};
use resonance_db::{Database, DbError, RenameTarget, StoredOperation};

use crate::identity::full_asset_path;

/// The name a case-only rename passes through, so file systems that ignore case see a change.
const CASE_STEP_SUFFIX: &str = ".resonance-rename";

#[derive(Debug)]
pub enum RenameError {
    Database(DbError),
    NoOperation(i64),
    NotARename(i64),
    CannotUndo {
        id: i64,
        state: OperationState,
    },
    /// A file was renamed or moved again after the operation, so undoing would lose that.
    ChangedSinceRename(PathBuf),
    File {
        path: PathBuf,
        problem: FileProblem,
    },
}

impl fmt::Display for RenameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(source) => source.fmt(f),
            Self::NoOperation(id) => write!(f, "There is no operation {id} in the journal."),
            Self::NotARename(id) => write!(f, "Operation {id} did not rename files."),
            Self::CannotUndo { id, state } => write!(
                f,
                "Operation {id} is {}, and only an applied operation can be undone.",
                state.name()
            ),
            Self::ChangedSinceRename(path) => write!(
                f,
                "{} changed after it was renamed, so undoing would lose that change.",
                path.display()
            ),
            Self::File { path, problem } => write!(f, "{}: {problem}.", path.display()),
        }
    }
}

impl std::error::Error for RenameError {}

impl From<DbError> for RenameError {
    fn from(source: DbError) -> Self {
        Self::Database(source)
    }
}

/// Why one file was not renamed.
#[derive(Debug)]
pub enum FileProblem {
    Missing,
    /// Another file took the name after the preview.
    NameTaken,
    Io(io::Error),
    Database(DbError),
}

impl fmt::Display for FileProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => f.write_str("the file is missing"),
            Self::NameTaken => f.write_str("another file has the new name"),
            Self::Io(source) => write!(f, "the file could not be renamed: {source}"),
            Self::Database(source) => write!(f, "the new name could not be recorded: {source}"),
        }
    }
}

/// One file's part of a preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRename {
    pub asset_id: i64,
    pub track_id: i64,
    pub status: RenameStatus,
    pub old_name: String,
    pub new_name: Option<String>,
    pub missing: Vec<TagField>,
    source_folder: String,
    /// The folder relative to the source, empty at its top.
    folder: String,
}

impl PlannedRename {
    fn old_path(&self) -> String {
        join(&self.folder, &self.old_name)
    }

    fn full_path(&self, name: &str) -> PathBuf {
        full_asset_path(&self.source_folder, &join(&self.folder, name))
    }
}

#[derive(Debug)]
pub struct FailedRename {
    pub track_id: i64,
    pub path: PathBuf,
    pub problem: FileProblem,
}

#[derive(Debug, Default)]
pub struct RenameReport {
    /// `None` when no file was renamed.
    pub operation_id: Option<i64>,
    pub renamed: u32,
    pub failed: Vec<FailedRename>,
    pub stopped: bool,
}

#[derive(Debug)]
pub struct UndoReport {
    pub operation: StoredOperation,
    pub restored: u32,
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    }
}

fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn locked(database: &Mutex<Database>) -> MutexGuard<'_, Database> {
    database.lock().unwrap_or_else(PoisonError::into_inner)
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn names_on_disk(folder: &Path) -> Vec<String> {
    fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn plan_target(
    database: &Database,
    target: RenameTarget,
    template: &Template,
    claims: &mut NameClaims,
) -> Result<PlannedRename, DbError> {
    let (folder, old_name) = split(&target.path);
    let folder_units = utf16_len(&target.source_folder)
        + if folder.is_empty() {
            0
        } else {
            1 + utf16_len(folder)
        };
    let NamePlan {
        mut status,
        new_name,
        missing,
    } = plan_name(
        template,
        |field| {
            target
                .tags
                .iter()
                .find(|(tag, _)| *tag == field)
                .map(|(_, value)| value.as_str())
        },
        old_name,
        folder_units,
    );
    if target.missing && status != RenameStatus::MissingFields {
        status = RenameStatus::FileMissing;
    }
    let mut planned = PlannedRename {
        asset_id: target.asset_id,
        track_id: target.track_id,
        status,
        old_name: old_name.to_owned(),
        new_name,
        missing,
        source_folder: target.source_folder.clone(),
        folder: folder.to_owned(),
    };
    if let (RenameStatus::Ready, Some(new_name)) = (planned.status, planned.new_name.as_deref()) {
        let key = format!("{}/{}", target.source_id, folder);
        if !claims.knows(&key) {
            let on_disk = names_on_disk(&full_asset_path(&target.source_folder, folder));
            let in_library = database.asset_names_in_folder(target.source_id, folder)?;
            claims.add_folder(
                &key,
                on_disk.iter().chain(in_library.iter()).map(String::as_str),
            );
        }
        if !claims.claim(&key, old_name, new_name) {
            planned.status = RenameStatus::Collision;
        }
    }
    Ok(planned)
}

/// What renaming the files `asset_ids` with `template` would do, file by file, in the order
/// given. Nothing on disk changes.
pub fn preview_renames(
    database: &Database,
    asset_ids: &[i64],
    template: &Template,
) -> Result<Vec<PlannedRename>, DbError> {
    let mut claims = NameClaims::default();
    let mut planned = Vec::with_capacity(asset_ids.len());
    for asset_id in asset_ids {
        if let Some(target) = database.rename_target(*asset_id)? {
            planned.push(plan_target(database, target, template, &mut claims)?);
        }
    }
    Ok(planned)
}

/// Renames `from` to `to`, refusing to replace another file. A case-only change steps through a
/// third name, because NTFS and APFS would otherwise treat it as no change.
fn rename_file(from: &Path, to: &Path) -> Result<(), FileProblem> {
    let (Some(from_name), Some(to_name)) = (
        from.file_name().and_then(|name| name.to_str()),
        to.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(FileProblem::Missing);
    };
    match fs::symlink_metadata(from) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Err(FileProblem::Missing),
        Err(error) => return Err(FileProblem::Io(error)),
    }
    if is_case_only(from_name, to_name) {
        let folder = from.parent().unwrap_or(Path::new(""));
        if names_on_disk(folder).iter().any(|name| name == to_name) {
            return Err(FileProblem::NameTaken);
        }
        let step = from.with_file_name(format!("{to_name}{CASE_STEP_SUFFIX}"));
        if fs::symlink_metadata(&step).is_ok() {
            return Err(FileProblem::NameTaken);
        }
        fs::rename(from, &step).map_err(FileProblem::Io)?;
        if let Err(error) = fs::rename(&step, to) {
            let _ = fs::rename(&step, from);
            return Err(FileProblem::Io(error));
        }
        return Ok(());
    }
    // `fs::rename` replaces an existing file on every platform, so the name is checked first.
    if fs::symlink_metadata(to).is_ok() {
        return Err(FileProblem::NameTaken);
    }
    fs::rename(from, to).map_err(FileProblem::Io)
}

/// Renames the file and records its new path while holding the library, putting the file back
/// if the library refuses the path.
fn rename_recorded(
    database: &Mutex<Database>,
    operation_id: i64,
    planned: &PlannedRename,
    new_name: &str,
) -> Result<(), FileProblem> {
    let from = planned.full_path(&planned.old_name);
    let to = planned.full_path(new_name);
    let mut library = locked(database);
    rename_file(&from, &to)?;
    if let Err(error) = library.record_asset_rename(
        operation_id,
        planned.asset_id,
        &planned.old_path(),
        &join(&planned.folder, new_name),
    ) {
        let _ = rename_file(&to, &from);
        return Err(FileProblem::Database(error));
    }
    Ok(())
}

fn file_count(count: u64) -> String {
    match count {
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    }
}

/// Renames every ready file the preview of `asset_ids` lists, as one journaled operation.
/// `stop` is asked before each file with the count done and the total; when it returns true the
/// batch ends there and what was renamed stays one operation that can be undone.
pub fn apply_renames(
    database: &Mutex<Database>,
    asset_ids: &[i64],
    template: &Template,
    template_text: &str,
    mut stop: impl FnMut(u64, u64) -> bool,
) -> Result<RenameReport, RenameError> {
    let _operation = crate::file_operation();
    let planned = preview_renames(&locked(database), asset_ids, template)?;
    let ready: Vec<&PlannedRename> = planned
        .iter()
        .filter(|plan| plan.status == RenameStatus::Ready)
        .collect();
    let mut report = RenameReport::default();
    if ready.is_empty() {
        return Ok(report);
    }
    let total = ready.len() as u64;
    let forward = serde_json::json!({ "template": template_text, "files": total }).to_string();
    let operation_id = locked(database).begin_operation(
        OperationKind::RenameFiles,
        &format!("Renaming {}", file_count(total)),
        &forward,
        "{}",
    )?;
    report.operation_id = Some(operation_id);

    for (done, plan) in ready.into_iter().enumerate() {
        if stop(done as u64, total) {
            report.stopped = true;
            break;
        }
        let Some(new_name) = plan.new_name.as_deref() else {
            continue;
        };
        match rename_recorded(database, operation_id, plan, new_name) {
            Ok(()) => report.renamed += 1,
            Err(problem) => report.failed.push(FailedRename {
                track_id: plan.track_id,
                path: plan.full_path(&plan.old_name),
                problem,
            }),
        }
    }
    let _ = stop(total, total);

    let (state, summary) = match report.renamed {
        0 => (
            OperationState::Failed,
            format!("No files were renamed ({} failed)", report.failed.len()),
        ),
        renamed => (
            OperationState::Applied,
            format!("Renamed {}", file_count(u64::from(renamed))),
        ),
    };
    locked(database).finish_operation(operation_id, state, &summary)?;
    Ok(report)
}

/// Gives every file an applied rename moved its old name back, newest first. Nothing changes
/// unless every file is still where the rename left it.
pub fn undo_renames(
    database: &Mutex<Database>,
    operation_id: i64,
) -> Result<UndoReport, RenameError> {
    let _operation = crate::file_operation();
    let (operation, steps) = {
        let library = locked(database);
        let operation = library
            .operation(operation_id)?
            .ok_or(RenameError::NoOperation(operation_id))?;
        if operation.kind != OperationKind::RenameFiles {
            return Err(RenameError::NotARename(operation_id));
        }
        if !operation.state.can_undo() {
            return Err(RenameError::CannotUndo {
                id: operation_id,
                state: operation.state,
            });
        }
        let mut steps = Vec::new();
        for rename in library.operation_renames(operation_id)?.into_iter().rev() {
            if rename.undone {
                continue;
            }
            let target = library
                .rename_target(rename.asset_id)?
                .ok_or(RenameError::NoOperation(operation_id))?;
            let from = full_asset_path(&target.source_folder, &rename.from_path);
            let to = full_asset_path(&target.source_folder, &rename.to_path);
            if target.path != rename.to_path || fs::symlink_metadata(&to).is_err() {
                return Err(RenameError::ChangedSinceRename(to));
            }
            steps.push((rename.asset_id, to, from));
        }
        (operation, steps)
    };

    let mut restored = 0;
    for (asset_id, current, original) in steps {
        let mut library = locked(database);
        rename_file(&current, &original).map_err(|problem| RenameError::File {
            path: current.clone(),
            problem,
        })?;
        if let Err(error) = library.record_rename_undone(operation_id, asset_id) {
            let _ = rename_file(&original, &current);
            return Err(error.into());
        }
        restored += 1;
    }

    let mut library = locked(database);
    library.finish_operation(operation_id, OperationState::Undone, &operation.summary)?;
    let operation = library
        .operation(operation_id)?
        .ok_or(RenameError::NoOperation(operation_id))?;
    Ok(UndoReport {
        operation,
        restored,
    })
}
