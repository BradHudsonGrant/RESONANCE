//! The audio device and the callback that fills it.
//!
//! The callback runs on the device's thread. It never allocates, locks, logs or does I/O: decks
//! arrive and leave through ring buffers, samples come from each deck's ring buffer, and what it
//! reports goes out through atomics in [`Published`].

#[cfg(windows)]
mod wasapi_stream;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, OutputCallbackInfo, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::DECKS;
use crate::decode::DeckFeed;
use crate::error::EngineError;
use crate::preview::PreviewFeed;
use crate::published::{ClockReading, Published};

/// The buffer the stream asks the device for.
pub const REQUESTED_BUFFER_FRAMES: u32 = 256;

/// Each deck plays 6 dB down, so two tracks mastered to full scale sum to full scale instead of
/// clipping.
const DECK_GAIN: f32 = 0.5;

/// One-pole gain smoother time constant (~5 ms settling time) so play, pause, seek and volume
/// changes never click.
const GAIN_RAMP_TAU_SECONDS: f32 = 0.005 / 3.0;

/// Room for commands on their way to the callback. The retirement queue has double capacity so
/// replacing active and fading feeds in the same callback cannot drop on the audio thread.
const CONTROL_CAPACITY: usize = 128;

pub(crate) enum RetiredFeed {
    Deck(DeckFeed),
    Preview(PreviewFeed),
}

impl RetiredFeed {
    pub(crate) fn discard(self) {
        match self {
            Self::Deck(feed) => drop(feed),
            Self::Preview(feed) => drop(feed),
        }
    }
}

pub(crate) enum Control {
    Load {
        deck: usize,
        feed: DeckFeed,
        state_seq: u32,
    },
    Play {
        deck: usize,
        playing: bool,
        state_seq: u32,
    },
    PreviewLoad {
        feed: PreviewFeed,
        play: bool,
    },
    PreviewPlay {
        playing: bool,
        state_seq: u32,
    },
    PreviewSetGain {
        gain: f32,
    },
}

/// How the stream asks for the output device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// Shared with other programs, at the shortest period the system's mixer allows.
    #[default]
    Shared,
    /// The device for this program alone, at [`REQUESTED_BUFFER_FRAMES`] where it allows that.
    /// Other programs cannot play sound meanwhile. Only Windows has it; elsewhere, or when the
    /// device refuses, the stream is shared.
    Exclusive,
}

/// The device as the stream opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// The mode the device accepted, which is shared when exclusive use was refused.
    pub mode: OutputMode,
    pub requested_buffer_frames: u32,
    /// The period the device runs its callback at, whatever was asked for.
    pub period_frames: u32,
}

/// The open stream. It lives on its own thread, which drops it when this is dropped.
pub(crate) struct Output {
    stop: Option<mpsc::Sender<()>>,
    /// Read by a render loop that runs on the output thread itself, which cannot wait on `stop`.
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pub device: Device,
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) struct Opened {
    pub output: Output,
    pub controls: Producer<Control>,
    pub retired: Consumer<RetiredFeed>,
}

pub(crate) fn open(
    published: &Arc<Published>,
    epoch: Instant,
    force_silent: bool,
    mode: OutputMode,
) -> Result<Opened, EngineError> {
    if force_silent || use_silent_output() {
        return open_silent(published, epoch);
    }
    let (renderer, controls, retired) = Renderer::new(published, epoch);
    let (ready, opened) = mpsc::sync_channel(1);
    let (stop, stopped) = mpsc::channel::<()>();
    let stopping = Arc::new(AtomicBool::new(false));
    let for_thread = Arc::clone(published);
    let stopping_for_thread = Arc::clone(&stopping);
    let thread = thread::Builder::new()
        .name("audio output".to_owned())
        .spawn(move || {
            #[cfg(windows)]
            let renderer = match wasapi_stream::Stream::open(mode, &for_thread) {
                Ok(stream) => {
                    let _ = ready.send(Ok(stream.device.clone()));
                    stream.run(renderer, &stopping_for_thread);
                    return;
                }
                // The device refused both modes through WASAPI directly, so cpal gets a go.
                Err(_) => renderer,
            };
            #[cfg(not(windows))]
            let _ = (mode, &stopping_for_thread);
            match start(renderer, &for_thread) {
                Ok((stream, device)) => {
                    let _ = ready.send(Ok(device));
                    let _ = stopped.recv();
                    drop(stream);
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            }
        })
        .map_err(EngineError::Thread)?;
    let device = opened.recv().map_err(|_| EngineError::OutputGone)??;
    Ok(Opened {
        output: Output {
            stop: Some(stop),
            stopping,
            thread: Some(thread),
            device,
        },
        controls,
        retired,
    })
}

fn use_silent_output() -> bool {
    matches!(
        std::env::var("RESONANCE_SILENT_AUDIO").as_deref(),
        Ok("1" | "true")
    )
}

fn open_silent(published: &Arc<Published>, epoch: Instant) -> Result<Opened, EngineError> {
    const SILENT_RATE: u32 = 48_000;
    const SILENT_CHANNELS: u16 = 2;
    const SILENT_PERIOD_FRAMES: u32 = 480;
    let (mut renderer, controls, retired) = Renderer::new(published, epoch);
    renderer.set_format(SILENT_RATE, usize::from(SILENT_CHANNELS));
    published
        .counters
        .period_frames
        .store(SILENT_PERIOD_FRAMES, Ordering::Relaxed);
    let (stop, stopped) = mpsc::channel::<()>();
    let thread = thread::Builder::new()
        .name("silent audio output".to_owned())
        .spawn(move || {
            let mut out =
                vec![0.0f32; SILENT_PERIOD_FRAMES as usize * usize::from(SILENT_CHANNELS)];
            let step = Duration::from_millis(10);
            while stopped.recv_timeout(step) == Err(mpsc::RecvTimeoutError::Timeout) {
                renderer.render_with_latency(&mut out, step);
            }
        })
        .map_err(EngineError::Thread)?;
    Ok(Opened {
        output: Output {
            stop: Some(stop),
            stopping: Arc::new(AtomicBool::new(false)),
            thread: Some(thread),
            device: Device {
                name: "Silent output".to_owned(),
                sample_rate: SILENT_RATE,
                channels: SILENT_CHANNELS,
                mode: OutputMode::Shared,
                requested_buffer_frames: REQUESTED_BUFFER_FRAMES,
                period_frames: SILENT_PERIOD_FRAMES,
            },
        },
        controls,
        retired,
    })
}

fn start(
    mut renderer: Renderer,
    published: &Arc<Published>,
) -> Result<(cpal::Stream, Device), EngineError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(EngineError::NoOutputDevice)?;
    let name = device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_else(|_| "Default output".to_owned());
    let supported = device.default_output_config()?;
    let channels = supported.channels();
    let sample_rate = supported.sample_rate();
    renderer.set_format(sample_rate, usize::from(channels.max(1)));
    let errors = Arc::clone(published);
    let stream = device.build_output_stream::<f32, _, _>(
        StreamConfig {
            channels,
            sample_rate,
            buffer_size: BufferSize::Fixed(REQUESTED_BUFFER_FRAMES),
        },
        move |out: &mut [f32], info: &OutputCallbackInfo| renderer.render(out, info),
        move |_| {
            errors
                .counters
                .stream_errors
                .fetch_add(1, Ordering::Relaxed);
        },
        None,
    )?;
    let period_frames = stream.buffer_size().unwrap_or(0);
    published
        .counters
        .period_frames
        .store(period_frames, Ordering::Relaxed);
    stream.play()?;
    Ok((
        stream,
        Device {
            name,
            sample_rate,
            channels,
            mode: OutputMode::Shared,
            requested_buffer_frames: REQUESTED_BUFFER_FRAMES,
            period_frames,
        },
    ))
}

#[derive(Default)]
struct Deck {
    feed: Option<DeckFeed>,
    playing: bool,
    state_seq: u32,
    /// Frames played from the feed, at the device's rate.
    played_frames: u64,
}

#[derive(Default)]
struct PreviewChannel {
    feed: Option<PreviewFeed>,
    fading_feed: Option<PreviewFeed>,
    playing: bool,
    ended: bool,
    played_frames: u64,
    generation: u32,
    state_seq: u32,
    target_gain: f32,
    current_gain: f32,
    fade_out_gain: f32,
}

struct Renderer {
    controls: Consumer<Control>,
    retired: Producer<RetiredFeed>,
    decks: [Deck; DECKS],
    preview: PreviewChannel,
    published: Arc<Published>,
    channels: usize,
    gain_alpha: f32,
    epoch: Instant,
    first_callback: bool,
    /// Set for an exclusive stream, which is always asked for exactly one period. Lateness then
    /// shows in the time between callbacks rather than in how much each is asked for.
    event_period: Option<Duration>,
    last_entered: Option<Instant>,
}

impl Renderer {
    fn new(
        published: &Arc<Published>,
        epoch: Instant,
    ) -> (Self, Producer<Control>, Consumer<RetiredFeed>) {
        let (controls, controls_in_callback) = RingBuffer::new(CONTROL_CAPACITY);
        let (retired_in_callback, retired) = RingBuffer::new(CONTROL_CAPACITY * 2);
        let mut renderer = Self {
            controls: controls_in_callback,
            retired: retired_in_callback,
            decks: Default::default(),
            preview: PreviewChannel {
                target_gain: 1.0,
                ..PreviewChannel::default()
            },
            published: Arc::clone(published),
            channels: 2,
            gain_alpha: 0.0,
            epoch,
            first_callback: true,
            event_period: None,
            last_entered: None,
        };
        renderer.set_format(48_000, 2);
        (renderer, controls, retired)
    }

    fn set_format(&mut self, sample_rate: u32, channels: usize) {
        self.channels = channels.max(1);
        let tau_samples = GAIN_RAMP_TAU_SECONDS * (sample_rate.max(1) as f32);
        self.gain_alpha = 1.0 - (-1.0 / tau_samples).exp();
    }

    fn render(&mut self, out: &mut [f32], info: &OutputCallbackInfo) {
        let stamp = info.timestamp();
        let latency = stamp.playback.saturating_duration_since(stamp.callback);
        self.render_with_latency(out, latency);
    }

    fn render_with_latency(&mut self, out: &mut [f32], latency: Duration) {
        let entered = Instant::now();
        while let Ok(control) = self.controls.pop() {
            self.apply(control);
        }
        out.fill(0.0);
        let frames = out.len() / self.channels;
        let mut reading = ClockReading {
            anchor_ns: nanos(entered.saturating_duration_since(self.epoch)),
            latency_ns: nanos(latency),
            preview_generation: self.preview.generation,
            preview_state_seq: self.preview.state_seq,
            preview_played_frames: self.preview.played_frames,
            preview_playing: self.preview.playing,
            preview_ended: self.preview.ended,
            ..ClockReading::default()
        };
        for (index, deck) in self.decks.iter_mut().enumerate() {
            reading.played_frames[index] = deck.played_frames;
            reading.playing[index] = deck.playing;
            reading.deck_state_seq[index] = deck.state_seq;
            deck.render(index, out, self.channels, frames, &self.published);
        }
        self.preview.render(
            out,
            self.channels,
            frames,
            &mut self.retired,
            self.gain_alpha,
        );
        self.published.clock.publish(&reading);
        self.count(u32::try_from(frames).unwrap_or(u32::MAX), entered);
    }

    fn apply(&mut self, control: Control) {
        match control {
            Control::Load {
                deck,
                feed,
                state_seq,
            } => {
                let deck = &mut self.decks[deck];
                if let Some(old) = deck.feed.replace(feed) {
                    // Freeing a feed would free memory here, so it goes back to be dropped
                    // elsewhere. The queue has room: see CONTROL_CAPACITY.
                    let _ = self.retired.push(RetiredFeed::Deck(old));
                }
                deck.playing = false;
                deck.state_seq = state_seq;
                deck.played_frames = 0;
            }
            Control::Play {
                deck,
                playing,
                state_seq,
            } => {
                let deck = &mut self.decks[deck];
                deck.state_seq = state_seq;
                deck.playing = playing && deck.feed.is_some();
            }
            Control::PreviewLoad { feed, play } => {
                if let Some(old) = self.preview.feed.take() {
                    if self.preview.playing && self.preview.current_gain > 0.0 {
                        if let Some(stale) = self.preview.fading_feed.take() {
                            let _ = self.retired.push(RetiredFeed::Preview(stale));
                        }
                        self.preview.fade_out_gain = self.preview.current_gain;
                        self.preview.fading_feed = Some(old);
                    } else {
                        let _ = self.retired.push(RetiredFeed::Preview(old));
                    }
                }
                self.preview.played_frames = feed.start_frame;
                self.preview.generation = feed.generation;
                self.preview.state_seq = feed.state_seq;
                self.preview.feed = Some(feed);
                self.preview.playing = play;
                self.preview.ended = false;
                self.preview.current_gain = 0.0;
            }
            Control::PreviewPlay { playing, state_seq } => {
                self.preview.state_seq = state_seq;
                self.preview.playing =
                    playing && self.preview.feed.is_some() && !self.preview.ended;
            }
            Control::PreviewSetGain { gain } => {
                self.preview.target_gain = gain.clamp(0.0, 1.0);
            }
        }
    }

    fn count(&mut self, frames: u32, entered: Instant) {
        let counters = &self.published.counters;
        counters.callbacks.fetch_add(1, Ordering::Relaxed);
        if self.first_callback {
            // The device's buffer starts empty, so the first callback is asked to fill all of it.
            counters.buffer_frames.store(frames, Ordering::Relaxed);
            self.first_callback = false;
        } else if let (Some(period), Some(last)) = (self.event_period, self.last_entered) {
            // An exclusive device plays one period while the next is written, so a callback
            // more than a period late has let it run dry.
            let gap = entered.saturating_duration_since(last);
            if gap > period * 2 {
                counters.starved_callbacks.fetch_add(1, Ordering::Relaxed);
            } else if gap * 2 > period * 3 {
                counters.late_callbacks.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            let buffer = counters.buffer_frames.load(Ordering::Relaxed);
            let period = counters.period_frames.load(Ordering::Relaxed);
            if buffer > 0 && frames >= buffer {
                counters.starved_callbacks.fetch_add(1, Ordering::Relaxed);
            } else if period > 0 && u64::from(frames) * 2 > u64::from(period) * 3 {
                counters.late_callbacks.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.last_entered = Some(entered);
        counters
            .window_min_frames
            .fetch_min(frames, Ordering::Relaxed);
        counters
            .window_max_frames
            .fetch_max(frames, Ordering::Relaxed);
        counters
            .window_max_cost_ns
            .fetch_max(nanos(entered.elapsed()), Ordering::Relaxed);
    }
}

impl Deck {
    fn render(
        &mut self,
        index: usize,
        out: &mut [f32],
        channels: usize,
        frames: usize,
        published: &Published,
    ) {
        if !self.playing {
            return;
        }
        let Some(feed) = self.feed.as_mut() else {
            return;
        };
        // The decoder writes whole frames and the buffer's length is even, so neither slice
        // below splits a frame.
        let take = feed.samples.slots().min(frames * 2) & !1;
        let mut peak = [0.0f32; 2];
        if let Ok(chunk) = feed.samples.read_chunk(take) {
            let (first, second) = chunk.as_slices();
            let pairs = first
                .as_chunks::<2>()
                .0
                .iter()
                .chain(second.as_chunks::<2>().0);
            for (frame, &[left, right]) in pairs.enumerate() {
                peak[0] = peak[0].max(left.abs());
                peak[1] = peak[1].max(right.abs());
                mix_sample(out, frame * channels, channels, DECK_GAIN, left, right);
            }
            chunk.commit_all();
        }
        published.peaks.raise(index, 0, peak[0]);
        published.peaks.raise(index, 1, peak[1]);
        let played = take / 2;
        self.played_frames += played as u64;
        if played < frames {
            if feed.finished.load(Ordering::Acquire) && feed.samples.is_empty() {
                self.playing = false;
            } else {
                let counters = &published.counters;
                counters.underruns.fetch_add(1, Ordering::Relaxed);
                counters
                    .missing_frames
                    .fetch_add((frames - played) as u64, Ordering::Relaxed);
            }
        }
    }
}

fn mix_sample(out: &mut [f32], at: usize, channels: usize, gain: f32, left: f32, right: f32) {
    if channels == 1 {
        out[at] += 0.5 * gain * (left + right);
    } else {
        out[at] += gain * left;
        out[at + 1] += gain * right;
    }
}

fn mix_chunk_with_ramp(
    samples: &mut rtrb::Consumer<f32>,
    out: &mut [f32],
    channels: usize,
    frames: usize,
    gain: &mut f32,
    target_gain: f32,
    alpha: f32,
) -> usize {
    let take = samples.slots().min(frames * 2) & !1;
    if let Ok(chunk) = samples.read_chunk(take) {
        let (first, second) = chunk.as_slices();
        let pairs = first
            .as_chunks::<2>()
            .0
            .iter()
            .chain(second.as_chunks::<2>().0);
        for (frame, &[left, right]) in pairs.enumerate() {
            *gain += alpha * (target_gain - *gain);
            if (target_gain - *gain).abs() < 1e-5 {
                *gain = target_gain;
            }
            mix_sample(out, frame * channels, channels, *gain, left, right);
        }
        chunk.commit_all();
    }
    take / 2
}

impl PreviewChannel {
    fn render(
        &mut self,
        out: &mut [f32],
        channels: usize,
        frames: usize,
        retired: &mut Producer<RetiredFeed>,
        alpha: f32,
    ) {
        if let Some(mut fading) = self.fading_feed.take() {
            let faded_frames = mix_chunk_with_ramp(
                &mut fading.samples,
                out,
                channels,
                frames,
                &mut self.fade_out_gain,
                0.0,
                alpha,
            );
            if self.fade_out_gain == 0.0
                || faded_frames < frames
                || (fading.finished.load(Ordering::Acquire) && fading.samples.is_empty())
            {
                self.fade_out_gain = 0.0;
                let _ = retired.push(RetiredFeed::Preview(fading));
            } else {
                self.fading_feed = Some(fading);
            }
        }

        let Some(feed) = self.feed.as_mut() else {
            self.playing = false;
            self.current_gain = 0.0;
            return;
        };
        let desired_gain = if self.playing { self.target_gain } else { 0.0 };
        if !self.playing && self.current_gain == 0.0 {
            return;
        }

        let consumed = mix_chunk_with_ramp(
            &mut feed.samples,
            out,
            channels,
            frames,
            &mut self.current_gain,
            desired_gain,
            alpha,
        );
        self.played_frames += consumed as u64;

        if !self.playing && consumed < frames {
            self.current_gain = 0.0;
        }

        if self.playing
            && consumed < frames
            && feed.finished.load(Ordering::Acquire)
            && feed.samples.is_empty()
        {
            self.playing = false;
            self.ended = true;
            self.current_gain = 0.0;
        }
    }
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use cpal::{OutputStreamTimestamp, StreamInstant};

    use super::*;

    const LATENCY_NS: u64 = 20_000_000;

    fn info() -> OutputCallbackInfo {
        let callback = StreamInstant::new(1, 0);
        OutputCallbackInfo::new(OutputStreamTimestamp {
            callback,
            playback: callback + Duration::from_nanos(LATENCY_NS),
        })
    }

    /// A feed holding `frames` stereo frames, every left sample `left` and every right `right`.
    fn feed(frames: usize, left: f32, right: f32, finished: bool) -> DeckFeed {
        let (mut producer, samples) = RingBuffer::new(4096);
        for _ in 0..frames {
            producer.push(left).unwrap();
            producer.push(right).unwrap();
        }
        DeckFeed {
            samples,
            finished: Arc::new(AtomicBool::new(finished)),
        }
    }

    fn preview_feed(frames: usize, left: f32, right: f32, finished: bool) -> PreviewFeed {
        let (mut producer, samples) = RingBuffer::new(4096);
        for _ in 0..frames {
            producer.push(left).unwrap();
            producer.push(right).unwrap();
        }
        PreviewFeed {
            samples,
            finished: Arc::new(AtomicBool::new(finished)),
            start_frame: 0,
            generation: 1,
            state_seq: 1,
        }
    }

    struct Harness {
        renderer: Renderer,
        controls: Producer<Control>,
        retired: Consumer<RetiredFeed>,
        published: Arc<Published>,
    }

    impl Harness {
        fn new() -> Self {
            let published = Arc::new(Published::default());
            let (renderer, controls, retired) = Renderer::new(&published, Instant::now());
            Self {
                renderer,
                controls,
                retired,
                published,
            }
        }

        fn send(&mut self, control: Control) {
            assert!(self.controls.push(control).is_ok());
        }

        fn playing(&mut self, deck: usize, feed: DeckFeed) {
            self.send(Control::Load {
                deck,
                feed,
                state_seq: 1,
            });
            self.send(Control::Play {
                deck,
                playing: true,
                state_seq: 2,
            });
        }

        fn render(&mut self, frames: usize) -> Vec<f32> {
            let mut out = vec![1.0; frames * 2];
            self.renderer.render(&mut out, &info());
            out
        }

        fn count(&self, counter: &AtomicU64) -> u64 {
            counter.load(Ordering::Relaxed)
        }
    }

    #[test]
    fn two_playing_decks_are_mixed_at_half_gain_each() {
        let mut harness = Harness::new();
        harness.playing(0, feed(256, 0.5, -0.5, false));
        harness.playing(1, feed(256, 0.25, 0.25, false));

        let out = harness.render(256);

        assert!(out.chunks(2).all(|frame| frame == [0.375, -0.125]));
    }

    #[test]
    fn a_stopped_deck_is_silent_and_keeps_its_place() {
        let mut harness = Harness::new();
        harness.send(Control::Load {
            deck: 0,
            feed: feed(256, 0.5, 0.5, false),
            state_seq: 1,
        });

        let out = harness.render(128);

        assert!(out.iter().all(|sample| *sample == 0.0));
        assert_eq!(harness.published.clock.read().played_frames, [0, 0]);
    }

    #[test]
    fn each_deck_reports_its_loudest_sample_per_channel() {
        let mut harness = Harness::new();
        harness.playing(1, feed(64, -0.75, 0.5, false));

        harness.render(64);

        assert_eq!(harness.published.peaks.take(1, 0), 0.75);
        assert_eq!(harness.published.peaks.take(1, 1), 0.5);
        assert_eq!(harness.published.peaks.take(0, 0), 0.0);
    }

    #[test]
    fn the_clock_gives_the_frames_played_before_each_buffer_and_the_latency() {
        let mut harness = Harness::new();
        harness.playing(0, feed(1000, 0.1, 0.1, false));

        harness.render(256);
        harness.render(256);

        let reading = harness.published.clock.read();
        assert_eq!(reading.played_frames, [256, 0]);
        assert_eq!(reading.playing, [true, false]);
        assert_eq!(reading.latency_ns, LATENCY_NS);
    }

    #[test]
    fn a_feed_that_runs_short_counts_one_underrun_and_the_frames_it_missed() {
        let mut harness = Harness::new();
        harness.playing(0, feed(100, 0.5, 0.5, false));

        let out = harness.render(256);

        let counters = &harness.published.counters;
        assert_eq!(harness.count(&counters.underruns), 1);
        assert_eq!(harness.count(&counters.missing_frames), 156);
        assert!(out[200..].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn a_track_that_ends_stops_its_deck_without_an_underrun() {
        let mut harness = Harness::new();
        harness.playing(0, feed(100, 0.5, 0.5, true));

        harness.render(256);
        harness.render(256);

        assert_eq!(harness.count(&harness.published.counters.underruns), 0);
        assert_eq!(harness.published.clock.read().playing, [false, false]);
    }

    #[test]
    fn loading_over_a_track_sends_the_old_feed_back_to_be_dropped() {
        let mut harness = Harness::new();
        harness.playing(0, feed(10, 0.5, 0.5, false));
        harness.render(4);

        harness.send(Control::Load {
            deck: 0,
            feed: feed(10, 0.25, 0.25, false),
            state_seq: 3,
        });
        harness.render(4);

        assert!(harness.retired.pop().is_ok());
        assert_eq!(harness.published.clock.read().playing, [false, false]);
    }

    #[test]
    fn the_first_callback_sizes_the_buffer_and_a_full_request_later_counts_as_starved() {
        let mut harness = Harness::new();
        harness
            .published
            .counters
            .period_frames
            .store(480, Ordering::Relaxed);

        harness.render(1056);
        harness.render(480);
        harness.render(800);
        harness.render(1056);

        let counters = &harness.published.counters;
        assert_eq!(counters.buffer_frames.load(Ordering::Relaxed), 1056);
        assert_eq!(harness.count(&counters.late_callbacks), 1);
        assert_eq!(harness.count(&counters.starved_callbacks), 1);
        assert_eq!(harness.count(&counters.callbacks), 4);
    }

    #[test]
    fn an_exclusive_stream_judges_late_and_starved_callbacks_by_the_time_between_them() {
        let mut harness = Harness::new();
        let period = Duration::from_micros(5_333);
        harness.renderer.event_period = Some(period);
        let start = Instant::now();

        harness.renderer.count(256, start);
        harness.renderer.count(256, start + period);
        harness
            .renderer
            .count(256, start + period * 2 + period * 3 / 4);
        harness.renderer.count(256, start + period * 5);

        let counters = &harness.published.counters;
        assert_eq!(harness.count(&counters.late_callbacks), 1);
        assert_eq!(harness.count(&counters.starved_callbacks), 1);
        assert_eq!(harness.count(&counters.callbacks), 4);
    }

    #[test]
    fn preview_ramps_smoothly_on_start_volume_change_pause_and_seek() {
        let mut harness = Harness::new();
        harness.send(Control::PreviewSetGain { gain: 0.8 });
        harness.send(Control::PreviewLoad {
            feed: preview_feed(2000, 1.0, -1.0, false),
            play: true,
        });

        let start = harness.render(480);
        // Starts ramped from near zero instead of jumping straight to 0.8.
        assert!(start[0].abs() < 0.05, "first sample {}", start[0]);
        assert!((start[start.len() - 2] - 0.8).abs() < 0.01);

        // Volume drop ramps smoothly without a single-sample step jump.
        harness.send(Control::PreviewSetGain { gain: 0.25 });
        let vol = harness.render(480);
        let prev = start[start.len() - 2];
        assert!((vol[0] - prev).abs() < 0.05);
        assert!((vol[vol.len() - 2] - 0.25).abs() < 0.01);

        // Pausing fades down to zero smoothly and stops advancing thereafter.
        harness.send(Control::PreviewPlay {
            playing: false,
            state_seq: 2,
        });
        let fade = harness.render(960);
        assert!((fade[0] - vol[vol.len() - 2]).abs() < 0.05);
        assert_eq!(fade[fade.len() - 2], 0.0);

        let paused = harness.render(128);
        assert!(paused.iter().all(|s| *s == 0.0));
        let played_while_paused = harness.published.clock.read().preview_played_frames;
        harness.render(128);
        assert_eq!(
            harness.published.clock.read().preview_played_frames,
            played_while_paused
        );

        // Seeking over an active stream retires the displaced feed cleanly.
        harness.send(Control::PreviewLoad {
            feed: preview_feed(500, 0.5, 0.5, false),
            play: true,
        });
        harness.render(64);
        assert!(matches!(harness.retired.pop(), Ok(RetiredFeed::Preview(_))));
    }

    #[test]
    fn swapping_tracks_crossfades_to_the_latest_and_retires_the_skipped_ones() {
        let mut harness = Harness::new();
        harness.send(Control::PreviewLoad {
            feed: preview_feed(2000, 0.8, 0.8, false),
            play: true,
        });
        let before = harness.render(480);

        // A held arrow key can land several loads between two callbacks.
        for level in [0.1, 0.2, -0.6] {
            harness.send(Control::PreviewLoad {
                feed: preview_feed(2000, level, level, false),
                play: true,
            });
        }
        let swap = harness.render(960);

        let mut previous = before[before.len() - 2];
        for frame in swap.as_chunks::<2>().0 {
            assert!(
                (frame[0] - previous).abs() < 0.05,
                "step from {previous} to {}",
                frame[0]
            );
            previous = frame[0];
        }
        assert!((swap[swap.len() - 2] + 0.6).abs() < 0.01);

        let mut retired = 0;
        while harness.retired.pop().is_ok() {
            retired += 1;
        }
        assert_eq!(retired, 3, "two skipped tracks and the faded one");
    }

    #[test]
    fn preview_track_that_ends_stops_playing() {
        let mut harness = Harness::new();
        harness.send(Control::PreviewLoad {
            feed: preview_feed(64, 0.5, 0.5, true),
            play: true,
        });

        harness.render(128);
        harness.render(128);

        let clock = harness.published.clock.read();
        assert!(!clock.preview_playing);
        assert!(clock.preview_ended);
        assert_eq!(clock.preview_played_frames, 64);
        assert_eq!(harness.count(&harness.published.counters.underruns), 0);
    }

    #[test]
    fn preview_superseded_short_feed_retires_when_its_buffered_samples_run_out() {
        let mut harness = Harness::new();
        harness.send(Control::PreviewLoad {
            feed: preview_feed(320, 0.8, 0.8, false),
            play: true,
        });
        // Plays 288 of the 320 frames so `current_gain > 0.0` and only 32 frames remain.
        harness.render(288);
        assert!(harness.retired.pop().is_err());

        // Superseding with a new load moves the remaining 32 frames (`finished == false`) into
        // `fading_feed`, which runs dry during the 128-frame callback and retires at once.
        harness.send(Control::PreviewLoad {
            feed: preview_feed(256, 0.4, 0.4, false),
            play: true,
        });
        harness.render(128);
        assert!(matches!(harness.retired.pop(), Ok(RetiredFeed::Preview(_))));
    }
}
