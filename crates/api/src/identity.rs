use std::sync::PoisonError;
use std::sync::atomic::{AtomicBool, Ordering};

use resonance_library::identity::{
    AUDIO_DECODER_VERSION, IdentifyReport as CoreIdentifyReport, apply_computed_identity_batch,
    compute_batch,
};

use crate::LibraryCore;
use crate::command::IdentifyReport;
use crate::error::ApiError;

pub(crate) const IDENTIFY_BATCH_SIZE: u32 = 16;

pub(crate) fn run_identify_pass(
    core: &LibraryCore,
    shutdown: Option<&AtomicBool>,
) -> Result<CoreIdentifyReport, ApiError> {
    let mut report = CoreIdentifyReport::default();
    let mut after_asset_id = 0;

    loop {
        if shutdown.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            break;
        }
        let _pass = core
            .identify_pass
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let batch = {
            let library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
            library
                .pending_identity_assets(after_asset_id, AUDIO_DECODER_VERSION, IDENTIFY_BATCH_SIZE)
                .map_err(ApiError::library_unreadable)?
        };
        let Some(last) = batch.last() else {
            break;
        };
        after_asset_id = last.asset_id;

        let computed = compute_batch(&batch, shutdown);
        if shutdown.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Ok(report);
        }

        let batch_report = apply_computed_identity_batch(&core.library, &batch, computed)
            .map_err(ApiError::library_unreadable)?;
        report.processed += batch_report.processed;
        report.decoded += batch_report.decoded;
        report.reconciled_moves += batch_report.reconciled_moves;
        report.skipped_unreadable += batch_report.skipped_unreadable;
    }

    Ok(report)
}

pub(crate) fn to_command_report(report: CoreIdentifyReport) -> IdentifyReport {
    IdentifyReport {
        processed: report.processed,
        decoded: report.decoded,
        reconciled_moves: report.reconciled_moves,
        skipped_unreadable: report.skipped_unreadable,
    }
}
