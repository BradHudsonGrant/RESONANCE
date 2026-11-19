//! The operation journal. The steps are stored as JSON the crate that runs them writes, so this
//! module never needs to know what a step is.

use resonance_core::journal::{OperationKind, OperationState};
use rusqlite::{Row, params};

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
}

const COLUMNS: &str = "id, kind, state, summary, forward_json, inverse_json, created_ns, \
                       applied_ns, undone_ns";

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
    })
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
