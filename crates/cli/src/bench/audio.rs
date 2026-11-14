use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use resonance_engine::{DeckId, Device, EngineError, EngineTelemetry, Mix};

/// Both decks playing through the default output device, counted by the audio callback.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioRun {
    pub device: Device,
    pub buffer_frames: u32,
    pub seconds: f64,
    pub callbacks: u64,
    pub underruns: u64,
    pub missing_frames: u64,
    pub late_callbacks: u64,
    pub starved_callbacks: u64,
    pub stream_errors: u64,
    pub fewest_callback_frames: u32,
    pub most_callback_frames: u32,
    pub longest_callback_ms: f64,
}

impl AudioRun {
    /// Times the output could have gone silent. A late callback only shows the device was slow
    /// to ask, so it is counted on its own.
    pub fn dropouts(&self) -> u64 {
        self.underruns + self.starved_callbacks + self.stream_errors
    }
}

pub fn play(tracks: &[PathBuf; 2], seconds: u32) -> Result<AudioRun, EngineError> {
    let mix = Mix::new();
    let mut device = None;
    for (deck, track) in DeckId::ALL.into_iter().zip(tracks) {
        device = mix.load(deck, track)?.device;
    }
    let device = device.ok_or(EngineError::OutputGone)?;
    // The decode workers fill each deck's buffer before it plays, as they do when a DJ loads a
    // track and starts it a moment later.
    thread::sleep(Duration::from_secs(1));
    let before = reading(&mix)?;
    for deck in DeckId::ALL {
        mix.play(deck, true)?;
    }

    let started = Instant::now();
    let length = Duration::from_secs(u64::from(seconds));
    let mut last = before;
    let mut fewest = u32::MAX;
    let mut most = 0;
    let mut longest_ns = 0;
    while started.elapsed() < length {
        thread::sleep(Duration::from_millis(100));
        let Some(telemetry) = mix.telemetry() else {
            return Err(EngineError::OutputGone);
        };
        last = telemetry.engine;
        if last.window_min_frames > 0 {
            fewest = fewest.min(last.window_min_frames);
        }
        most = most.max(last.window_max_frames);
        longest_ns = longest_ns.max(last.window_max_callback_ns);
        // A track that ends stops its deck, and the run with it.
        let settled = started.elapsed() > Duration::from_secs(1);
        if settled && telemetry.decks.iter().any(|deck| !deck.playing) {
            break;
        }
    }
    let seconds = started.elapsed().as_secs_f64();
    for deck in DeckId::ALL {
        mix.play(deck, false)?;
    }

    Ok(AudioRun {
        device,
        buffer_frames: last.buffer_frames,
        seconds,
        callbacks: last.callbacks - before.callbacks,
        underruns: last.underruns - before.underruns,
        missing_frames: last.missing_frames - before.missing_frames,
        late_callbacks: last.late_callbacks - before.late_callbacks,
        starved_callbacks: last.starved_callbacks - before.starved_callbacks,
        stream_errors: last.stream_errors - before.stream_errors,
        fewest_callback_frames: if fewest == u32::MAX { 0 } else { fewest },
        most_callback_frames: most,
        longest_callback_ms: longest_ns as f64 / 1e6,
    })
}

fn reading(mix: &Mix) -> Result<EngineTelemetry, EngineError> {
    mix.telemetry()
        .map(|telemetry| telemetry.engine)
        .ok_or(EngineError::OutputGone)
}
