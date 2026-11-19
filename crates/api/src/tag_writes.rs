use resonance_core::journal::{OperationKind, OperationState};
use resonance_db::StoredOperation;
use resonance_library::write_tags::{
    self, PlannedFile, TagWriteError, apply_tag_writes, preview_tag_writes, undo_operation,
};

use crate::Api;
use crate::command::{
    FailedTagWrite, FieldChange, FileTagWrite, JournalOperation, OperationKindName,
    OperationStateName, TagWriteReport, UndoReport,
};
use crate::error::ApiError;
use crate::staging::tag_name;

const MAX_RECENT_OPERATIONS: u32 = 200;

fn ms(ns: i64) -> i64 {
    ns / 1_000_000
}

pub(crate) fn journal_operation(operation: StoredOperation) -> JournalOperation {
    JournalOperation {
        id: operation.id,
        kind: match operation.kind {
            OperationKind::WriteTags => OperationKindName::WriteTags,
            OperationKind::RenameFiles => OperationKindName::RenameFiles,
        },
        state: match operation.state {
            OperationState::Pending => OperationStateName::Pending,
            OperationState::Applied => OperationStateName::Applied,
            OperationState::Failed => OperationStateName::Failed,
            OperationState::Undone => OperationStateName::Undone,
        },
        can_undo: operation.state.can_undo(),
        summary: operation.summary,
        created_ms: ms(operation.created_ns),
        applied_ms: operation.applied_ns.map(ms),
        undone_ms: operation.undone_ns.map(ms),
    }
}

fn file_tag_write(file: PlannedFile) -> FileTagWrite {
    FileTagWrite {
        track_id: file.target.track_id,
        path: file.full_path.display().to_string(),
        fields: file
            .writes
            .into_iter()
            .map(|write| FieldChange {
                field: tag_name(write.field),
                from: write.from,
                to: write.to,
            })
            .collect(),
        blocked: file.blocked.map(|problem| problem.to_string()),
    }
}

fn api_error(error: TagWriteError) -> ApiError {
    match error {
        TagWriteError::Database(source) => ApiError::library_unreadable(source),
        TagWriteError::Journal(_) => ApiError::internal(error),
        _ => ApiError::invalid_input(error),
    }
}

impl Api {
    pub(crate) fn preview_tag_writes(
        &self,
        track_ids: &[i64],
    ) -> Result<Vec<FileTagWrite>, ApiError> {
        let planned = preview_tag_writes(&self.locked_library(), track_ids)
            .map_err(ApiError::library_unreadable)?;
        Ok(planned.into_iter().map(file_tag_write).collect())
    }

    pub(crate) fn apply_tag_writes(&self, track_ids: &[i64]) -> Result<TagWriteReport, ApiError> {
        let report = apply_tag_writes(&self.core.library, track_ids).map_err(api_error)?;
        if report.written > 0 {
            self.library_edited();
        }
        Ok(TagWriteReport {
            operation_id: report.operation_id,
            written: report.written,
            failed: report
                .failed
                .into_iter()
                .map(|failed| FailedTagWrite {
                    track_id: failed.track_id,
                    path: failed.path.display().to_string(),
                    reason: failed.problem.to_string(),
                })
                .collect(),
        })
    }

    pub(crate) fn recent_operations(&self, limit: u32) -> Result<Vec<JournalOperation>, ApiError> {
        let operations = self
            .locked_library()
            .recent_operations(limit.min(MAX_RECENT_OPERATIONS))
            .map_err(ApiError::library_unreadable)?;
        Ok(operations.into_iter().map(journal_operation).collect())
    }

    pub(crate) fn undo_operation(&self, operation_id: i64) -> Result<UndoReport, ApiError> {
        let kind = self
            .locked_library()
            .operation(operation_id)
            .map_err(ApiError::library_unreadable)?
            .map(|operation| operation.kind);
        if kind == Some(OperationKind::RenameFiles) {
            return self.undo_rename(operation_id);
        }
        let write_tags::UndoReport {
            operation,
            restored,
        } = undo_operation(&self.core.library, operation_id).map_err(api_error)?;
        if restored > 0 {
            self.library_edited();
        }
        Ok(UndoReport {
            operation: journal_operation(operation),
            restored,
        })
    }
}
