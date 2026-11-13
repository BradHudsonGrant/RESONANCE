//! What the audio callback tells the rest of the program. Everything here is an atomic, so the
//! callback writes without waiting and readers never hold it up.

use std::hint;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};

use crate::DECKS;

/// The audio clock as of the last callback, published as a sequence lock so a reader sees the
/// fields of one callback together. The callback is the only writer and never waits; a reader
/// that lands on a write in progress reads again.
#[derive(Debug, Default)]
pub(crate) struct Clock {
    sequence: AtomicU64,
    anchor_ns: AtomicU64,
    latency_ns: AtomicU64,
    played_frames: [AtomicU64; DECKS],
    playing: [AtomicU32; DECKS],
}

/// One callback's view of the clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ClockReading {
    /// When the callback began, in nanoseconds from the engine's start.
    pub anchor_ns: u64,
    /// How long after `anchor_ns` the first frame of that callback's buffer is heard.
    pub latency_ns: u64,
    /// Frames each deck had played before that buffer, at the device's sample rate.
    pub played_frames: [u64; DECKS],
    pub playing: [bool; DECKS],
}

impl Clock {
    pub fn publish(&self, reading: &ClockReading) {
        let sequence = self.sequence.load(Ordering::Relaxed);
        self.sequence
            .store(sequence.wrapping_add(1), Ordering::Relaxed);
        fence(Ordering::Release);
        self.anchor_ns.store(reading.anchor_ns, Ordering::Relaxed);
        self.latency_ns.store(reading.latency_ns, Ordering::Relaxed);
        for deck in 0..DECKS {
            self.played_frames[deck].store(reading.played_frames[deck], Ordering::Relaxed);
            self.playing[deck].store(u32::from(reading.playing[deck]), Ordering::Relaxed);
        }
        self.sequence
            .store(sequence.wrapping_add(2), Ordering::Release);
    }

    pub fn read(&self) -> ClockReading {
        loop {
            let before = self.sequence.load(Ordering::Acquire);
            if before % 2 == 1 {
                hint::spin_loop();
                continue;
            }
            let mut reading = ClockReading {
                anchor_ns: self.anchor_ns.load(Ordering::Relaxed),
                latency_ns: self.latency_ns.load(Ordering::Relaxed),
                ..ClockReading::default()
            };
            for deck in 0..DECKS {
                reading.played_frames[deck] = self.played_frames[deck].load(Ordering::Relaxed);
                reading.playing[deck] = self.playing[deck].load(Ordering::Relaxed) != 0;
            }
            fence(Ordering::Acquire);
            if self.sequence.load(Ordering::Relaxed) == before {
                return reading;
            }
        }
    }
}

/// Counts kept by the callback. Totals only grow; the `window_` values are taken and reset by
/// the one reader that samples them for the page.
#[derive(Debug)]
pub(crate) struct Counters {
    pub callbacks: AtomicU64,
    /// Callbacks in which a playing deck's ring buffer had less than the callback needed.
    pub underruns: AtomicU64,
    /// Frames of silence those underruns put in place of music.
    pub missing_frames: AtomicU64,
    /// Callbacks that asked for more than one and a half device periods, so the thread ran late.
    pub late_callbacks: AtomicU64,
    /// Callbacks that found the device's buffer empty, which is heard as a dropout.
    pub starved_callbacks: AtomicU64,
    pub stream_errors: AtomicU64,
    /// The device period in frames, known once the stream is open.
    pub period_frames: AtomicU32,
    /// The device's whole buffer in frames, which the first callback is asked to fill.
    pub buffer_frames: AtomicU32,
    pub window_min_frames: AtomicU32,
    pub window_max_frames: AtomicU32,
    pub window_max_cost_ns: AtomicU64,
}

impl Default for Counters {
    fn default() -> Self {
        Self {
            callbacks: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            missing_frames: AtomicU64::new(0),
            late_callbacks: AtomicU64::new(0),
            starved_callbacks: AtomicU64::new(0),
            stream_errors: AtomicU64::new(0),
            period_frames: AtomicU32::new(0),
            buffer_frames: AtomicU32::new(0),
            window_min_frames: AtomicU32::new(u32::MAX),
            window_max_frames: AtomicU32::new(0),
            window_max_cost_ns: AtomicU64::new(0),
        }
    }
}

/// The loudest sample each deck played since the last reading, per channel. Amplitudes are never
/// negative, and non-negative floats order the same way as their bits, so `fetch_max` on the bits
/// keeps the largest.
#[derive(Debug, Default)]
pub(crate) struct Peaks {
    bits: [[AtomicU32; 2]; DECKS],
}

impl Peaks {
    pub fn raise(&self, deck: usize, channel: usize, amplitude: f32) {
        self.bits[deck][channel].fetch_max(amplitude.to_bits(), Ordering::Relaxed);
    }

    pub fn take(&self, deck: usize, channel: usize) -> f32 {
        f32::from_bits(self.bits[deck][channel].swap(0, Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Published {
    pub clock: Clock,
    pub counters: Counters,
    pub peaks: Peaks,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_gives_back_what_was_published() {
        let clock = Clock::default();
        let reading = ClockReading {
            anchor_ns: 5_000_000,
            latency_ns: 21_000_000,
            played_frames: [48_000, 96_000],
            playing: [true, false],
        };

        clock.publish(&reading);

        assert_eq!(clock.read(), reading);
    }

    #[test]
    fn peaks_keep_the_largest_since_the_last_reading_and_reset_when_read() {
        let peaks = Peaks::default();

        peaks.raise(1, 0, 0.25);
        peaks.raise(1, 0, 0.75);
        peaks.raise(1, 0, 0.5);

        assert_eq!(peaks.take(1, 0), 0.75);
        assert_eq!(peaks.take(1, 0), 0.0);
        assert_eq!(peaks.take(0, 0), 0.0);
    }
}
