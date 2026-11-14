//! Reading a file into samples. This runs on worker threads, never in the audio callback.

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::Duration;

use resonance_core::waveform::{Summariser, Waveform};
use rtrb::{Producer, RingBuffer};
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as DecodeError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::error::EngineError;

/// Seconds of decoded audio a deck keeps ahead of the callback. A decoder that stalls for less
/// than this is never heard.
const AHEAD_SECONDS: usize = 2;

/// How long the decoder sleeps when the ring buffer is full. Far shorter than the audio held.
const FULL_WAIT: Duration = Duration::from_millis(5);

/// An open file and its decoder, giving interleaved `f32` samples a packet at a time.
pub(crate) struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    pub sample_rate: u32,
    pub channels: usize,
    /// Frames in the track, when the container says.
    pub frames: Option<u64>,
    packet: Vec<f32>,
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self, EngineError> {
        let unreadable = |reason: String| EngineError::Unreadable {
            path: path.to_owned(),
            reason,
        };
        let file = File::open(path).map_err(|error| unreadable(error.to_string()))?;
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            hint.with_extension(extension);
        }
        let format = symphonia::default::get_probe()
            .probe(
                &hint,
                MediaSourceStream::new(Box::new(file), Default::default()),
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .map_err(|error| unreadable(error.to_string()))?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| unreadable("it has no audio track".to_owned()))?;
        let parameters = track
            .codec_params
            .as_ref()
            .and_then(|parameters| parameters.audio())
            .ok_or_else(|| unreadable("its audio track has no codec parameters".to_owned()))?
            .clone();
        let sample_rate = parameters
            .sample_rate
            .ok_or_else(|| unreadable("its sample rate is not stated".to_owned()))?;
        let channels = parameters
            .channels
            .as_ref()
            .map(|channels| channels.count())
            .filter(|count| *count > 0)
            .ok_or_else(|| unreadable("its channels are not stated".to_owned()))?;
        let track_id = track.id;
        let frames = track.num_frames;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&parameters, &AudioDecoderOptions::default())
            .map_err(|error| unreadable(error.to_string()))?;
        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            frames,
            packet: Vec::new(),
        })
    }

    /// The next packet's samples, interleaved, or `None` at the end of the track. A damaged packet
    /// is skipped, as a player would; any other failure ends the track.
    pub fn next_packet(&mut self) -> Option<&[f32]> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) | Err(_) => return None,
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(audio) => {
                    audio.copy_to_vec_interleaved(&mut self.packet);
                    return Some(&self.packet);
                }
                Err(DecodeError::DecodeError(_)) => continue,
                Err(_) => return None,
            }
        }
    }
}

/// The audio side's end of a deck's stream: stereo frames at the device's rate.
pub(crate) struct DeckFeed {
    pub samples: rtrb::Consumer<f32>,
    /// Set once the decoder has pushed the last frame of the track.
    pub finished: Arc<AtomicBool>,
}

/// Starts a thread that decodes `decoder` into a ring buffer at `output_rate`, and returns the
/// buffer's reading end. The thread ends at the end of the track, or as soon as the reading end
/// is dropped.
pub(crate) fn feed_deck(decoder: Decoder, output_rate: u32) -> Result<DeckFeed, EngineError> {
    let capacity = AHEAD_SECONDS * output_rate as usize * 2;
    let (producer, samples) = RingBuffer::new(capacity);
    let finished = Arc::new(AtomicBool::new(false));
    let done = Arc::clone(&finished);
    thread::Builder::new()
        .name("deck decoder".to_owned())
        .spawn(move || feed(decoder, output_rate, producer, &done))
        .map_err(EngineError::Thread)?;
    Ok(DeckFeed { samples, finished })
}

fn feed(mut decoder: Decoder, output_rate: u32, mut producer: Producer<f32>, done: &AtomicBool) {
    let mut resampler = Resampler::new(decoder.sample_rate, output_rate);
    let channels = decoder.channels;
    let mut stereo = Vec::new();
    let mut resampled = Vec::new();
    while let Some(samples) = decoder.next_packet() {
        to_stereo(samples, channels, &mut stereo);
        resampler.process(&stereo, &mut resampled);
        let mut pending = resampled.as_slice();
        while !pending.is_empty() {
            if producer.is_abandoned() {
                return;
            }
            // Whole frames only, so the callback never reads half of one.
            let room = producer.slots().min(pending.len()) & !1;
            if room == 0 {
                thread::sleep(FULL_WAIT);
                continue;
            }
            let (pushed, _) = producer.push_partial_slice(&pending[..room]);
            pending = &pending[pushed.len()..];
        }
    }
    done.store(true, Ordering::Release);
}

/// Mono is copied to both sides; past two channels, the first two are kept.
fn to_stereo(samples: &[f32], channels: usize, stereo: &mut Vec<f32>) {
    stereo.clear();
    for frame in samples.chunks_exact(channels) {
        let left = frame[0];
        let right = frame.get(1).copied().unwrap_or(left);
        stereo.push(left);
        stereo.push(right);
    }
}

/// Linear-interpolation resampling of stereo frames, carried across packets.
struct Resampler {
    /// Input frames per output frame.
    step: f64,
    /// Where the next output frame falls, in input frames, counting the last frame of the
    /// previous packet as 0.
    position: f64,
    previous: [f32; 2],
}

impl Resampler {
    fn new(input_rate: u32, output_rate: u32) -> Self {
        Self {
            step: f64::from(input_rate) / f64::from(output_rate.max(1)),
            position: 1.0,
            previous: [0.0; 2],
        }
    }

    fn process(&mut self, input: &[f32], output: &mut Vec<f32>) {
        output.clear();
        if self.step == 1.0 {
            output.extend_from_slice(input);
            return;
        }
        let frames = input.len() / 2;
        let frame = |index: usize| -> [f32; 2] {
            if index == 0 {
                self.previous
            } else {
                [input[2 * index - 2], input[2 * index - 1]]
            }
        };
        while self.position < frames as f64 {
            let index = self.position as usize;
            let fraction = (self.position - index as f64) as f32;
            let [left_a, right_a] = frame(index);
            let [left_b, right_b] = frame(index + 1);
            output.push(left_a + (left_b - left_a) * fraction);
            output.push(right_a + (right_b - right_a) * fraction);
            self.position += self.step;
        }
        if frames > 0 {
            self.position -= frames as f64;
            self.previous = [input[2 * frames - 2], input[2 * frames - 1]];
        }
    }
}

/// Decodes the whole file on its own thread and leaves its waveform in the returned cell. The
/// cell stays empty if the file stops decoding before the end of the first packet.
pub(crate) fn summarise(path: &Path) -> Result<Arc<OnceLock<Arc<Waveform>>>, EngineError> {
    let mut decoder = Decoder::open(path)?;
    let cell = Arc::new(OnceLock::new());
    let ready = Arc::clone(&cell);
    thread::Builder::new()
        .name("deck waveform".to_owned())
        .spawn(move || {
            let mut summariser = Summariser::new(decoder.sample_rate, decoder.channels);
            let channels = decoder.channels;
            while let Some(samples) = decoder.next_packet() {
                let whole = samples.len() - samples.len() % channels;
                summariser.push(&samples[..whole]);
            }
            let waveform = summariser.finish();
            if waveform.frames > 0 {
                let _ = ready.set(Arc::new(waveform));
            }
        })
        .map_err(EngineError::Thread)?;
    Ok(cell)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::Instant;

    use resonance_core::waveform::BINS_PER_SECOND;
    use resonance_fixtures::signal::Signal;

    use super::*;

    fn test_file(name: &str) -> PathBuf {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp/engine-decode");
        fs::create_dir_all(&folder).unwrap();
        folder.join(name)
    }

    fn tone_file(name: &str, seconds: f64, sample_rate: u32) -> PathBuf {
        let path = test_file(name);
        Signal::tone(440.0, seconds, -6.0, sample_rate)
            .write_wav(&path)
            .unwrap();
        path
    }

    fn wait_for<T>(mut ready: impl FnMut() -> Option<T>) -> T {
        let started = Instant::now();
        loop {
            if let Some(value) = ready() {
                return value;
            }
            assert!(started.elapsed() < Duration::from_secs(10), "timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_file_decodes_through_its_feed_to_every_frame_at_the_output_rate() {
        let path = tone_file("two-seconds.wav", 2.0, 44_100);

        let mut feed = feed_deck(Decoder::open(&path).unwrap(), 48_000).unwrap();
        let mut samples = 0;
        wait_for(|| {
            samples += feed
                .samples
                .read_chunk(feed.samples.slots())
                .map_or(0, |chunk| {
                    let read = chunk.len();
                    chunk.commit_all();
                    read
                });
            (feed.finished.load(Ordering::Acquire) && feed.samples.is_empty()).then_some(())
        });

        assert!((samples as i64 / 2 - 96_000).abs() <= 1);
    }

    #[test]
    fn a_mono_file_plays_on_both_sides() {
        let path = tone_file("mono.wav", 0.1, 48_000);

        let mut feed = feed_deck(Decoder::open(&path).unwrap(), 48_000).unwrap();
        wait_for(|| feed.finished.load(Ordering::Acquire).then_some(()));

        let frames = feed.samples.slots() / 2;
        let chunk = feed.samples.read_chunk(frames * 2).unwrap();
        let (first, second) = chunk.as_slices();
        let all: Vec<f32> = first.iter().chain(second).copied().collect();
        assert!(all.chunks(2).all(|frame| frame[0] == frame[1]));
        assert!(all.iter().any(|sample| *sample > 0.4));
    }

    #[test]
    fn the_waveform_of_a_file_covers_every_frame() {
        let path = tone_file("waveform.wav", 3.0, 44_100);

        let cell = summarise(&path).unwrap();
        let waveform = wait_for(|| cell.get().cloned());

        assert_eq!(waveform.frames, 3 * 44_100);
        assert_eq!(waveform.bins.len(), 3 * BINS_PER_SECOND as usize);
        assert_eq!(waveform.bins[100].peak, 128);
    }

    #[test]
    fn a_file_that_is_not_audio_is_refused_with_its_path() {
        let path = test_file("not-audio.mp3");
        fs::write(&path, b"this is not an mp3").unwrap();

        let Err(error) = Decoder::open(&path) else {
            panic!("a text file opened as audio");
        };

        assert!(error.to_string().contains("not-audio.mp3"));
    }

    fn resample(input_rate: u32, output_rate: u32, pieces: &[&[f32]]) -> Vec<f32> {
        let mut resampler = Resampler::new(input_rate, output_rate);
        let mut all = Vec::new();
        let mut output = Vec::new();
        for piece in pieces {
            resampler.process(piece, &mut output);
            all.extend_from_slice(&output);
        }
        all
    }

    #[test]
    fn the_same_rate_passes_samples_through_unchanged() {
        let input = [0.1, 0.2, 0.3, 0.4];

        assert_eq!(resample(48_000, 48_000, &[&input]), input);
    }

    #[test]
    fn halving_the_rate_keeps_every_other_frame() {
        let input: Vec<f32> = (0..8).flat_map(|i| [i as f32, -(i as f32)]).collect();

        assert_eq!(
            resample(2, 1, &[&input]),
            [0.0, -0.0, 2.0, -2.0, 4.0, -4.0, 6.0, -6.0]
        );
    }

    #[test]
    fn doubling_the_rate_puts_a_midpoint_between_frames_across_packets() {
        let first = [0.0, 0.0, 1.0, -1.0];
        let second = [2.0, -2.0];

        let output = resample(1, 2, &[&first, &second]);

        // The last frame comes out with the next packet, once there is a frame to blend it with.
        assert_eq!(output, [0.0, 0.0, 0.5, -0.5, 1.0, -1.0, 1.5, -1.5]);
    }

    #[test]
    fn a_minute_at_44_1_khz_becomes_a_minute_at_48_khz() {
        let second: Vec<f32> = vec![0.25; 44_100 * 2];
        let pieces: Vec<&[f32]> = (0..60).map(|_| second.as_slice()).collect();

        let output = resample(44_100, 48_000, &pieces);

        assert!((output.len() as i64 / 2 - 60 * 48_000).abs() <= 1);
    }

    #[test]
    fn mono_is_sent_to_both_sides_and_extra_channels_are_dropped() {
        let mut stereo = Vec::new();

        to_stereo(&[0.5, 0.25], 1, &mut stereo);
        assert_eq!(stereo, [0.5, 0.5, 0.25, 0.25]);

        to_stereo(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 3, &mut stereo);
        assert_eq!(stereo, [0.1, 0.2, 0.4, 0.5]);
    }
}
