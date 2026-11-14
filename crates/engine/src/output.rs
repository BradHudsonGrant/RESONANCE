//! The audio device and the callback that fills it.
//!
//! The callback runs on the device's thread. It never allocates, locks, logs or does I/O: decks
//! arrive and leave through ring buffers, samples come from each deck's ring buffer, and what it
//! reports goes out through atomics in [`Published`].

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, OutputCallbackInfo, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::DECKS;
use crate::decode::DeckFeed;
use crate::error::EngineError;
use crate::published::{ClockReading, Published};

/// The buffer the stream asks the device for.
pub const REQUESTED_BUFFER_FRAMES: u32 = 256;

/// Each deck plays 6 dB down, so two tracks mastered to full scale sum to full scale instead of
/// clipping.
const DECK_GAIN: f32 = 0.5;

/// Room for commands on their way to the callback. The queue that brings old feeds back is the
/// same size, so it cannot fill while the control side empties it before each command.
const CONTROL_CAPACITY: usize = 16;

pub(crate) enum Control {
    Load { deck: usize, feed: DeckFeed },
    Play { deck: usize, playing: bool },
}

/// The device as the stream opened it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub requested_buffer_frames: u32,
    /// The period the device runs its callback at, whatever was asked for.
    pub period_frames: u32,
}

/// The open stream. It lives on its own thread, which drops it when this is dropped.
pub(crate) struct Output {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    pub device: Device,
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) struct Opened {
    pub output: Output,
    pub controls: Producer<Control>,
    pub retired: Consumer<DeckFeed>,
}

pub(crate) fn open(published: &Arc<Published>, epoch: Instant) -> Result<Opened, EngineError> {
    let (renderer, controls, retired) = Renderer::new(published, epoch);
    let (ready, opened) = mpsc::sync_channel(1);
    let (stop, stopped) = mpsc::channel::<()>();
    let for_thread = Arc::clone(published);
    let thread = thread::Builder::new()
        .name("audio output".to_owned())
        .spawn(move || match start(renderer, &for_thread) {
            Ok((stream, device)) => {
                let _ = ready.send(Ok(device));
                let _ = stopped.recv();
                drop(stream);
            }
            Err(error) => {
                let _ = ready.send(Err(error));
            }
        })
        .map_err(EngineError::Thread)?;
    let device = opened.recv().map_err(|_| EngineError::OutputGone)??;
    Ok(Opened {
        output: Output {
            stop: Some(stop),
            thread: Some(thread),
            device,
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
    renderer.channels = usize::from(channels.max(1));
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
            requested_buffer_frames: REQUESTED_BUFFER_FRAMES,
            period_frames,
        },
    ))
}

#[derive(Default)]
struct Deck {
    feed: Option<DeckFeed>,
    playing: bool,
    /// Frames played from the feed, at the device's rate.
    played_frames: u64,
}

struct Renderer {
    controls: Consumer<Control>,
    retired: Producer<DeckFeed>,
    decks: [Deck; DECKS],
    published: Arc<Published>,
    channels: usize,
    epoch: Instant,
    first_callback: bool,
}

impl Renderer {
    fn new(
        published: &Arc<Published>,
        epoch: Instant,
    ) -> (Self, Producer<Control>, Consumer<DeckFeed>) {
        let (controls, controls_in_callback) = RingBuffer::new(CONTROL_CAPACITY);
        let (retired_in_callback, retired) = RingBuffer::new(CONTROL_CAPACITY);
        let renderer = Self {
            controls: controls_in_callback,
            retired: retired_in_callback,
            decks: Default::default(),
            published: Arc::clone(published),
            channels: 2,
            epoch,
            first_callback: true,
        };
        (renderer, controls, retired)
    }

    fn render(&mut self, out: &mut [f32], info: &OutputCallbackInfo) {
        let entered = Instant::now();
        let stamp = info.timestamp();
        let latency = stamp.playback.saturating_duration_since(stamp.callback);
        while let Ok(control) = self.controls.pop() {
            self.apply(control);
        }
        out.fill(0.0);
        let frames = out.len() / self.channels;
        let mut reading = ClockReading {
            anchor_ns: nanos(entered.saturating_duration_since(self.epoch)),
            latency_ns: nanos(latency),
            ..ClockReading::default()
        };
        for (index, deck) in self.decks.iter_mut().enumerate() {
            reading.played_frames[index] = deck.played_frames;
            reading.playing[index] = deck.playing;
            deck.render(index, out, self.channels, frames, &self.published);
        }
        self.published.clock.publish(&reading);
        self.count(u32::try_from(frames).unwrap_or(u32::MAX), entered);
    }

    fn apply(&mut self, control: Control) {
        match control {
            Control::Load { deck, feed } => {
                let deck = &mut self.decks[deck];
                if let Some(old) = deck.feed.replace(feed) {
                    // Freeing a feed would free memory here, so it goes back to be dropped
                    // elsewhere. The queue has room: see CONTROL_CAPACITY.
                    let _ = self.retired.push(old);
                }
                deck.playing = false;
                deck.played_frames = 0;
            }
            Control::Play { deck, playing } => {
                let deck = &mut self.decks[deck];
                deck.playing = playing && deck.feed.is_some();
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
        } else {
            let buffer = counters.buffer_frames.load(Ordering::Relaxed);
            let period = counters.period_frames.load(Ordering::Relaxed);
            if buffer > 0 && frames >= buffer {
                counters.starved_callbacks.fetch_add(1, Ordering::Relaxed);
            } else if period > 0 && u64::from(frames) * 2 > u64::from(period) * 3 {
                counters.late_callbacks.fetch_add(1, Ordering::Relaxed);
            }
        }
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
                let at = frame * channels;
                if channels == 1 {
                    out[at] += 0.5 * DECK_GAIN * (left + right);
                } else {
                    out[at] += DECK_GAIN * left;
                    out[at + 1] += DECK_GAIN * right;
                }
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

    struct Harness {
        renderer: Renderer,
        controls: Producer<Control>,
        retired: Consumer<DeckFeed>,
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
            self.send(Control::Load { deck, feed });
            self.send(Control::Play {
                deck,
                playing: true,
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
}
