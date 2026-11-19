use resonance_core::jobs::{JOB_KIND_RENAME_FILES, JobTier, NewJob};
use resonance_core::template::{RenameStatus, Template};
use resonance_jobs::{JobExecution, JobInterrupt, JobOutcome};
use resonance_library::rename::{
    self, PlannedRename, RenameError, apply_renames, preview_renames, undo_renames,
};
use serde::{Deserialize, Serialize};

use crate::command::{
    FailedRename, RenameFile, RenamePreview, RenameReport, RenameScope, RenameStatusName,
    UndoReport,
};
use crate::error::ApiError;
use crate::jobs::job_api_error;
use crate::staging::tag_name;
use crate::tag_writes::journal_operation;
use crate::{Api, LibraryCore, core_track_query};

/// Batches up to this many files rename while the sheet waits. Larger ones go to Activity, so
/// the whole library can be renamed with progress and a way to stop.
const RENAME_AT_ONCE: usize = 200;

#[derive(Debug, Serialize, Deserialize)]
struct RenameJobPayload {
    asset_ids: Vec<i64>,
    template: String,
}

fn status_name(status: RenameStatus) -> RenameStatusName {
    match status {
        RenameStatus::Unchanged => RenameStatusName::Unchanged,
        RenameStatus::Ready => RenameStatusName::Ready,
        RenameStatus::Collision => RenameStatusName::Collision,
        RenameStatus::MissingFields => RenameStatusName::MissingFields,
        RenameStatus::TooLong => RenameStatusName::TooLong,
        RenameStatus::FileMissing => RenameStatusName::FileMissing,
    }
}

/// Problems first, so a long preview shows what needs attention without scrolling.
fn display_order(status: RenameStatus) -> u8 {
    match status {
        RenameStatus::Collision => 0,
        RenameStatus::MissingFields => 1,
        RenameStatus::TooLong => 2,
        RenameStatus::FileMissing => 3,
        RenameStatus::Ready => 4,
        RenameStatus::Unchanged => 5,
    }
}

fn rename_file(plan: &PlannedRename) -> RenameFile {
    RenameFile {
        track_id: plan.track_id,
        status: status_name(plan.status),
        old_name: plan.old_name.clone(),
        new_name: plan.new_name.clone(),
        missing: plan.missing.iter().map(|field| tag_name(*field)).collect(),
    }
}

fn parse_template(text: &str) -> Result<Template, ApiError> {
    Template::parse(text).map_err(ApiError::invalid_input)
}

fn rename_error(error: RenameError) -> ApiError {
    match error {
        RenameError::Database(source) => ApiError::library_unreadable(source),
        other => ApiError::invalid_input(other),
    }
}

fn report_of(report: rename::RenameReport) -> RenameReport {
    RenameReport {
        operation_id: report.operation_id,
        renamed: report.renamed,
        failed: report
            .failed
            .into_iter()
            .map(|failed| FailedRename {
                track_id: failed.track_id,
                path: failed.path.display().to_string(),
                reason: failed.problem.to_string(),
            })
            .collect(),
        queued: false,
    }
}

fn file_count(count: usize) -> String {
    match count {
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    }
}

pub(crate) fn execute_rename_job(exec: &JobExecution<'_>, core: &LibraryCore) -> JobOutcome {
    let payload: RenameJobPayload = match serde_json::from_str(&exec.job().payload_json) {
        Ok(payload) => payload,
        Err(error) => return JobOutcome::Failed(ApiError::bad_request(error).message),
    };
    let template = match Template::parse(&payload.template) {
        Ok(template) => template,
        Err(error) => return JobOutcome::Failed(error.to_string()),
    };
    let mut interrupted = None;
    let result = apply_renames(
        &core.library,
        &payload.asset_ids,
        &template,
        &payload.template,
        |done, total| {
            let _ = exec.report_progress(done, Some(total));
            match exec.wait_if_paused() {
                Ok(()) => false,
                Err(interrupt) => {
                    interrupted = Some(interrupt);
                    true
                }
            }
        },
    );
    core.watch_revisions.send_modify(|revision| *revision += 1);
    match (result, interrupted) {
        (Err(error), _) => JobOutcome::Failed(rename_error(error).message),
        (Ok(_), Some(JobInterrupt::Cancelled)) => JobOutcome::Cancelled,
        (Ok(_), Some(JobInterrupt::Shutdown)) => JobOutcome::Interrupted,
        (Ok(report), None) => {
            let done = u64::from(report.renamed);
            JobOutcome::Completed {
                done_items: done,
                total_items: Some(done + report.failed.len() as u64),
            }
        }
    }
}

impl Api {
    fn rename_assets(&self, scope: RenameScope) -> Result<Vec<i64>, ApiError> {
        let library = self.locked_library();
        match scope {
            RenameScope::Tracks { track_ids } => library.primary_asset_ids(&track_ids),
            RenameScope::Query { query } => {
                library.query_asset_ids(&core_track_query(Some(query))?)
            }
        }
        .map_err(ApiError::library_unreadable)
    }

    pub(crate) fn preview_rename(
        &self,
        scope: RenameScope,
        template: &str,
        limit: u32,
    ) -> Result<RenamePreview, ApiError> {
        let template = parse_template(template)?;
        let assets = self.rename_assets(scope)?;
        let mut planned = preview_renames(&self.locked_library(), &assets, &template)
            .map_err(ApiError::library_unreadable)?;
        let count = |status| planned.iter().filter(|plan| plan.status == status).count() as u32;
        let mut preview = RenamePreview {
            total: planned.len() as u32,
            unchanged: count(RenameStatus::Unchanged),
            ready: count(RenameStatus::Ready),
            collision: count(RenameStatus::Collision),
            missing_fields: count(RenameStatus::MissingFields),
            too_long: count(RenameStatus::TooLong),
            file_missing: count(RenameStatus::FileMissing),
            files: Vec::new(),
        };
        planned.sort_by_key(|plan| display_order(plan.status));
        preview.files = planned
            .iter()
            .take(limit as usize)
            .map(rename_file)
            .collect();
        Ok(preview)
    }

    pub(crate) fn apply_rename(
        &self,
        scope: RenameScope,
        template_text: &str,
    ) -> Result<RenameReport, ApiError> {
        let template = parse_template(template_text)?;
        let assets = self.rename_assets(scope)?;
        if assets.len() > RENAME_AT_ONCE {
            let payload_json = serde_json::to_string(&RenameJobPayload {
                asset_ids: assets.clone(),
                template: template_text.to_owned(),
            })
            .map_err(ApiError::internal)?;
            self.jobs
                .enqueue(&NewJob {
                    kind: JOB_KIND_RENAME_FILES.to_owned(),
                    tier: JobTier::BackgroundCpu,
                    label: format!("Rename {}", file_count(assets.len())),
                    payload_json,
                    total_items: Some(assets.len() as u64),
                })
                .map_err(job_api_error)?;
            return Ok(RenameReport {
                operation_id: None,
                renamed: 0,
                failed: Vec::new(),
                queued: true,
            });
        }
        let report = apply_renames(
            &self.core.library,
            &assets,
            &template,
            template_text,
            |_, _| false,
        )
        .map_err(rename_error)?;
        if report.renamed > 0 {
            self.library_edited();
        }
        Ok(report_of(report))
    }

    pub(crate) fn undo_rename(&self, operation_id: i64) -> Result<UndoReport, ApiError> {
        let rename::UndoReport {
            operation,
            restored,
        } = undo_renames(&self.core.library, operation_id).map_err(rename_error)?;
        if restored > 0 {
            self.library_edited();
        }
        Ok(UndoReport {
            operation: journal_operation(operation),
            restored,
        })
    }
}
