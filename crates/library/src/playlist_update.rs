//! An update to another app's playlist: each file it rewrote whole, with the backup taken
//! before, and the path changes it cleared. The plan is journaled after the backups are checked
//! and before the first file is written, so undo and recovery need only the disk and the hashes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use resonance_core::journal::{
    OperationKind, OperationState, Recovery, Rewrite, can_undo_rewrite, rewrite_recovery,
};
use resonance_db::{ClearedExternalChange, Database, StoredOperation};
use serde::{Deserialize, Serialize};

use crate::journal::{JournalError, byte_hash_hex};

/// One file the update rewrote, by the BLAKE3 hashes, in hex, of its bytes before and after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewrittenFile {
    pub path: PathBuf,
    pub backup: PathBuf,
    pub before_hash: String,
    pub after_hash: String,
}

/// A path change the update wrote, by its row in the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClearedChange {
    pub id: i64,
    pub library_id: i64,
    pub track_id: i64,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistUpdate {
    pub files: Vec<RewrittenFile>,
    pub cleared: Vec<ClearedChange>,
}

/// The BLAKE3 hash of `bytes`, in hex.
pub fn byte_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

pub fn summary(playlists: usize) -> String {
    if playlists == 1 {
        "Updated 1 playlist".to_owned()
    } else {
        format!("Updated {playlists} playlists")
    }
}

/// Journals the update as pending before any file is written. Returns its id.
pub fn begin(database: &mut Database, update: &PlaylistUpdate) -> Result<i64, JournalError> {
    Ok(database.begin_operation(
        OperationKind::UpdatePlaylist,
        &summary(update.files.len()),
        &serde_json::to_string(update)?,
        "{}",
    )?)
}

/// Clears the changes the update wrote and records it applied.
pub fn finish(
    database: &mut Database,
    operation_id: i64,
    update: &PlaylistUpdate,
) -> Result<(), JournalError> {
    let ids: Vec<i64> = update.cleared.iter().map(|change| change.id).collect();
    Ok(database.finish_playlist_update(operation_id, &ids, &summary(update.files.len()))?)
}

/// After a write that failed, puts back every file not as before and records the update rolled
/// back. The changes stay queued.
pub fn roll_back(
    database: &mut Database,
    operation_id: i64,
    update: &PlaylistUpdate,
) -> Result<(), JournalError> {
    let _operation = crate::file_operation();
    let outcome = put_back(update, &rewrite_recovery(&states(update)));
    let (state, note) = match &outcome {
        Ok(()) => (
            OperationState::RolledBack,
            "Stopped and put back from the backup.".to_owned(),
        ),
        Err(problem) => (
            OperationState::Failed,
            format!("Stopped, and putting it back failed: {problem}"),
        ),
    };
    database.finish_operation(operation_id, state, &note)?;
    outcome
}

fn plan_of(operation: &StoredOperation) -> Result<PlaylistUpdate, JournalError> {
    Ok(serde_json::from_str(&operation.forward_json)?)
}

fn states(update: &PlaylistUpdate) -> Vec<Rewrite> {
    update
        .files
        .iter()
        .map(|file| {
            Rewrite::of(
                byte_hash_hex(&file.path).ok().as_deref(),
                &file.before_hash,
                &file.after_hash,
            )
        })
        .collect()
}

fn staged_beside(path: &Path) -> PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".resonance-tmp");
    PathBuf::from(staged)
}

/// Writes the backup's bytes beside the file and moves them over it, then reads the file back.
fn restore_backup(file: &RewrittenFile) -> Result<(), JournalError> {
    let bytes = fs::read(&file.backup).map_err(|source| JournalError::File {
        path: file.backup.clone(),
        source,
    })?;
    if byte_hash(&bytes) != file.before_hash {
        return Err(JournalError::CopyDiffers(file.backup.clone()));
    }
    let staged = staged_beside(&file.path);
    let written = (|| {
        let mut out = fs::File::create(&staged)?;
        io::Write::write_all(&mut out, &bytes)?;
        out.sync_all()?;
        drop(out);
        fs::rename(&staged, &file.path)
    })();
    if let Err(source) = written {
        let _ = fs::remove_file(&staged);
        return Err(JournalError::File {
            path: file.path.clone(),
            source,
        });
    }
    match byte_hash_hex(&file.path) {
        Ok(hash) if hash == file.before_hash => Ok(()),
        Ok(_) => Err(JournalError::CopyDiffers(file.path.clone())),
        Err(source) => Err(JournalError::File {
            path: file.path.clone(),
            source,
        }),
    }
}

fn put_back(update: &PlaylistUpdate, decision: &Recovery) -> Result<(), JournalError> {
    if let Recovery::RollBack(positions) = decision {
        for position in positions {
            restore_backup(&update.files[*position])?;
        }
    }
    for file in &update.files {
        let staged = staged_beside(&file.path);
        if staged.exists() {
            fs::remove_file(&staged).map_err(|source| JournalError::File {
                path: staged,
                source,
            })?;
        }
    }
    Ok(())
}

/// Puts each playlist back from its backup and queues its changes again. Refused when another
/// app changed a playlist since the update wrote it. A file already back is skipped, so an undo
/// that stopped part way can run again.
pub fn undo(database: &mut Database, operation_id: i64) -> Result<StoredOperation, JournalError> {
    let _operation = crate::file_operation();
    let operation = database
        .operation(operation_id)?
        .ok_or(JournalError::NoOperation(operation_id))?;
    if operation.kind != OperationKind::UpdatePlaylist || !operation.state.can_undo() {
        return Err(JournalError::CannotUndo {
            id: operation_id,
            state: operation.state,
        });
    }
    let update = plan_of(&operation)?;
    let states = states(&update);
    if !can_undo_rewrite(&states) {
        let changed = states
            .iter()
            .position(|state| *state == Rewrite::ChangedSince)
            .map_or_else(PathBuf::new, |position| update.files[position].path.clone());
        return Err(JournalError::ChangedSinceWrite(changed));
    }
    for (file, state) in update.files.iter().zip(&states) {
        if *state == Rewrite::AsWritten {
            restore_backup(file)?;
        }
    }
    let changes: Vec<ClearedExternalChange> = update
        .cleared
        .iter()
        .map(|change| ClearedExternalChange {
            library_id: change.library_id,
            track_id: change.track_id,
            old_path: change.old_path.clone(),
            new_path: change.new_path.clone(),
        })
        .collect();
    database.undo_playlist_update(operation_id, &changes, &operation.summary)?;
    database
        .operation(operation_id)?
        .ok_or(JournalError::NoOperation(operation_id))
}

/// Brings an update the app stopped in the middle of to rest. When every playlist holds what
/// was written, its changes are cleared and it stands. Otherwise each playlist goes back to its
/// backup and the changes stay queued.
pub(crate) fn recover(
    database: &mut Database,
    operation: &StoredOperation,
) -> Result<(OperationState, String), JournalError> {
    let update = plan_of(operation)?;
    let decision = rewrite_recovery(&states(&update));
    put_back(&update, &decision)?;
    if decision == Recovery::Finish {
        let ids: Vec<i64> = update.cleared.iter().map(|change| change.id).collect();
        database.clear_external_changes(&ids)?;
        Ok((
            OperationState::Applied,
            "Every playlist had been written before the app stopped, so it was finished."
                .to_owned(),
        ))
    } else {
        Ok((
            OperationState::RolledBack,
            "The app stopped part way, so the playlist was put back from its backup.".to_owned(),
        ))
    }
}
