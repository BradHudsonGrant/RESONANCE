//! Background decoding and `rubato` resampling for single-track Library preview playback.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rtrb::Producer;
use rubato::{
    Resampler as _, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::decode::{Decoder, to_stereo};
use crate::error::EngineError;

const PREVIEW_AHEAD_SECONDS: usize = 2;
const RESAMPLE_CHUNK: usize = 1024;
const FULL_WAIT: Duration = Duration::from_millis(5);

/// The audio callback's end of a loaded or sought preview stream, stereo at the device rate.
pub(crate) struct PreviewFeed {
    pub samples: rtrb::Consumer<f32>,
    /// Set once the decoder has pushed the last frame of the preview stream.
    pub finished: Arc<AtomicBool>,
    /// Output-rate frame index where this feed begins.
    pub start_frame: u64,
    /// Load/seek generation this feed belongs to.
    pub generation: u32,
    /// Transport sequence token this feed activates with.
    pub state_seq: u32,
}

pub(crate) struct WorkerRequest {
    pub generation: u32,
    pub path: PathBuf,
    pub start_ms: u64,
    pub output_rate: u32,
    pub producer: Producer<f32>,
    pub finished: Arc<AtomicBool>,
    pub initial_decoder: Option<Decoder>,
}

struct WorkerState {
    latest: Option<WorkerRequest>,
}

struct Shared {
    state: Mutex<WorkerState>,
    wake: Condvar,
    latest_generation: AtomicU32,
    stopped: AtomicBool,
}

impl Shared {
    fn is_superseded(&self, generation: u32) -> bool {
        self.stopped.load(Ordering::Acquire)
            || self.latest_generation.load(Ordering::Acquire) != generation
    }
}

pub(crate) struct PreviewWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl PreviewWorker {
    pub fn spawn() -> Result<Self, EngineError> {
        let shared = Arc::new(Shared {
            state: Mutex::new(WorkerState { latest: None }),
            wake: Condvar::new(),
            latest_generation: AtomicU32::new(0),
            stopped: AtomicBool::new(false),
        });
        let worker_shared = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("preview decoder".to_owned())
            .spawn(move || run_worker(&worker_shared))
            .map_err(EngineError::Thread)?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    pub fn enqueue(&self, request: WorkerRequest) {
        self.shared
            .latest_generation
            .store(request.generation, Ordering::Release);
        let mut guard = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.latest = Some(request);
        self.shared.wake.notify_all();
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.shared.wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) fn buffer_capacity(output_rate: u32) -> usize {
    (PREVIEW_AHEAD_SECONDS * output_rate as usize * 2).max(2048)
}

pub(crate) fn start_frame_for_ms(position_ms: u64, output_rate: u32) -> u64 {
    u64::try_from((u128::from(position_ms) * u128::from(output_rate)) / 1000).unwrap_or(u64::MAX)
}

fn run_worker(shared: &Shared) {
    let mut cached_decoder: Option<(PathBuf, Decoder)> = None;
    let mut cached_resampler: Option<PreviewResampler> = None;
    let mut stereo = Vec::new();
    let mut resampled = Vec::new();

    loop {
        let mut task = {
            let mut guard = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if shared.stopped.load(Ordering::Acquire) {
                    return;
                }
                if let Some(req) = guard.latest.take() {
                    break req;
                }
                guard = shared
                    .wake
                    .wait(guard)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };

        if shared.is_superseded(task.generation) {
            continue;
        }

        if resolve_decoder(&mut cached_decoder, &mut task).is_err() {
            cached_decoder = None;
            task.finished.store(true, Ordering::Release);
            continue;
        }

        if let Some((_, active_dec)) = cached_decoder.as_mut() {
            let resampler = prepare_resampler(
                &mut cached_resampler,
                active_dec.sample_rate,
                task.output_rate,
            );
            stream_task(
                shared,
                task,
                active_dec,
                resampler,
                &mut stereo,
                &mut resampled,
            );
        }
    }
}

fn resolve_decoder(
    cached: &mut Option<(PathBuf, Decoder)>,
    task: &mut WorkerRequest,
) -> Result<(), EngineError> {
    if let Some((cached_path, decoder)) = cached.as_mut()
        && *cached_path == task.path
        && task.initial_decoder.is_none()
    {
        return decoder.seek_ms(task.start_ms);
    }

    // Use the pre-opened decoder when available, then seek if starting non-zero.
    *cached = None;
    let mut decoder = match task.initial_decoder.take() {
        Some(decoder) => decoder,
        None => Decoder::open(&task.path)?,
    };
    if task.start_ms > 0 {
        decoder.seek_ms(task.start_ms)?;
    }
    *cached = Some((task.path.clone(), decoder));
    Ok(())
}

fn prepare_resampler(
    cached: &mut Option<PreviewResampler>,
    input_rate: u32,
    output_rate: u32,
) -> &mut PreviewResampler {
    let matches = cached
        .as_ref()
        .is_some_and(|r| r.input_rate == input_rate && r.output_rate == output_rate);
    if !matches {
        *cached = None;
    }
    let resampler = cached.get_or_insert_with(|| PreviewResampler::new(input_rate, output_rate));
    resampler.reset();
    resampler
}

fn stream_task(
    shared: &Shared,
    mut task: WorkerRequest,
    decoder: &mut Decoder,
    resampler: &mut PreviewResampler,
    stereo: &mut Vec<f32>,
    resampled: &mut Vec<f32>,
) {
    let channels = decoder.channels;
    while !shared.is_superseded(task.generation) {
        let Some(samples) = decoder.next_packet() else {
            resampled.clear();
            resampler.finish(resampled);
            let _ = push_samples(
                shared,
                task.generation,
                &mut task.producer,
                resampled.as_slice(),
            );
            if !shared.is_superseded(task.generation) {
                task.finished.store(true, Ordering::Release);
            }
            return;
        };
        to_stereo(samples, channels, stereo);
        resampled.clear();
        resampler.push_interleaved(stereo, resampled);
        if !push_samples(
            shared,
            task.generation,
            &mut task.producer,
            resampled.as_slice(),
        ) {
            return;
        }
    }
}

fn push_samples(
    shared: &Shared,
    generation: u32,
    producer: &mut Producer<f32>,
    mut pending: &[f32],
) -> bool {
    while !pending.is_empty() {
        if shared.is_superseded(generation) || producer.is_abandoned() {
            return false;
        }
        let room = producer.slots().min(pending.len()) & !1;
        if room == 0 {
            thread::sleep(FULL_WAIT);
            continue;
        }
        let (pushed, _) = producer.push_partial_slice(&pending[..room]);
        pending = &pending[pushed.len()..];
    }
    true
}

/// Stereo preview resampler backed by `rubato::SincFixedIn` when sample rates differ.
pub(crate) struct PreviewResampler {
    pub input_rate: u32,
    pub output_rate: u32,
    ratio: f64,
    sinc: Option<SincFixedIn<f32>>,
    in_channels: [Vec<f32>; 2],
    out_channels: [Vec<f32>; 2],
    delay_to_skip: usize,
    input_frames_fed: u64,
    output_frames_emitted: u64,
}

impl PreviewResampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        let safe_in = input_rate.max(1);
        let safe_out = output_rate.max(1);
        let ratio = f64::from(safe_out) / f64::from(safe_in);
        if safe_in == safe_out {
            return Self {
                input_rate: safe_in,
                output_rate: safe_out,
                ratio: 1.0,
                sinc: None,
                in_channels: [Vec::new(), Vec::new()],
                out_channels: [Vec::new(), Vec::new()],
                delay_to_skip: 0,
                input_frames_fed: 0,
                output_frames_emitted: 0,
            };
        }

        let params = SincInterpolationParameters {
            sinc_len: 128,
            f_cutoff: 0.95,
            oversampling_factor: 128,
            interpolation: SincInterpolationType::Cubic,
            window: WindowFunction::BlackmanHarris2,
        };
        let sinc = SincFixedIn::<f32>::new(ratio, 1.0, params, RESAMPLE_CHUNK, 2)
            .expect("fixed preview resampler parameters are valid");
        let max_out = sinc.output_frames_max();
        let delay = sinc.output_delay();
        Self {
            input_rate: safe_in,
            output_rate: safe_out,
            ratio,
            sinc: Some(sinc),
            in_channels: [
                Vec::with_capacity(RESAMPLE_CHUNK * 2),
                Vec::with_capacity(RESAMPLE_CHUNK * 2),
            ],
            out_channels: [vec![0.0; max_out], vec![0.0; max_out]],
            delay_to_skip: delay,
            input_frames_fed: 0,
            output_frames_emitted: 0,
        }
    }

    pub fn reset(&mut self) {
        self.in_channels[0].clear();
        self.in_channels[1].clear();
        self.input_frames_fed = 0;
        self.output_frames_emitted = 0;
        if let Some(sinc) = &mut self.sinc {
            sinc.reset();
            self.delay_to_skip = sinc.output_delay();
        } else {
            self.delay_to_skip = 0;
        }
    }

    pub fn push_interleaved(&mut self, stereo: &[f32], interleaved_out: &mut Vec<f32>) {
        if self.sinc.is_none() {
            interleaved_out.extend_from_slice(stereo);
            return;
        }
        let frames = stereo.len() / 2;
        self.input_frames_fed += frames as u64;
        let (pairs, _) = stereo.as_chunks::<2>();
        for &[left, right] in pairs {
            self.in_channels[0].push(left);
            self.in_channels[1].push(right);
        }

        let Some(sinc) = &mut self.sinc else {
            return;
        };
        while self.in_channels[0].len() >= RESAMPLE_CHUNK {
            let input_slices = [
                &self.in_channels[0][..RESAMPLE_CHUNK],
                &self.in_channels[1][..RESAMPLE_CHUNK],
            ];
            let Ok((_, out_frames)) =
                sinc.process_into_buffer(&input_slices, &mut self.out_channels, None)
            else {
                break;
            };
            self.in_channels[0].drain(..RESAMPLE_CHUNK);
            self.in_channels[1].drain(..RESAMPLE_CHUNK);
            Self::emit_frames(
                &self.out_channels,
                out_frames,
                &mut self.delay_to_skip,
                &mut self.output_frames_emitted,
                u64::MAX,
                interleaved_out,
            );
        }
    }

    pub fn finish(&mut self, interleaved_out: &mut Vec<f32>) {
        let Some(sinc) = &mut self.sinc else {
            return;
        };
        let expected_total_out = (self.input_frames_fed as f64 * self.ratio).round() as u64;
        if !self.in_channels[0].is_empty() {
            let partial = [
                self.in_channels[0].as_slice(),
                self.in_channels[1].as_slice(),
            ];
            if let Ok((_, out_frames)) =
                sinc.process_partial_into_buffer(Some(&partial), &mut self.out_channels, None)
            {
                Self::emit_frames(
                    &self.out_channels,
                    out_frames,
                    &mut self.delay_to_skip,
                    &mut self.output_frames_emitted,
                    expected_total_out,
                    interleaved_out,
                );
            }
            self.in_channels[0].clear();
            self.in_channels[1].clear();
        }
        while self.output_frames_emitted < expected_total_out {
            let Ok((_, out_frames)) =
                sinc.process_partial_into_buffer::<&[f32], _>(None, &mut self.out_channels, None)
            else {
                break;
            };
            if out_frames == 0 {
                break;
            }
            Self::emit_frames(
                &self.out_channels,
                out_frames,
                &mut self.delay_to_skip,
                &mut self.output_frames_emitted,
                expected_total_out,
                interleaved_out,
            );
        }
    }

    fn emit_frames(
        out_channels: &[Vec<f32>; 2],
        out_frames: usize,
        delay_to_skip: &mut usize,
        output_frames_emitted: &mut u64,
        expected_total_out: u64,
        interleaved_out: &mut Vec<f32>,
    ) {
        let skipped = (*delay_to_skip).min(out_frames);
        *delay_to_skip -= skipped;
        let remaining_cap = expected_total_out.saturating_sub(*output_frames_emitted) as usize;
        let emit_end = skipped + (out_frames - skipped).min(remaining_cap);
        for index in skipped..emit_end {
            interleaved_out.push(out_channels[0][index]);
            interleaved_out.push(out_channels[1][index]);
        }
        *output_frames_emitted += (emit_end - skipped) as u64;
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use resonance_fixtures::signal::Signal;
    use resonance_fixtures::silence::{write_silent_m4a, write_silent_mp3};

    use super::*;

    fn test_file(name: &str) -> PathBuf {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-tmp/engine-preview");
        fs::create_dir_all(&folder).unwrap();
        folder.join(name)
    }

    fn decode_all_at_rate(path: &Path, start_ms: u64, output_rate: u32) -> Vec<f32> {
        let mut decoder = Decoder::open(path).unwrap();
        if start_ms > 0 {
            decoder.seek_ms(start_ms).unwrap();
        }
        let channels = decoder.channels;
        let mut resampler = PreviewResampler::new(decoder.sample_rate, output_rate);
        let mut stereo = Vec::new();
        let mut out = Vec::new();
        while let Some(samples) = decoder.next_packet() {
            to_stereo(samples, channels, &mut stereo);
            resampler.push_interleaved(&stereo, &mut out);
        }
        resampler.finish(&mut out);
        out
    }

    #[test]
    fn resamples_44100_to_48000_accurately_without_leading_silence() {
        let path = test_file("tone-44100.wav");
        Signal::tone(440.0, 1.0, -6.0, 44_100)
            .write_wav(&path)
            .unwrap();

        let out = decode_all_at_rate(&path, 0, 48_000);
        let frames = out.len() / 2;
        assert!((frames as i64 - 48_000).abs() <= 2);
        // Active signal reaches amplitude cleanly right near start once sine begins rising.
        let peak_first_5ms = out[..480].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak_first_5ms > 0.2, "peak_first_5ms={peak_first_5ms}");
    }

    #[test]
    fn seeks_sample_accurately_to_second_half_of_two_tone_track() {
        let path = test_file("two-tone.wav");
        // First 1s is 220 Hz, second 1s is 880 Hz (8x cycles in 20ms vs 2x -> clear pitch shift).
        let mut samples = Vec::with_capacity(44_100 * 2);
        let amp = 0.5_f32;
        for i in 0..44_100 {
            let t = i as f32 / 44_100.0;
            samples.push(amp * (std::f32::consts::TAU * 220.0 * t).sin());
        }
        for i in 0..44_100 {
            let t = i as f32 / 44_100.0;
            samples.push(amp * (std::f32::consts::TAU * 880.0 * t).sin());
        }
        Signal {
            sample_rate: 44_100,
            channels: 1,
            samples,
        }
        .write_wav(&path)
        .unwrap();

        let out = decode_all_at_rate(&path, 1_000, 44_100);
        let frames = out.len() / 2;
        assert!((frames as i64 - 44_100).abs() <= 2, "frames={frames}");

        // Measure zero-crossings over the first 100ms (880 Hz -> ~176 crossings; 220 Hz -> ~44).
        let crossings = out[..4_410 * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|frame| frame[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| (pair[0] <= 0.0 && pair[1] > 0.0) || (pair[0] >= 0.0 && pair[1] < 0.0))
            .count();
        assert!(
            crossings > 140,
            "expected ~880 Hz, got {crossings} crossings"
        );
    }

    #[test]
    fn a_flood_of_loads_decodes_only_the_latest() {
        let path = test_file("flood.wav");
        Signal::tone(440.0, 0.5, -6.0, 48_000)
            .write_wav(&path)
            .unwrap();

        let worker = PreviewWorker::spawn().unwrap();
        let mut feeds = Vec::new();
        for generation in 1..=30 {
            let (producer, samples) = rtrb::RingBuffer::new(buffer_capacity(48_000));
            let finished = Arc::new(AtomicBool::new(false));
            worker.enqueue(WorkerRequest {
                generation,
                path: path.clone(),
                start_ms: 0,
                output_rate: 48_000,
                producer,
                finished: Arc::clone(&finished),
                initial_decoder: None,
            });
            feeds.push((samples, finished));
        }

        let (latest, latest_finished) = &feeds[feeds.len() - 1];
        let started = std::time::Instant::now();
        while !latest_finished.load(Ordering::Acquire) {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "latest never finished"
            );
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(latest.slots(), 48_000);

        let skipped = &feeds[..feeds.len() - 1];
        assert!(
            skipped
                .iter()
                .all(|(_, finished)| !finished.load(Ordering::Acquire))
        );
        let decoded = skipped
            .iter()
            .filter(|(samples, _)| samples.slots() > 0)
            .count();
        assert!(
            decoded <= 1,
            "{decoded} superseded loads were decoded after a newer one arrived"
        );
    }

    #[test]
    fn decodes_flac_aiff_mp3_and_id3_prefixed_m4a_through_preview_pipeline() {
        let flac = test_file("fixture.flac");
        let aiff = test_file("fixture.aiff");
        let mp3 = test_file("fixture.mp3");
        let raw_m4a = test_file("fixture-raw.m4a");
        let id3_m4a = test_file("fixture-id3.m4a");

        Signal::tone(330.0, 0.5, -6.0, 44_100)
            .write_flac(&flac)
            .unwrap();
        Signal::tone(330.0, 0.5, -6.0, 44_100)
            .write_aiff(&aiff)
            .unwrap();
        write_silent_mp3(&mp3, 1.0).unwrap();
        write_silent_m4a(&raw_m4a, 1.0).unwrap();

        let raw_bytes = fs::read(&raw_m4a).unwrap();
        let mut wrapped = File::create(&id3_m4a).unwrap();
        wrapped
            .write_all(b"ID3\x03\x00\x00\x00\x00\x00\x04TEST")
            .unwrap();
        wrapped.write_all(&raw_bytes).unwrap();

        assert!(!decode_all_at_rate(&flac, 0, 48_000).is_empty());
        assert!(!decode_all_at_rate(&aiff, 0, 48_000).is_empty());
        assert!(!decode_all_at_rate(&mp3, 0, 48_000).is_empty());
        assert!(!decode_all_at_rate(&id3_m4a, 0, 48_000).is_empty());
    }
}
