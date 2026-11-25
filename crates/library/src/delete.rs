//! Deleting tracks' files as one journaled operation: to the bin, where undo can fetch them
//! back, or for good where there is no bin and the user has confirmed.

use std::fs;

use resonance_core::journal::{OperationKind, StepState};
use resonance_db::{Database, StoredOperation};

use crate::identity::full_asset_path;
use crate::journal::{self, JournalError, Step};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    ToBin,
    /// Only after the user has confirmed, because there is no bin to take the files.
    ForGood,
}

fn file_count(count: usize) -> String {
    match count {
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    }
}

/// The steps that take every file of `track_ids` out of the library and off the disk. Each file
/// leaves the library before it leaves the disk, so a scan never meets a library file that is
/// gone. A file already missing only leaves the library.
pub fn plan_delete(
    database: &Database,
    track_ids: &[i64],
    mode: DeleteMode,
) -> Result<Vec<Step>, JournalError> {
    let mut steps = Vec::new();
    for asset_id in database.included_assets_of_tracks(track_ids)? {
        let Some(asset) = database.asset_file(asset_id)? else {
            continue;
        };
        if asset.trashed {
            continue;
        }
        steps.push(Step::HideAsset { asset_id });
        let path = full_asset_path(&asset.source_folder, &asset.path);
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let size_bytes = metadata.len();
        steps.push(match mode {
            DeleteMode::ToBin => Step::TrashFile { path, size_bytes },
            DeleteMode::ForGood => Step::RemoveFile { path, size_bytes },
        });
    }
    Ok(steps)
}

/// Deletes every file of `track_ids` as one operation. When one file cannot go, every file
/// done so far is put back and nothing changes.
pub fn delete_tracks(
    database: &mut Database,
    track_ids: &[i64],
    mode: DeleteMode,
) -> Result<StoredOperation, JournalError> {
    let steps = plan_delete(database, track_ids, mode)?;
    let files = steps
        .iter()
        .filter(|step| matches!(step, Step::HideAsset { .. }))
        .count();
    if files == 0 {
        return Err(JournalError::NothingToDelete);
    }
    let (kind, summary) = match mode {
        DeleteMode::ToBin => (
            OperationKind::DeleteFiles,
            format!("Moved {} to the bin", file_count(files)),
        ),
        DeleteMode::ForGood => (
            OperationKind::DeleteFilesForGood,
            format!("Deleted {} for good", file_count(files)),
        ),
    };
    let forward = serde_json::json!({ "tracks": track_ids, "files": files }).to_string();
    journal::apply(database, kind, &summary, &forward, &steps)
}

/// Undoes a delete, fetching each file back from the bin. Returns the operation and how many
/// files came back into the library.
pub fn undo_delete(
    database: &mut Database,
    operation_id: i64,
) -> Result<(StoredOperation, u32), JournalError> {
    let mut files = 0;
    for stored in database.operation_steps(operation_id)? {
        if matches!(
            serde_json::from_str(&stored.forward_json)?,
            Step::HideAsset { .. }
        ) && stored.state == StepState::Done
        {
            files += 1;
        }
    }
    let operation = journal::undo(database, operation_id)?;
    Ok((operation, files))
}
