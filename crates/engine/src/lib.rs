//! Real-time audio: two decks streamed from files, mixed and played on the default output device.
//!
//! Decoding runs on a worker thread per deck that fills a lock-free ring buffer. The audio
//! callback in `output` reads those buffers and reports through atomics, and never allocates,
//! locks, logs or does I/O. Its clock is the only clock: deck positions come from the frames it
//! has played and the latency the device reports.

mod decode;
mod error;
mod output;
mod preview;
mod published;
mod reader;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use resonance_core::meter::PeakMeter;
use resonance_core::waveform::Waveform;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::decode::Decoder;
use crate::output::{Control, Output, RetiredFeed};
use crate::preview::{PreviewFeed, PreviewWorker, WorkerRequest};
use crate::published::{ClockReading, Published};

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

/// One reading of the decks, preview channel and audio thread, taken about 60 times a second.
#[derive(Debug, Clone, PartialEq)]
pub struct Telemetry {
    pub decks: [DeckTelemetry; DECKS],
    pub preview: PreviewTelemetry,
    pub engine: EngineTelemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PreviewTelemetry {
    pub generation: u32,
    pub playing: bool,
    pub position_seconds: f64,
    pub duration_seconds: f64,
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

/// Control-side snapshot of the Library preview player.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewStatus {
    pub generation: u32,
    pub path: Option<PathBuf>,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
}

/// The decks and the device. Nothing is opened until the first track is loaded.
pub struct Mix {
    epoch: Instant,
    published: Arc<Published>,
    running: Mutex<Option<Running>>,
    preview_volume_bits: AtomicU32,
    silent: bool,
}

impl Default for Mix {
    fn default() -> Self {
        Self::new()
    }
}

struct Running {
    output: Output,
    controls: Producer<Control>,
    retired: Consumer<RetiredFeed>,
    decks: [Loaded; DECKS],
    next_generation: u32,
    meters: [[PeakMeter; 2]; DECKS],
    last_reading_ns: u64,
    preview_worker: Option<PreviewWorker>,
    preview: PreviewLoaded,
}

#[derive(Default)]
struct PreviewLoaded {
    generation: u32,
    state_seq: u32,
    path: Option<PathBuf>,
    duration_ms: u64,
    desired_playing: bool,
    fallback_position_ms: u64,
    floor_position_seconds: f64,
}

#[derive(Default)]
struct Loaded {
    generation: u32,
    state_seq: u32,
    desired_playing: bool,
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
            preview_volume_bits: AtomicU32::new(1.0f32.to_bits()),
            silent: false,
        }
    }

    /// Creates an engine whose audio output thread ticks silently at 48 kHz without opening the
    /// host speaker hardware (for offline API and integration tests).
    #[doc(hidden)]
    pub fn new_silent() -> Self {
        Self {
            silent: true,
            ..Self::new()
        }
    }

    fn ensure_running<'a>(
        &self,
        running: &'a mut Option<Running>,
    ) -> Result<&'a mut Running, EngineError> {
        if running.is_none() {
            let opened = output::open(&self.published, self.epoch, self.silent)?;
            let mut state = Running {
                output: opened.output,
                controls: opened.controls,
                retired: opened.retired,
                decks: Default::default(),
                next_generation: 1,
                meters: Default::default(),
                last_reading_ns: 0,
                preview_worker: None,
                preview: PreviewLoaded::default(),
            };
            let gain = f32::from_bits(self.preview_volume_bits.load(Ordering::Relaxed));
            state.send(Control::PreviewSetGain { gain })?;
            *running = Some(state);
        }
        running.as_mut().ok_or(EngineError::OutputGone)
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
        let state = self.ensure_running(&mut running)?;
        let index = deck.index();
        let state_seq = state.decks[index].state_seq.wrapping_add(1).max(1);
        let feed = decode::feed_deck(decoder, state.output.device.sample_rate)?;
        state.send(Control::Load {
            deck: index,
            feed,
            state_seq,
        })?;
        let generation = state.next_generation;
        state.next_generation = state.next_generation.wrapping_add(1).max(1);
        state.decks[index] = Loaded {
            generation,
            state_seq,
            desired_playing: false,
            path: Some(path.to_owned()),
            stated_duration_seconds,
            waveform: Some(waveform),
        };
        Ok(self.status_of(&running))
    }

    pub fn play(&self, deck: DeckId, playing: bool) -> Result<MixStatus, EngineError> {
        let mut running = self.lock();
        if playing {
            self.set_preview_playing_locked(&mut running, false)?;
        }
        if let Some(state) = running.as_mut() {
            let index = deck.index();
            let effective_playing = playing && state.decks[index].path.is_some();
            let state_seq = state.decks[index].state_seq.wrapping_add(1).max(1);
            state.decks[index].state_seq = state_seq;
            state.decks[index].desired_playing = effective_playing;
            state.send(Control::Play {
                deck: index,
                playing: effective_playing,
                state_seq,
            })?;
        }
        Ok(self.status_of(&running))
    }

    /// True while either Mix deck is playing (immediately reflecting control changes before the
    /// next audio callback, and automatically clearing when a track reaches natural end-of-file).
    pub fn any_deck_playing(&self) -> bool {
        let running = self.lock();
        let Some(state) = running.as_ref() else {
            return false;
        };
        let clock = self.published.clock.read();
        (0..DECKS).any(|index| deck_playing(&state.decks[index], &clock, index))
    }

    /// Loads `path` into the preview channel at `start_ms` and begins playback, opening the
    /// shared audio device if needed.
    pub fn preview_load_and_play(
        &self,
        path: &Path,
        start_ms: u64,
        fallback_duration_ms: Option<u64>,
    ) -> Result<PreviewStatus, EngineError> {
        let decoder = Decoder::open(path)?;
        let decoded_duration_ms = decoder.duration_ms();
        let duration_ms = if decoded_duration_ms > 0 {
            decoded_duration_ms
        } else {
            fallback_duration_ms.unwrap_or(0)
        };
        let clamped_start = if duration_ms > 0 {
            start_ms.min(duration_ms)
        } else {
            start_ms
        };
        let mut running = self.lock();
        let state = self.ensure_running(&mut running)?;
        start_preview_feed(state, path, duration_ms, clamped_start, true, Some(decoder))?;
        Ok(self.preview_status_of(&running))
    }

    pub fn preview_toggle_play(&self) -> Result<PreviewStatus, EngineError> {
        let mut running = self.lock();
        let current = self.preview_status_of(&running);
        if current.path.is_none() {
            return Ok(current);
        }
        let clock = self.published.clock.read();
        let natural_eof = running.as_ref().is_some_and(|state| {
            clock.preview_generation == state.preview.generation
                && clock.preview_state_seq == state.preview.state_seq
                && clock.preview_ended
        });
        if current.playing {
            self.set_preview_playing_locked(&mut running, false)?;
        } else if natural_eof
            || (current.duration_ms > 0 && current.position_ms >= current.duration_ms)
        {
            self.seek_preview_locked(&mut running, 0, Some(true))?;
        } else {
            self.set_preview_playing_locked(&mut running, true)?;
        }
        Ok(self.preview_status_of(&running))
    }

    pub fn preview_pause(&self) -> Result<PreviewStatus, EngineError> {
        let mut running = self.lock();
        self.set_preview_playing_locked(&mut running, false)?;
        Ok(self.preview_status_of(&running))
    }

    pub fn preview_stop(&self) -> Result<PreviewStatus, EngineError> {
        let mut running = self.lock();
        if let Some(state) = running.as_ref()
            && state.preview.path.is_some()
        {
            self.seek_preview_locked(&mut running, 0, Some(false))?;
        }
        Ok(self.preview_status_of(&running))
    }

    pub fn preview_seek(&self, position_ms: u64) -> Result<PreviewStatus, EngineError> {
        let mut running = self.lock();
        let current = self.preview_status_of(&running);
        if current.path.is_none() {
            return Ok(current);
        }
        self.seek_preview_locked(&mut running, position_ms, Some(current.playing))?;
        Ok(self.preview_status_of(&running))
    }

    pub fn preview_set_volume(&self, gain_linear: f32) -> Result<PreviewStatus, EngineError> {
        let clamped = if gain_linear.is_finite() {
            gain_linear.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.preview_volume_bits
            .store(clamped.to_bits(), Ordering::Relaxed);
        let mut running = self.lock();
        if let Some(state) = running.as_mut() {
            state.send(Control::PreviewSetGain { gain: clamped })?;
        }
        Ok(self.preview_status_of(&running))
    }

    pub fn preview_status(&self) -> PreviewStatus {
        let mut running = self.lock();
        if let Some(state) = running.as_mut() {
            state.drop_retired();
        }
        self.preview_status_of(&running)
    }

    fn set_preview_playing_locked(
        &self,
        running: &mut Option<Running>,
        playing: bool,
    ) -> Result<(), EngineError> {
        let snap_ms = self.preview_status_of(running).position_ms;
        let Some(state) = running.as_mut() else {
            return Ok(());
        };
        if state.preview.path.is_none() {
            return Ok(());
        }
        state.preview.state_seq = state.preview.state_seq.wrapping_add(1).max(1);
        state.preview.desired_playing = playing;
        state.preview.fallback_position_ms = snap_ms;
        let state_seq = state.preview.state_seq;
        state.send(Control::PreviewPlay { playing, state_seq })
    }

    fn seek_preview_locked(
        &self,
        running: &mut Option<Running>,
        position_ms: u64,
        play_override: Option<bool>,
    ) -> Result<(), EngineError> {
        let current = self.preview_status_of(running);
        let Some(state) = running.as_mut() else {
            return Ok(());
        };
        let Some(path) = state.preview.path.clone() else {
            return Ok(());
        };
        let duration_ms = state.preview.duration_ms;
        let target_ms = if duration_ms > 0 {
            position_ms.min(duration_ms)
        } else {
            position_ms
        };
        let play = play_override.unwrap_or(current.playing);
        start_preview_feed(state, &path, duration_ms, target_ms, play, None)
    }

    fn preview_status_of(&self, running: &Option<Running>) -> PreviewStatus {
        let volume = f32::from_bits(self.preview_volume_bits.load(Ordering::Relaxed));
        let Some(state) = running.as_ref() else {
            return PreviewStatus {
                generation: 0,
                path: None,
                playing: false,
                position_ms: 0,
                duration_ms: 0,
                volume,
            };
        };
        let loaded = &state.preview;
        if loaded.path.is_none() {
            return PreviewStatus {
                generation: loaded.generation,
                path: None,
                playing: false,
                position_ms: 0,
                duration_ms: 0,
                volume,
            };
        }
        let clock = self.published.clock.read();
        let now_ns = nanos(self.epoch.elapsed());
        let reading = preview_telemetry_of(state, &clock, now_ns);
        let raw_ms = (reading.position_seconds.max(0.0) * 1000.0).round() as u64;
        let position_ms = if loaded.duration_ms > 0 {
            raw_ms.min(loaded.duration_ms)
        } else {
            raw_ms
        };
        PreviewStatus {
            generation: reading.generation,
            path: loaded.path.clone(),
            playing: reading.playing,
            position_ms,
            duration_ms: loaded.duration_ms,
            volume,
        }
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
            let position_seconds = clock_position_seconds(
                clock.played_frames[index],
                clock.playing[index],
                rate,
                now_ns,
                &clock,
            );
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
        let preview = preview_telemetry_of(state, &clock, now_ns);
        let counters = &self.published.counters;
        let total = |counter: &std::sync::atomic::AtomicU64| counter.load(Ordering::Relaxed);
        let window_min_frames = counters.window_min_frames.swap(u32::MAX, Ordering::Relaxed);
        Some(Telemetry {
            decks,
            preview,
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
            let index = id.index();
            let loaded = running.as_ref().map(|state| &state.decks[index]);
            DeckStatus {
                deck: id,
                generation: loaded.map_or(0, |loaded| loaded.generation),
                path: loaded.and_then(|loaded| loaded.path.clone()),
                duration_seconds: loaded.map_or(0.0, Loaded::duration_seconds),
                playing: loaded.is_some_and(|loaded| deck_playing(loaded, &clock, index)),
            }
        };
        MixStatus {
            device: running.as_ref().map(|state| state.output.device.clone()),
            decks: DeckId::ALL.map(deck),
        }
    }
}

fn deck_playing(loaded: &Loaded, clock: &ClockReading, index: usize) -> bool {
    if loaded.path.is_none() {
        false
    } else if clock.deck_state_seq[index] == loaded.state_seq {
        clock.playing[index]
    } else {
        loaded.desired_playing
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
        while let Ok(retired) = self.retired.pop() {
            retired.discard();
        }
    }
}

fn clock_position_seconds(
    played_frames: u64,
    playing: bool,
    sample_rate: f64,
    now_ns: u64,
    clock: &ClockReading,
) -> f64 {
    let played_seconds = played_frames as f64 / sample_rate;
    if playing {
        let heard_from_ns = clock.anchor_ns.saturating_add(clock.latency_ns);
        played_seconds + (now_ns as f64 - heard_from_ns as f64) / 1e9
    } else {
        played_seconds
    }
}

fn preview_telemetry_of(state: &Running, clock: &ClockReading, now_ns: u64) -> PreviewTelemetry {
    let loaded = &state.preview;
    if loaded.path.is_none() {
        return PreviewTelemetry::default();
    }
    let duration_seconds = loaded.duration_ms as f64 / 1000.0;
    let (playing, position_seconds) = if clock.preview_generation == loaded.generation
        && clock.preview_state_seq == loaded.state_seq
    {
        let rate = f64::from(state.output.device.sample_rate.max(1));
        if clock.preview_ended {
            let ended_pos = if duration_seconds > 0.0 {
                duration_seconds
            } else {
                clock.preview_played_frames as f64 / rate
            };
            (false, ended_pos)
        } else {
            let pos = clock_position_seconds(
                clock.preview_played_frames,
                clock.preview_playing,
                rate,
                now_ns,
                clock,
            )
            .max(loaded.floor_position_seconds);
            (clock.preview_playing, pos)
        }
    } else {
        (
            loaded.desired_playing,
            loaded.fallback_position_ms as f64 / 1000.0,
        )
    };
    let clamped = if duration_seconds > 0.0 {
        position_seconds.clamp(0.0, duration_seconds)
    } else {
        position_seconds.max(0.0)
    };
    PreviewTelemetry {
        generation: loaded.generation,
        playing,
        position_seconds: clamped,
        duration_seconds,
    }
}

fn start_preview_feed(
    state: &mut Running,
    path: &Path,
    duration_ms: u64,
    start_ms: u64,
    play: bool,
    initial_decoder: Option<Decoder>,
) -> Result<(), EngineError> {
    if state.preview_worker.is_none() {
        state.preview_worker = Some(PreviewWorker::spawn()?);
    }
    let sample_rate = state.output.device.sample_rate;
    let (producer, samples) = RingBuffer::new(preview::buffer_capacity(sample_rate));
    let finished = Arc::new(AtomicBool::new(false));
    let generation = state.preview.generation.wrapping_add(1).max(1);
    let state_seq = state.preview.state_seq.wrapping_add(1).max(1);
    let start_frame = preview::start_frame_for_ms(start_ms, sample_rate);

    if let Some(worker) = state.preview_worker.as_ref() {
        worker.enqueue(WorkerRequest {
            generation,
            path: path.to_owned(),
            start_ms,
            output_rate: sample_rate,
            producer,
            finished: Arc::clone(&finished),
            initial_decoder,
        });
    }
    state.send(Control::PreviewLoad {
        feed: PreviewFeed {
            samples,
            finished,
            start_frame,
            generation,
            state_seq,
        },
        play,
    })?;
    state.preview = PreviewLoaded {
        generation,
        state_seq,
        path: Some(path.to_owned()),
        duration_ms,
        desired_playing: play,
        fallback_position_ms: start_ms,
        floor_position_seconds: start_ms as f64 / 1000.0,
    };
    Ok(())
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::thread;

    use resonance_fixtures::signal::Signal;

    use super::*;

    fn test_file(name: &str) -> PathBuf {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/engine-mix");
        fs::create_dir_all(&folder).unwrap();
        folder.join(name)
    }

    #[test]
    fn preview_load_seek_toggle_volume_and_mix_handoff_work_silently() {
        let path = test_file("preview-mix.wav");
        Signal::tone(440.0, 2.0, -6.0, 44_100)
            .write_wav(&path)
            .unwrap();

        let mix = Mix::new_silent();
        let initial = mix.preview_set_volume(0.6).unwrap();
        assert!(!initial.playing);
        assert_eq!(initial.volume, 0.6);

        let loaded = mix.preview_load_and_play(&path, 0, None).unwrap();
        assert!(loaded.playing);
        assert_eq!(loaded.duration_ms, 2_000);
        assert_eq!(loaded.volume, 0.6);

        thread::sleep(Duration::from_millis(60));
        let advanced = mix.preview_status();
        assert!(advanced.playing);
        assert!(advanced.position_ms > 0);

        let sought = mix.preview_seek(1_200).unwrap();
        assert!(sought.playing);
        assert!(sought.position_ms >= 1_200);

        let paused = mix.preview_toggle_play().unwrap();
        assert!(!paused.playing);

        let resumed = mix.preview_toggle_play().unwrap();
        assert!(resumed.playing);

        mix.load(DeckId::A, &path).unwrap();
        mix.play(DeckId::A, true).unwrap();
        assert!(!mix.preview_status().playing);
    }

    #[test]
    fn preview_restarts_from_zero_after_reaching_natural_eof() {
        let path = test_file("preview-eof.wav");
        Signal::tone(440.0, 0.12, -6.0, 44_100)
            .write_wav(&path)
            .unwrap();

        let mix = Mix::new_silent();
        mix.preview_load_and_play(&path, 0, None).unwrap();

        let started = Instant::now();
        while mix.preview_status().playing {
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "preview did not finish"
            );
            thread::sleep(Duration::from_millis(15));
        }

        let ended = mix.preview_status();
        assert!(!ended.playing);
        assert_eq!(ended.position_ms, ended.duration_ms);

        let replayed = mix.preview_toggle_play().unwrap();
        assert!(replayed.playing);
        assert!(replayed.position_ms < ended.duration_ms);
    }
}
