//! The binary frames that carry the Mix view's real-time data on the socket: meters, playheads,
//! the audio thread's counts and each deck's waveform. Commands stay JSON text frames, and none
//! of this goes through JSON.
//!
//! Numbers are little-endian. The page reads these frames in `ui/src/lib/mix/frames.ts`, which
//! has to match the offsets here.
//!
//! A telemetry frame is 144 bytes:
//!
//! | Offset | Type | Field |
//! | --- | --- | --- |
//! | 0 | u8 | kind, 1 |
//! | 4 | u32 | device sample rate |
//! | 8 | u32 | buffer frames asked for |
//! | 12 | u32 | device period in frames |
//! | 16 | u32 | device buffer in frames |
//! | 20 | u32 | fewest frames a callback was asked for since the last frame |
//! | 24 | u32 | most frames a callback was asked for since the last frame |
//! | 28 | u32 | longest callback since the last frame, in nanoseconds |
//! | 32 | u32 | callbacks |
//! | 36 | u32 | underruns |
//! | 40 | u32 | frames missed in underruns |
//! | 44 | u32 | late callbacks |
//! | 48 | u32 | starved callbacks |
//! | 52 | u32 | stream errors |
//! | 56 | 32 bytes | deck A |
//! | 88 | 32 bytes | deck B |
//! | 120 | 24 bytes | preview channel |
//!
//! Counts are totals since the device opened, wrapping at 2^32. Each deck is:
//!
//! | Offset | Type | Field |
//! | --- | --- | --- |
//! | 0 | f64 | position being heard, in seconds |
//! | 8 | f64 | duration in seconds |
//! | 16 | f32 | left meter, 0 to 1 |
//! | 20 | f32 | right meter, 0 to 1 |
//! | 24 | u32 | generation, 0 for an empty deck |
//! | 28 | u8 | 1 while playing |
//!
//! The preview channel is 24 bytes:
//!
//! | Offset | Type | Field |
//! | --- | --- | --- |
//! | 0 | f64 | position being heard, in seconds |
//! | 8 | f64 | duration, in seconds |
//! | 16 | u32 | generation, 0 when unloaded |
//! | 20 | u8 | 1 while playing |
//!
//! A waveform frame is a 16-byte header and then a peak and an RMS byte for each bin:
//!
//! | Offset | Type | Field |
//! | --- | --- | --- |
//! | 0 | u8 | kind, 2 |
//! | 1 | u8 | deck, 0 for A and 1 for B |
//! | 4 | u32 | generation |
//! | 8 | u32 | bins per second |
//! | 12 | u32 | bins |

use resonance_core::waveform::{BINS_PER_SECOND, Waveform};
use resonance_engine::{DeckId, DeckTelemetry, PreviewTelemetry, Telemetry};

pub const TELEMETRY: u8 = 1;
pub const WAVEFORM: u8 = 2;
pub const TELEMETRY_LENGTH: usize = 144;
const DECKS_AT: usize = 56;
const DECK_LENGTH: usize = 32;
const PREVIEW_AT: usize = 120;
const WAVEFORM_HEADER_LENGTH: usize = 16;

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn new(kind: u8, length: usize) -> Self {
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(&[kind, 0, 0, 0]);
        Self { bytes }
    }

    fn u8(&mut self, value: u8) -> &mut Self {
        self.bytes.push(value);
        self
    }

    fn u32(&mut self, value: u32) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    /// Counts travel as 32 bits so the page reads them as plain numbers.
    fn count(&mut self, value: u64) -> &mut Self {
        self.u32(value as u32)
    }

    fn f32(&mut self, value: f32) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    fn f64(&mut self, value: f64) -> &mut Self {
        self.bytes.extend_from_slice(&value.to_le_bytes());
        self
    }

    fn pad_to(&mut self, length: usize) -> &mut Self {
        self.bytes.resize(length, 0);
        self
    }
}

pub fn telemetry(telemetry: &Telemetry) -> Vec<u8> {
    let engine = &telemetry.engine;
    let mut frame = Writer::new(TELEMETRY, TELEMETRY_LENGTH);
    frame
        .u32(engine.sample_rate)
        .u32(engine.requested_buffer_frames)
        .u32(engine.period_frames)
        .u32(engine.buffer_frames)
        .u32(engine.window_min_frames)
        .u32(engine.window_max_frames)
        .u32(u32::try_from(engine.window_max_callback_ns).unwrap_or(u32::MAX))
        .count(engine.callbacks)
        .count(engine.underruns)
        .count(engine.missing_frames)
        .count(engine.late_callbacks)
        .count(engine.starved_callbacks)
        .count(engine.stream_errors);
    for (index, deck) in telemetry.decks.iter().enumerate() {
        frame.pad_to(DECKS_AT + index * DECK_LENGTH);
        write_deck(&mut frame, deck);
    }
    frame.pad_to(PREVIEW_AT);
    write_preview(&mut frame, &telemetry.preview);
    frame.pad_to(TELEMETRY_LENGTH);
    frame.bytes
}

fn write_preview(frame: &mut Writer, preview: &PreviewTelemetry) {
    frame
        .f64(preview.position_seconds)
        .f64(preview.duration_seconds)
        .u32(preview.generation)
        .u8(u8::from(preview.playing));
}

fn write_deck(frame: &mut Writer, deck: &DeckTelemetry) {
    frame
        .f64(deck.position_seconds)
        .f64(deck.duration_seconds)
        .f32(deck.meter[0])
        .f32(deck.meter[1])
        .u32(deck.generation)
        .u8(u8::from(deck.playing));
}

pub fn waveform(deck: DeckId, generation: u32, waveform: &Waveform) -> Vec<u8> {
    let bins = waveform.bins.len();
    let mut frame = Writer::new(WAVEFORM, WAVEFORM_HEADER_LENGTH + 2 * bins);
    frame.bytes[1] = deck.index() as u8;
    frame
        .u32(generation)
        .u32(BINS_PER_SECOND)
        .u32(u32::try_from(bins).unwrap_or(u32::MAX));
    for bin in &waveform.bins {
        frame.u8(bin.peak).u8(bin.rms);
    }
    frame.bytes
}

#[cfg(test)]
mod tests {
    use resonance_core::waveform::Bin;
    use resonance_engine::EngineTelemetry;

    use super::*;

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn f32_at(bytes: &[u8], at: usize) -> f32 {
        f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn f64_at(bytes: &[u8], at: usize) -> f64 {
        f64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }

    #[test]
    fn a_telemetry_frame_puts_every_field_at_its_documented_offset() {
        let reading = Telemetry {
            decks: [
                DeckTelemetry {
                    generation: 3,
                    playing: true,
                    position_seconds: 61.25,
                    duration_seconds: 300.5,
                    meter: [0.75, 0.5],
                },
                DeckTelemetry {
                    generation: 4,
                    playing: false,
                    position_seconds: 0.0,
                    duration_seconds: 200.0,
                    meter: [0.0, 0.25],
                },
            ],
            preview: PreviewTelemetry {
                generation: 9,
                playing: true,
                position_seconds: 12.5,
                duration_seconds: 245.0,
            },
            engine: EngineTelemetry {
                sample_rate: 48_000,
                requested_buffer_frames: 256,
                period_frames: 480,
                buffer_frames: 1056,
                callbacks: 1000,
                underruns: 2,
                missing_frames: 300,
                late_callbacks: 5,
                starved_callbacks: 1,
                stream_errors: 0,
                window_min_frames: 470,
                window_max_frames: 490,
                window_max_callback_ns: 51_000,
            },
        };

        let frame = telemetry(&reading);

        assert_eq!(frame.len(), TELEMETRY_LENGTH);
        assert_eq!(frame[0], TELEMETRY);
        let header: Vec<u32> = (1..14).map(|field| u32_at(&frame, field * 4)).collect();
        assert_eq!(
            header,
            [
                48_000, 256, 480, 1056, 470, 490, 51_000, 1000, 2, 300, 5, 1, 0
            ]
        );
        assert_eq!(f64_at(&frame, 56), 61.25);
        assert_eq!(f64_at(&frame, 64), 300.5);
        assert_eq!(f32_at(&frame, 72), 0.75);
        assert_eq!(f32_at(&frame, 76), 0.5);
        assert_eq!(u32_at(&frame, 80), 3);
        assert_eq!(frame[84], 1);
        assert_eq!(f64_at(&frame, 96), 200.0);
        assert_eq!(f32_at(&frame, 108), 0.25);
        assert_eq!(u32_at(&frame, 112), 4);
        assert_eq!(frame[116], 0);
        assert_eq!(f64_at(&frame, 120), 12.5);
        assert_eq!(f64_at(&frame, 128), 245.0);
        assert_eq!(u32_at(&frame, 136), 9);
        assert_eq!(frame[140], 1);
    }

    #[test]
    fn a_waveform_frame_carries_its_deck_generation_and_bins_in_order() {
        let waveform = Waveform {
            bins: vec![Bin { peak: 10, rms: 5 }, Bin { peak: 255, rms: 90 }],
            frames: 300,
            sample_rate: 44_100,
        };

        let frame = super::waveform(DeckId::B, 7, &waveform);

        assert_eq!(frame[..2], [WAVEFORM, 1]);
        assert_eq!(u32_at(&frame, 4), 7);
        assert_eq!(u32_at(&frame, 8), BINS_PER_SECOND);
        assert_eq!(u32_at(&frame, 12), 2);
        assert_eq!(frame[16..], [10, 5, 255, 90]);
    }
}
