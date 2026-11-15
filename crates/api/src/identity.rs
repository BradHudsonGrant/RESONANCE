use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread::{self, JoinHandle};

use resonance_library::identity::{
    AUDIO_DECODER_VERSION, IdentifyReport as CoreIdentifyReport, apply_computed_identity,
    compute_batch,
};

use crate::LibraryCore;
use crate::command::IdentifyReport;
use crate::error::ApiError;

const BATCH_SIZE: u32 = 16;

enum Signal {
    Wake,
    Shutdown,
}

/// Computes `byte_hash`, `audio_hash` and Chromaprint fingerprints in the background without
/// holding `LibraryCore::library` while decoding audio.
pub(crate) struct IdentityWorker {
    tx: Sender<Signal>,
    shutdown: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl IdentityWorker {
    pub(crate) fn new(core: Weak<LibraryCore>) -> Self {
        let (tx, rx) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker_shutdown = Arc::clone(&shutdown);
        let handle = thread::Builder::new()
            .name("identity worker".to_owned())
            .spawn(move || run_worker(rx, core, worker_shutdown))
            .ok();
        Self {
            tx,
            shutdown,
            worker: Mutex::new(handle),
        }
    }

    pub(crate) fn wake(&self) {
        let _ = self.tx.send(Signal::Wake);
    }
}

impl Drop for IdentityWorker {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Signal::Shutdown);
        if let Some(handle) = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = handle.join();
        }
    }
}

fn run_worker(rx: Receiver<Signal>, core: Weak<LibraryCore>, shutdown: Arc<AtomicBool>) {
    while let Ok(signal) = rx.recv() {
        if matches!(signal, Signal::Shutdown) || shutdown.load(Ordering::Relaxed) {
            return;
        }
        while let Ok(queued) = rx.try_recv() {
            if matches!(queued, Signal::Shutdown) {
                return;
            }
        }
        let Some(core) = core.upgrade() else {
            return;
        };
        if let Ok(report) = run_identify_pass(&core, Some(&shutdown))
            && report.reconciled_moves > 0
        {
            core.watch_revisions.send_modify(|revision| *revision += 1);
        }
    }
}

pub(crate) fn run_identify_pass(
    core: &LibraryCore,
    shutdown: Option<&AtomicBool>,
) -> Result<CoreIdentifyReport, ApiError> {
    let _pass = core
        .identify_pass
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut report = CoreIdentifyReport::default();
    let mut after_asset_id = 0;

    loop {
        if shutdown.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            break;
        }
        let batch = {
            let library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
            library
                .pending_identity_assets(after_asset_id, AUDIO_DECODER_VERSION, BATCH_SIZE)
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

        let mut library = core.library.lock().unwrap_or_else(PoisonError::into_inner);
        for (pending, maybe_identity) in batch.into_iter().zip(computed) {
            let Some(identity) = maybe_identity else {
                report.skipped_unreadable += 1;
                continue;
            };
            report.processed += 1;
            if identity.audio.is_some() {
                report.decoded += 1;
            }
            if apply_computed_identity(&mut library, &pending, &identity)
                .map_err(ApiError::library_unreadable)?
            {
                report.reconciled_moves += 1;
            }
        }
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
