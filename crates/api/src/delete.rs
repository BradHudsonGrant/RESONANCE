//! Deleting tracks to the bin, deleting them for good, and undoing a delete.

use resonance_library::delete::{DeleteMode, delete_tracks, undo_delete};
use resonance_library::journal::JournalError;

use crate::Api;
use crate::command::{DeleteReport, UndoReport};
use crate::error::ApiError;
use crate::tag_writes::journal_operation;

fn journal_error(error: JournalError) -> ApiError {
    match error {
        JournalError::Database(source) => ApiError::library_unreadable(source),
        other => ApiError::invalid_input(other),
    }
}

impl Api {
    pub(crate) fn delete_tracks(
        &self,
        track_ids: &[i64],
        for_good: bool,
    ) -> Result<DeleteReport, ApiError> {
        let mode = if for_good {
            DeleteMode::ForGood
        } else {
            DeleteMode::ToBin
        };
        let outcome = delete_tracks(&mut self.locked_library(), track_ids, mode);
        match outcome {
            Ok(operation) => {
                self.library_edited();
                Ok(DeleteReport {
                    operation: Some(journal_operation(operation)),
                    no_bin: None,
                })
            }
            Err(JournalError::RolledBack { cause, .. })
                if matches!(*cause, JournalError::NoBin { .. }) =>
            {
                Ok(DeleteReport {
                    operation: None,
                    no_bin: Some(cause.to_string()),
                })
            }
            Err(error) => Err(journal_error(error)),
        }
    }

    pub(crate) fn undo_delete(&self, operation_id: i64) -> Result<UndoReport, ApiError> {
        let (operation, restored) =
            undo_delete(&mut self.locked_library(), operation_id).map_err(journal_error)?;
        self.library_edited();
        Ok(UndoReport {
            operation: journal_operation(operation),
            restored,
        })
    }
}
