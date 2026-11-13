//! Real-time audio: two decks streamed from files, mixed and played on the default output device.
//!
//! Decoding runs on a worker thread per deck that fills a lock-free ring buffer. The audio
//! callback in `output` reads those buffers and reports through atomics, and never allocates,
//! locks, logs or does I/O. Its clock is the only clock: deck positions come from the frames it
//! has played and the latency the device reports.

mod decode;
mod error;
mod output;
mod published;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use resonance_core::meter::PeakMeter;
use resonance_core::waveform::Waveform;
use rtrb::{Consumer, Producer};

use crate::decode::{DeckFeed, Decoder};
use crate::output::{Control, Output};
use crate::published::Published;

pub use error::EngineError;
pub use output::{Device, REQUESTED_BUFFER_FRAMES};

pub(crate) const DECKS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeckId {
    A,
    B,
}

impl DeckId {
    pub const ALL: [DeckId; DECKS] = [DeckId::A, DeckId::B];

    pub fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

/// A deck as the control side knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckStatus {
    pub deck: DeckId,
    /// Changes with every load, so a view can tell one track's waveform from the next.
    /// Zero while the deck is empty.
    pub generation: u32,
    pub path: Option<PathBuf>,
    pub duration_seconds: f64,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MixStatus {
    /// Missing until the first track is loaded opens the device.
    pub device: Option<Device>,
    pub decks: [DeckStatus; DECKS],
}

/// One reading of the decks and the audio thread, taken for the page about 60 times a second.
#[derive(Debug, Clone, PartialEq)]
pub struct Telemetry {
    pub decks: [DeckTelemetry; DECKS],
    pub engine: EngineTelemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeckTelemetry {
    pub generation: u32,
    pub playing: bool,
    /// The position being heard at the moment of the reading.
    pub position_seconds: f64,
    pub duration_seconds: f64,
    /// Left and right, from 0 at the meter's floor to 1 at full scale.
    pub meter: [f32; 2],
}

/// Counts from the audio callback. Totals grow from the moment the device opened; the `window_`
/// values cover the time since the previous reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineTelemetry {
    pub sample_rate: u32,
    pub requested_buffer_frames: u32,
    pub period_frames: u32,
    pub buffer_frames: u32,
    pub callbacks: u64,
    pub underruns: u64,
    pub missing_frames: u64,
    pub late_callbacks: u64,
    pub starved_callbacks: u64,
    pub stream_errors: u64,
    pub window_min_frames: u32,
    pub window_max_frames: u32,
    pub window_max_callback_ns: u64,
}

/// The decks and the device. Nothing is opened until the first track is loaded.
pub struct Mix {
    epoch: Instant,
    published: Arc<Published>,
    running: Mutex<Option<Running>>,
}

impl Default for Mix {
    fn default() -> Self {
        Self::new()
    }
}

struct Running {
    output: Output,
    controls: Producer<Control>,
    retired: Consumer<DeckFeed>,
    decks: [Loaded; DECKS],
    next_generation: u32,
    meters: [[PeakMeter; 2]; DECKS],
    last_reading_ns: u64,
}

#[derive(Default)]
struct Loaded {
    generation: u32,
    path: Option<PathBuf>,
    /// From the container until the waveform has counted every frame.
    stated_duration_seconds: f64,
    waveform: Option<Arc<OnceLock<Arc<Waveform>>>>,
}

impl Loaded {
    fn duration_seconds(&self) -> f64 {
        self.ready_waveform()
            .map(|waveform| waveform.duration_seconds())
            .unwrap_or(self.stated_duration_seconds)
    }

    fn ready_waveform(&self) -> Option<&Arc<Waveform>> {
        self.waveform.as_ref().and_then(|cell| cell.get())
    }
}

impl Mix {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
            published: Arc::new(Published::default()),
            running: Mutex::new(None),
        }
    }

    /// Puts a track on a deck, stopped at its start, opening the device if this is the first.
    pub fn load(&self, deck: DeckId, path: &Path) -> Result<MixStatus, EngineError> {
        let mut running = self.lock();
        let decoder = Decoder::open(path)?;
        let stated_duration_seconds = decoder
            .frames
            .map(|frames| frames as f64 / f64::from(decoder.sample_rate))
            .unwrap_or(0.0);
        let waveform = decode::summarise(path)?;
        if running.is_none() {
            let opened = output::open(&self.published, self.epoch)?;
            *running = Some(Running {
                output: opened.output,
                controls: opened.controls,
                retired: opened.retired,
                decks: Default::default(),
                next_generation: 1,
                meters: Default::default(),
                last_reading_ns: 0,
            });
        }
        let Some(state) = running.as_mut() else {
            return Err(EngineError::OutputGone);
        };
        let feed = decode::feed_deck(decoder, state.output.device.sample_rate)?;
        state.send(Control::Load {
            deck: deck.index(),
            feed,
        })?;
        let generation = state.next_generation;
        state.next_generation = state.next_generation.wrapping_add(1).max(1);
        state.decks[deck.index()] = Loaded {
            generation,
            path: Some(path.to_owned()),
            stated_duration_seconds,
            waveform: Some(waveform),
        };
        Ok(self.status_of(&running))
    }

    pub fn play(&self, deck: DeckId, playing: bool) -> Result<MixStatus, EngineError> {
        let mut running = self.lock();
        if let Some(state) = running.as_mut() {
            state.send(Control::Play {
                deck: deck.index(),
                playing,
            })?;
        }
        Ok(self.status_of(&running))
    }

    pub fn status(&self) -> MixStatus {
        self.status_of(&self.lock())
    }

    /// `None` while no device is open.
    pub fn telemetry(&self) -> Option<Telemetry> {
        let mut running = self.lock();
        let state = running.as_mut()?;
        state.drop_retired();
        let clock = self.published.clock.read();
        let now_ns = nanos(self.epoch.elapsed());
        let elapsed_seconds = now_ns.saturating_sub(state.last_reading_ns) as f32 / 1e9;
        state.last_reading_ns = now_ns;
        let rate = f64::from(state.output.device.sample_rate.max(1));
        let mut decks = [DeckTelemetry::default(); DECKS];
        for (index, deck) in decks.iter_mut().enumerate() {
            let loaded = &state.decks[index];
            let duration_seconds = loaded.duration_seconds();
            let played_seconds = clock.played_frames[index] as f64 / rate;
            let position_seconds = if clock.playing[index] {
                let heard_from_ns = clock.anchor_ns.saturating_add(clock.latency_ns);
                played_seconds + (now_ns as f64 - heard_from_ns as f64) / 1e9
            } else {
                played_seconds
            };
            for (channel, meter) in state.meters[index].iter_mut().enumerate() {
                meter.update(self.published.peaks.take(index, channel), elapsed_seconds);
            }
            *deck = DeckTelemetry {
                generation: loaded.generation,
                playing: clock.playing[index],
                position_seconds: position_seconds.clamp(0.0, duration_seconds.max(0.0)),
                duration_seconds,
                meter: [
                    state.meters[index][0].position(),
                    state.meters[index][1].position(),
                ],
            };
        }
        let counters = &self.published.counters;
        let total = |counter: &std::sync::atomic::AtomicU64| counter.load(Ordering::Relaxed);
        let window_min_frames = counters.window_min_frames.swap(u32::MAX, Ordering::Relaxed);
        Some(Telemetry {
            decks,
            engine: EngineTelemetry {
                sample_rate: state.output.device.sample_rate,
                requested_buffer_frames: state.output.device.requested_buffer_frames,
                period_frames: state.output.device.period_frames,
                buffer_frames: counters.buffer_frames.load(Ordering::Relaxed),
                callbacks: total(&counters.callbacks),
                underruns: total(&counters.underruns),
                missing_frames: total(&counters.missing_frames),
                late_callbacks: total(&counters.late_callbacks),
                starved_callbacks: total(&counters.starved_callbacks),
                stream_errors: total(&counters.stream_errors),
                window_min_frames: if window_min_frames == u32::MAX {
                    0
                } else {
                    window_min_frames
                },
                window_max_frames: counters.window_max_frames.swap(0, Ordering::Relaxed),
                window_max_callback_ns: counters.window_max_cost_ns.swap(0, Ordering::Relaxed),
            },
        })
    }

    /// The deck's waveform with the generation it belongs to, once it has been worked out.
    pub fn waveform(&self, deck: DeckId) -> Option<(u32, Arc<Waveform>)> {
        let running = self.lock();
        let loaded = &running.as_ref()?.decks[deck.index()];
        loaded
            .ready_waveform()
            .map(|waveform| (loaded.generation, Arc::clone(waveform)))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        // Every write under this lock leaves the state whole, so a panic elsewhere leaves
        // nothing half-done.
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn status_of(&self, running: &Option<Running>) -> MixStatus {
        let clock = self.published.clock.read();
        let deck = |id: DeckId| {
            let loaded = running.as_ref().map(|state| &state.decks[id.index()]);
            DeckStatus {
                deck: id,
                generation: loaded.map_or(0, |loaded| loaded.generation),
                path: loaded.and_then(|loaded| loaded.path.clone()),
                duration_seconds: loaded.map_or(0.0, Loaded::duration_seconds),
                playing: running.is_some() && clock.playing[id.index()],
            }
        };
        MixStatus {
            device: running.as_ref().map(|state| state.output.device.clone()),
            decks: DeckId::ALL.map(deck),
        }
    }
}

impl Running {
    fn send(&mut self, control: Control) -> Result<(), EngineError> {
        self.drop_retired();
        self.controls
            .push(control)
            .map_err(|_| EngineError::AudioStalled)
    }

    fn drop_retired(&mut self) {
        while self.retired.pop().is_ok() {}
    }
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
