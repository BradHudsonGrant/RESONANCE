//! The operation journal. The steps are stored as JSON the crate that runs them writes, so this
//! module never needs to know what a step is.

use resonance_core::journal::{OperationKind, OperationState, StepState};
use resonance_core::library::is_asset_path_included;
use rusqlite::{OptionalExtension, Row, params};

use crate::search::now_unix_ns;
use crate::{Database, DbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredOperation {
    pub id: i64,
    pub kind: OperationKind,
    pub state: OperationState,
    pub summary: String,
    pub forward_json: String,
    pub inverse_json: String,
    pub created_ns: i64,
    pub applied_ns: Option<i64>,
    pub undone_ns: Option<i64>,
    /// The step being worked on while the operation is executing.
    pub current_step: Option<u32>,
    pub recovered_ns: Option<i64>,
    /// What happened when the operation was recovered after the app stopped in the middle.
    pub recovery: Option<String>,
}

/// One step of an operation's plan, as the crate that runs it wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredStep {
    pub position: u32,
    pub state: StepState,
    pub forward_json: String,
    pub inverse_json: String,
}

/// A file as deleting it needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFile {
    pub asset_id: i64,
    pub track_id: i64,
    pub source_folder: String,
    pub path: String,
    pub size_bytes: u64,
    pub missing: bool,
    pub trashed: bool,
}

const COLUMNS: &str = "id, kind, state, summary, forward_json, inverse_json, created_ns, \
                       applied_ns, undone_ns, current_step, recovered_ns, recovery";

fn operation_from(row: &Row<'_>) -> Result<StoredOperation, DbError> {
    let kind: String = row.get(1)?;
    let state: String = row.get(2)?;
    Ok(StoredOperation {
        id: row.get(0)?,
        kind: OperationKind::from_name(&kind).ok_or(DbError::UnknownName {
            column: "operation.kind",
            name: kind,
        })?,
        state: OperationState::from_name(&state).ok_or(DbError::UnknownName {
            column: "operation.state",
            name: state,
        })?,
        summary: row.get(3)?,
        forward_json: row.get(4)?,
        inverse_json: row.get(5)?,
        created_ns: row.get(6)?,
        applied_ns: row.get(7)?,
        undone_ns: row.get(8)?,
        current_step: row.get(9)?,
        recovered_ns: row.get(10)?,
        recovery: row.get(11)?,
    })
}

fn step_from(row: &Row<'_>) -> Result<StoredStep, DbError> {
    let state: String = row.get(1)?;
    Ok(StoredStep {
        position: row.get(0)?,
        state: StepState::from_name(&state).ok_or(DbError::UnknownName {
            column: "operation_step.state",
            name: state,
        })?,
        forward_json: row.get(2)?,
        inverse_json: row.get(3)?,
    })
}

fn set_step_state(
    connection: &rusqlite::Connection,
    operation_id: i64,
    position: u32,
    state: StepState,
) -> Result<(), DbError> {
    connection
        .prepare_cached(
            "UPDATE operation_step SET state = ?3 WHERE operation_id = ?1 AND position = ?2",
        )?
        .execute(params![operation_id, position, state.name()])?;
    Ok(())
}

impl Database {
    /// Journals an operation before it touches any file. Returns its id.
    pub fn begin_operation(
        &mut self,
        kind: OperationKind,
        summary: &str,
        forward_json: &str,
        inverse_json: &str,
    ) -> Result<i64, DbError> {
        self.connection.execute(
            "INSERT INTO operation (kind, state, summary, forward_json, inverse_json, created_ns)
             VALUES (?1, 'pending', ?2, ?3, ?4, ?5)",
            params![
                kind.name(),
                summary,
                forward_json,
                inverse_json,
                now_unix_ns()
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    /// Records what it takes to undo the files changed so far.
    pub fn set_operation_inverse(&mut self, id: i64, inverse_json: &str) -> Result<(), DbError> {
        self.connection.execute(
            "UPDATE operation SET inverse_json = ?2 WHERE id = ?1",
            params![id, inverse_json],
        )?;
        Ok(())
    }

    /// Moves the operation to `state`, stamping when it was applied or undone.
    pub fn finish_operation(
        &mut self,
        id: i64,
        state: OperationState,
        summary: &str,
    ) -> Result<(), DbError> {
        let now = now_unix_ns();
        self.connection.execute(
            "UPDATE operation SET state = ?2, summary = ?3,
                 applied_ns = CASE WHEN ?2 = 'applied' THEN ?4 ELSE applied_ns END,
                 undone_ns = CASE WHEN ?2 = 'undone' THEN ?4 ELSE undone_ns END
             WHERE id = ?1",
            params![id, state.name(), summary, now],
        )?;
        Ok(())
    }

    /// Journals an operation and its whole plan, each step with what puts it back, in one
    /// transaction before any file is touched. Returns its id.
    pub fn begin_planned_operation(
        &mut self,
        kind: OperationKind,
        summary: &str,
        forward_json: &str,
        steps: &[(String, String)],
    ) -> Result<i64, DbError> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO operation (kind, state, summary, forward_json, inverse_json, created_ns)
             VALUES (?1, 'pending', ?2, ?3, '{}', ?4)",
            params![kind.name(), summary, forward_json, now_unix_ns()],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut add = tx.prepare(
                "INSERT INTO operation_step (operation_id, position, state, forward_json,
                     inverse_json)
                 VALUES (?1, ?2, 'planned', ?3, ?4)",
            )?;
            for (position, (forward, inverse)) in steps.iter().enumerate() {
                add.execute(params![id, position as i64, forward, inverse])?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    /// Records that step `position` is about to change something.
    pub fn start_step(&mut self, operation_id: i64, position: u32) -> Result<(), DbError> {
        self.connection.execute(
            "UPDATE operation SET state = 'executing', current_step = ?2 WHERE id = ?1",
            params![operation_id, position],
        )?;
        Ok(())
    }

    /// Records where a step stands once its change on disk has happened or been put back.
    pub fn set_step(
        &mut self,
        operation_id: i64,
        position: u32,
        state: StepState,
    ) -> Result<(), DbError> {
        set_step_state(&self.connection, operation_id, position, state)
    }

    pub fn operation_steps(&self, operation_id: i64) -> Result<Vec<StoredStep>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT position, state, forward_json, inverse_json FROM operation_step
             WHERE operation_id = ?1 ORDER BY position",
        )?;
        let mut rows = statement.query([operation_id])?;
        let mut steps = Vec::new();
        while let Some(row) = rows.next()? {
            steps.push(step_from(row)?);
        }
        Ok(steps)
    }

    /// Operations the app stopped in the middle of, oldest first.
    pub fn unfinished_operations(&self) -> Result<Vec<StoredOperation>, DbError> {
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM operation WHERE state IN ('pending', 'executing') ORDER BY id"
        ))?;
        let mut rows = statement.query([])?;
        let mut operations = Vec::new();
        while let Some(row) = rows.next()? {
            operations.push(operation_from(row)?);
        }
        Ok(operations)
    }

    /// Records how an unfinished operation was brought to rest and the state it now has.
    pub fn record_recovery(
        &mut self,
        id: i64,
        state: OperationState,
        recovery: &str,
    ) -> Result<(), DbError> {
        let now = now_unix_ns();
        self.connection.execute(
            "UPDATE operation SET state = ?2, recovery = ?3, recovered_ns = ?4, current_step = NULL,
                 applied_ns = CASE WHEN ?2 = 'applied' THEN ?4 ELSE applied_ns END
             WHERE id = ?1",
            params![id, state.name(), recovery, now],
        )?;
        Ok(())
    }

    /// Notes a problem met while recovering an operation. It stays unfinished, so the next
    /// start tries again.
    pub fn note_recovery_problem(&mut self, id: i64, recovery: &str) -> Result<(), DbError> {
        self.connection.execute(
            "UPDATE operation SET recovery = ?2, recovered_ns = ?3 WHERE id = ?1",
            params![id, recovery, now_unix_ns()],
        )?;
        Ok(())
    }

    pub fn asset_file(&self, asset_id: i64) -> Result<Option<AssetFile>, DbError> {
        Ok(self
            .connection
            .prepare_cached(
                "SELECT asset.id, asset.track_id, source.folder, asset.path, asset.size_bytes,
                     asset.missing_since IS NOT NULL, asset.trashed_by IS NOT NULL
                 FROM asset JOIN source ON source.id = asset.source_id
                 WHERE asset.id = ?1",
            )?
            .query_row([asset_id], |row| {
                Ok(AssetFile {
                    asset_id: row.get(0)?,
                    track_id: row.get(1)?,
                    source_folder: row.get(2)?,
                    path: row.get(3)?,
                    size_bytes: row.get(4)?,
                    missing: row.get(5)?,
                    trashed: row.get(6)?,
                })
            })
            .optional()?)
    }

    /// The files in the library of each track, in track order.
    pub fn included_assets_of_tracks(&self, track_ids: &[i64]) -> Result<Vec<i64>, DbError> {
        let mut statement = self.connection.prepare_cached(
            "SELECT id FROM asset WHERE track_id = ?1 AND included = 1
             ORDER BY is_primary DESC, id",
        )?;
        let mut assets = Vec::new();
        for track_id in track_ids {
            for asset in statement.query_map([track_id], |row| row.get(0))? {
                assets.push(asset?);
            }
        }
        Ok(assets)
    }

    /// Leaves a file out of the library because operation `operation_id` is moving it to the
    /// bin, and records step `position` done in the same transaction.
    pub fn hide_trashed_asset(
        &mut self,
        operation_id: i64,
        position: u32,
        asset_id: i64,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        tx.execute(
            "UPDATE asset SET included = 0, trashed_by = ?2 WHERE id = ?1",
            params![asset_id, operation_id],
        )?;
        set_step_state(&tx, operation_id, position, StepState::Done)?;
        tx.commit()?;
        Ok(())
    }

    /// Brings a file hidden by `hide_trashed_asset` back into the library under its source's
    /// folder rules, and records step `position` undone in the same transaction.
    pub fn show_trashed_asset(
        &mut self,
        operation_id: i64,
        position: u32,
        asset_id: i64,
    ) -> Result<(), DbError> {
        let tx = self.connection.transaction()?;
        let found: Option<(i64, String)> = tx
            .query_row(
                "SELECT source_id, path FROM asset WHERE id = ?1 AND trashed_by = ?2",
                params![asset_id, operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((source, path)) = found {
            let (include_subfolders, excluded, ignored) =
                crate::sources::source_inclusion_in_tx(&tx, crate::SourceId(source))?;
            let included = is_asset_path_included(&path, include_subfolders, &excluded, &ignored);
            tx.execute(
                "UPDATE asset SET included = ?2, trashed_by = NULL WHERE id = ?1",
                params![asset_id, included],
            )?;
        }
        set_step_state(&tx, operation_id, position, StepState::Undone)?;
        tx.commit()?;
        Ok(())
    }

    pub fn operation(&self, id: i64) -> Result<Option<StoredOperation>, DbError> {
        let mut statement = self
            .connection
            .prepare_cached(&format!("SELECT {COLUMNS} FROM operation WHERE id = ?1"))?;
        let mut rows = statement.query([id])?;
        rows.next()?.map(operation_from).transpose()
    }

    /// The newest operations first.
    pub fn recent_operations(&self, limit: u32) -> Result<Vec<StoredOperation>, DbError> {
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT {COLUMNS} FROM operation ORDER BY created_ns DESC, id DESC LIMIT ?1"
        ))?;
        let mut rows = statement.query([limit])?;
        let mut operations = Vec::new();
        while let Some(row) = rows.next()? {
            operations.push(operation_from(row)?);
        }
        Ok(operations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_operation_moves_from_pending_to_applied_to_undone() -> Result<(), DbError> {
        let mut database = Database::open_in_memory()?;
        let id = database.begin_operation(OperationKind::WriteTags, "Tags", "{}", "[]")?;
        assert_eq!(
            database.operation(id)?.map(|operation| operation.state),
            Some(OperationState::Pending)
        );

        database.set_operation_inverse(id, "[1]")?;
        database.finish_operation(id, OperationState::Applied, "Wrote tags to 1 file")?;
        let applied = database.operation(id)?.ok_or(DbError::UnknownName {
            column: "operation.id",
            name: id.to_string(),
        })?;
        assert_eq!(applied.state, OperationState::Applied);
        assert_eq!(applied.inverse_json, "[1]");
        assert_eq!(applied.summary, "Wrote tags to 1 file");
        assert!(applied.applied_ns.is_some());
        assert_eq!(applied.undone_ns, None);

        database.finish_operation(id, OperationState::Undone, "Wrote tags to 1 file")?;
        let recent = database.recent_operations(10)?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].state, OperationState::Undone);
        assert!(recent[0].undone_ns.is_some());
        assert_eq!(recent[0].applied_ns, applied.applied_ns);
        Ok(())
    }

    #[test]
    fn an_unknown_state_is_refused_by_the_schema() -> Result<(), DbError> {
        let database = Database::open_in_memory()?;
        let refused = database.connection.execute(
            "INSERT INTO operation (kind, state, summary, forward_json, inverse_json, created_ns)
             VALUES ('write_tags', 'done', '', '{}', '[]', 0)",
            [],
        );
        assert!(refused.is_err());
        Ok(())
    }
}
