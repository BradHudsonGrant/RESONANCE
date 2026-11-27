//! The front end every spectral analyser reads: the decoded audio as mono at 44.1 kHz, a
//! short-time Fourier transform, and that spectrum split into its harmonic and percussive parts
//! by median filtering (Fitzgerald, "Harmonic/percussive separation using median filtering",
//! DAFx 2010).
//!
//! Sustained notes are smooth along time and spiky across frequency, so a median across time
//! keeps them and a median across frequency loses them. Hits are the other way round. Key and
//! tuning read the harmonic part so drums do not smear the chroma; beat and swing read the
//! percussive part so pads and sweeps do not trigger onsets.
//!
//! Frames stream out as soon as their medians can be taken, eight frames behind the input, so a
//! whole track's spectrogram is never held in memory.

use std::sync::Arc;

use realfft::{RealFftPlanner, RealToComplex};
use rubato::{FftFixedIn, Resampler};

use crate::AnalysisError;

/// The rate the front end resamples to.
pub const SAMPLE_RATE: u32 = 44_100;
/// Samples in one Hann window: 46.4 ms.
pub const FFT_SIZE: usize = 2_048;
/// Samples between frames: 11.6 ms, so 86.13 frames a second.
pub const HOP: usize = 512;
/// Magnitude bins in a frame, from DC to Nyquist.
pub const SPECTRUM_BINS: usize = FFT_SIZE / 2 + 1;
/// Frames in the time median (197 ms) and bins in the frequency median (366 Hz).
pub const MEDIAN_SPAN: usize = 17;
const HALF_SPAN: usize = MEDIAN_SPAN / 2;
/// Input frames the resampler takes at a time.
const RESAMPLE_CHUNK: usize = 1_024;
/// Keeps a silent bin from dividing by zero in the masks.
const MASK_FLOOR: f32 = 1e-12;

pub const FRAMES_PER_SECOND: f64 = SAMPLE_RATE as f64 / HOP as f64;

/// One frame of the spectrum and its two parts. `harmonic` and `percussive` are the magnitude
/// under Wiener soft masks (power 2), so at each bin they add up to no more than `magnitude`.
#[derive(Debug, Clone, Copy)]
pub struct SeparatedFrame<'a> {
    /// Counted from the start of the track. Frame `n` starts at sample `n * HOP` at 44.1 kHz.
    pub index: u64,
    pub magnitude: &'a [f32],
    pub harmonic: &'a [f32],
    pub percussive: &'a [f32],
}

/// What the whole pass measured.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrontEndSummary {
    pub frames: u64,
    /// Sums of squared magnitudes over every frame and bin.
    pub harmonic_energy: f64,
    pub percussive_energy: f64,
    /// Sums of squared `(L + R) / 2` and `(L - R) / 2` over every sample, at the source rate.
    pub mid_energy: f64,
    pub side_energy: f64,
}

impl FrontEndSummary {
    /// The share of the separated energy that is harmonic, from 0 to 1. A drum tool scores low,
    /// a beatless pad high, and a full mix in between.
    pub fn harmonic_ratio(&self) -> f64 {
        share(self.harmonic_energy, self.percussive_energy)
    }

    pub fn percussive_ratio(&self) -> f64 {
        share(self.percussive_energy, self.harmonic_energy)
    }

    /// The share of the energy in the side channel: 0 for mono, a half for unrelated channels,
    /// 1 when one channel is the other inverted.
    pub fn stereo_width(&self) -> f64 {
        share(self.side_energy, self.mid_energy)
    }
}

fn share(part: f64, other: f64) -> f64 {
    let total = part + other;
    if total > 0.0 { part / total } else { 0.0 }
}

/// Runs the front end over a track's samples as they are decoded.
pub struct FrontEnd {
    channels: usize,
    resampler: Option<MonoResampler>,
    mono: Vec<f32>,
    resampled: Vec<f32>,
    stft: Stft,
    hpss: Hpss,
    summary: FrontEndSummary,
}

impl FrontEnd {
    pub fn new(sample_rate: u32, channels: usize) -> Result<Self, AnalysisError> {
        let resampler = if sample_rate == SAMPLE_RATE {
            None
        } else {
            Some(MonoResampler::new(sample_rate, SAMPLE_RATE)?)
        };
        Ok(Self {
            channels: channels.max(1),
            resampler,
            mono: Vec::new(),
            resampled: Vec::new(),
            stft: Stft::new(),
            hpss: Hpss::new(),
            summary: FrontEndSummary::default(),
        })
    }

    /// Takes interleaved samples at the source rate and hands every frame that can now be
    /// separated to `on_frame`.
    pub fn push(
        &mut self,
        interleaved: &[f32],
        on_frame: &mut impl FnMut(SeparatedFrame<'_>),
    ) -> Result<(), AnalysisError> {
        self.mono.clear();
        let scale = 1.0 / self.channels as f32;
        for frame in interleaved.chunks_exact(self.channels) {
            self.mono.push(frame.iter().sum::<f32>() * scale);
            let (mid, side) = match frame {
                [left, right, ..] => ((left + right) * 0.5, (left - right) * 0.5),
                [only] => (*only, 0.0),
                [] => (0.0, 0.0),
            };
            self.summary.mid_energy += f64::from(mid) * f64::from(mid);
            self.summary.side_energy += f64::from(side) * f64::from(side);
        }
        match &mut self.resampler {
            Some(resampler) => {
                self.resampled.clear();
                resampler.push(&self.mono, &mut self.resampled)?;
                self.analyse_resampled(on_frame);
            }
            None => {
                let Self {
                    mono,
                    stft,
                    hpss,
                    summary,
                    ..
                } = self;
                stft.push(mono, &mut |magnitude| {
                    hpss.push(magnitude, summary, on_frame)
                });
            }
        }
        Ok(())
    }

    /// Flushes the resampler, the last part-filled window and the frames still waiting on their
    /// time medians.
    pub fn finish(
        mut self,
        on_frame: &mut impl FnMut(SeparatedFrame<'_>),
    ) -> Result<FrontEndSummary, AnalysisError> {
        if let Some(resampler) = &mut self.resampler {
            self.resampled.clear();
            resampler.finish(&mut self.resampled)?;
            self.analyse_resampled(on_frame);
        }
        let Self {
            stft,
            hpss,
            summary,
            ..
        } = &mut self;
        stft.finish(&mut |magnitude| hpss.push(magnitude, summary, on_frame));
        hpss.finish(summary, on_frame);
        Ok(self.summary)
    }

    fn analyse_resampled(&mut self, on_frame: &mut impl FnMut(SeparatedFrame<'_>)) {
        let Self {
            resampled,
            stft,
            hpss,
            summary,
            ..
        } = self;
        stft.push(resampled, &mut |magnitude| {
            hpss.push(magnitude, summary, on_frame)
        });
    }
}

/// Resamples mono audio with a synchronous FFT resampler, dropping its delay so
/// frame `n` still starts at sample `n * HOP` of the track.
pub(crate) struct MonoResampler {
    inner: FftFixedIn<f32>,
    input: Vec<f32>,
    output: Vec<f32>,
    delay_to_skip: usize,
    input_frames: u64,
    output_frames: u64,
    ratio: f64,
}

impl MonoResampler {
    pub(crate) fn new(from: u32, to: u32) -> Result<Self, AnalysisError> {
        let inner = FftFixedIn::<f32>::new(
            from.max(1) as usize,
            to.max(1) as usize,
            RESAMPLE_CHUNK,
            2,
            1,
        )
        .map_err(AnalysisError::ResamplerSetup)?;
        let output = vec![0.0; inner.output_frames_max()];
        let delay_to_skip = inner.output_delay();
        Ok(Self {
            inner,
            input: Vec::with_capacity(RESAMPLE_CHUNK),
            output,
            delay_to_skip,
            input_frames: 0,
            output_frames: 0,
            ratio: f64::from(to.max(1)) / f64::from(from.max(1)),
        })
    }

    pub(crate) fn push(&mut self, mono: &[f32], out: &mut Vec<f32>) -> Result<(), AnalysisError> {
        self.input_frames += mono.len() as u64;
        let mut rest = mono;
        while !rest.is_empty() {
            let wanted = self.inner.input_frames_next() - self.input.len();
            let (take, left) = rest.split_at(wanted.min(rest.len()));
            self.input.extend_from_slice(take);
            rest = left;
            if self.input.len() == self.inner.input_frames_next() {
                let (_, written) = self
                    .inner
                    .process_into_buffer(&[&self.input], &mut [&mut self.output], None)
                    .map_err(AnalysisError::Resample)?;
                self.input.clear();
                self.emit(written, out);
            }
        }
        Ok(())
    }

    pub(crate) fn finish(&mut self, out: &mut Vec<f32>) -> Result<(), AnalysisError> {
        let expected = (self.input_frames as f64 * self.ratio).round() as u64;
        // rubato reads an empty chunk as a channel to leave out, so the leftovers go in only when
        // there are some.
        let mut pending = Some(std::mem::take(&mut self.input)).filter(|left| !left.is_empty());
        while self.output_frames < expected {
            let input = pending.take();
            let chunk: Option<[&[f32]; 1]> = input.as_deref().map(|samples| [samples]);
            let (_, written) = self
                .inner
                .process_partial_into_buffer(
                    chunk.as_ref().map(<[&[f32]; 1]>::as_slice),
                    &mut [&mut self.output],
                    None,
                )
                .map_err(AnalysisError::Resample)?;
            if written == 0 {
                break;
            }
            self.emit(written, out);
        }
        let excess = self.output_frames.saturating_sub(expected) as usize;
        out.truncate(out.len().saturating_sub(excess));
        Ok(())
    }

    fn emit(&mut self, written: usize, out: &mut Vec<f32>) {
        let skip = self.delay_to_skip.min(written);
        self.delay_to_skip -= skip;
        out.extend_from_slice(&self.output[skip..written]);
        self.output_frames += (written - skip) as u64;
    }
}

/// Hann-windowed frames of mono audio and their magnitude spectra.
struct Stft {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    pending: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<realfft::num_complex::Complex<f32>>,
    scratch: Vec<realfft::num_complex::Complex<f32>>,
    magnitude: Vec<f32>,
    /// Samples received that no frame has started at yet.
    unframed: usize,
}

impl Stft {
    fn new() -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        // The periodic Hann window, which overlaps to a constant at a quarter-window hop.
        let window = (0..FFT_SIZE)
            .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / FFT_SIZE as f32).cos())
            .collect();
        Self {
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            pending: Vec::with_capacity(2 * FFT_SIZE),
            magnitude: vec![0.0; SPECTRUM_BINS],
            unframed: 0,
        }
    }

    fn push(&mut self, samples: &[f32], on_magnitude: &mut impl FnMut(&[f32])) {
        self.pending.extend_from_slice(samples);
        self.unframed += samples.len();
        while self.pending.len() >= FFT_SIZE {
            self.frame(on_magnitude);
        }
    }

    /// Pads the tail with silence so every sample starts in some frame's first hop.
    fn finish(&mut self, on_magnitude: &mut impl FnMut(&[f32])) {
        while self.unframed > 0 {
            self.pending.resize(FFT_SIZE.max(self.pending.len()), 0.0);
            self.frame(on_magnitude);
        }
    }

    fn frame(&mut self, on_magnitude: &mut impl FnMut(&[f32])) {
        for ((input, sample), weight) in self.input.iter_mut().zip(&self.pending).zip(&self.window)
        {
            *input = sample * weight;
        }
        // The buffers come from the plan, so their lengths always match it.
        let _ =
            self.fft
                .process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch);
        for (magnitude, bin) in self.magnitude.iter_mut().zip(&self.spectrum) {
            *magnitude = bin.norm();
        }
        self.pending.drain(..HOP);
        self.unframed = self.unframed.saturating_sub(HOP);
        on_magnitude(&self.magnitude);
    }
}

/// Median-filtering separation over a sliding window of `MEDIAN_SPAN` magnitude frames.
struct Hpss {
    /// Frame `n` sits in slot `n % MEDIAN_SPAN`. Slots not yet filled hold silence, which pads
    /// the start of the track.
    ring: Vec<Vec<f32>>,
    received: u64,
    separated: u64,
    harmonic: Vec<f32>,
    percussive: Vec<f32>,
    silence: Vec<f32>,
}

impl Hpss {
    fn new() -> Self {
        Self {
            ring: vec![vec![0.0; SPECTRUM_BINS]; MEDIAN_SPAN],
            received: 0,
            separated: 0,
            harmonic: vec![0.0; SPECTRUM_BINS],
            percussive: vec![0.0; SPECTRUM_BINS],
            silence: vec![0.0; SPECTRUM_BINS],
        }
    }

    fn push(
        &mut self,
        magnitude: &[f32],
        summary: &mut FrontEndSummary,
        on_frame: &mut impl FnMut(SeparatedFrame<'_>),
    ) {
        let slot = (self.received % MEDIAN_SPAN as u64) as usize;
        self.ring[slot].copy_from_slice(magnitude);
        self.received += 1;
        if self.received > HALF_SPAN as u64 {
            self.separate(summary, on_frame);
        }
    }

    /// Pads the end with silence until the last frame received is separated.
    fn finish(
        &mut self,
        summary: &mut FrontEndSummary,
        on_frame: &mut impl FnMut(SeparatedFrame<'_>),
    ) {
        let frames = self.received;
        while self.separated < frames {
            let slot = (self.received % MEDIAN_SPAN as u64) as usize;
            self.ring[slot].copy_from_slice(&self.silence);
            self.received += 1;
            self.separate(summary, on_frame);
        }
    }

    /// Separates the frame `HALF_SPAN` behind the newest, whose time window is the whole ring.
    fn separate(
        &mut self,
        summary: &mut FrontEndSummary,
        on_frame: &mut impl FnMut(SeparatedFrame<'_>),
    ) {
        let index = self.separated;
        let centre = &self.ring[(index % MEDIAN_SPAN as u64) as usize];
        let mut window = [0.0f32; MEDIAN_SPAN];
        for bin in 0..SPECTRUM_BINS {
            for (value, frame) in window.iter_mut().zip(&self.ring) {
                *value = frame[bin];
            }
            let harmonic = median(&mut window);

            let from = bin.saturating_sub(HALF_SPAN);
            let to = (bin + HALF_SPAN + 1).min(SPECTRUM_BINS);
            window = [0.0; MEDIAN_SPAN];
            let offset = HALF_SPAN - (bin - from);
            window[offset..offset + (to - from)].copy_from_slice(&centre[from..to]);
            let percussive = median(&mut window);

            let harmonic_power = harmonic * harmonic;
            let percussive_power = percussive * percussive;
            let total = harmonic_power + percussive_power + MASK_FLOOR;
            let magnitude = centre[bin];
            let h = magnitude * harmonic_power / total;
            let p = magnitude * percussive_power / total;
            self.harmonic[bin] = h;
            self.percussive[bin] = p;
            summary.harmonic_energy += f64::from(h) * f64::from(h);
            summary.percussive_energy += f64::from(p) * f64::from(p);
        }
        summary.frames += 1;
        self.separated += 1;
        on_frame(SeparatedFrame {
            index,
            magnitude: centre,
            harmonic: &self.harmonic,
            percussive: &self.percussive,
        });
    }
}

fn median(values: &mut [f32; MEDIAN_SPAN]) -> f32 {
    *values.select_nth_unstable_by(HALF_SPAN, f32::total_cmp).1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(sample_rate: u32, channels: usize, samples: &[f32]) -> (FrontEndSummary, Vec<u64>) {
        let mut indices = Vec::new();
        let mut on_frame = |frame: SeparatedFrame<'_>| {
            assert_eq!(frame.magnitude.len(), SPECTRUM_BINS);
            for ((magnitude, h), p) in frame
                .magnitude
                .iter()
                .zip(frame.harmonic)
                .zip(frame.percussive)
            {
                assert!(h + p <= magnitude * 1.0001 + 1e-9);
            }
            indices.push(frame.index);
        };
        let mut front_end = FrontEnd::new(sample_rate, channels).expect("front end");
        for piece in samples.chunks(4_096 * channels) {
            front_end.push(piece, &mut on_frame).expect("push");
        }
        let summary = front_end.finish(&mut on_frame).expect("finish");
        (summary, indices)
    }

    fn sine(frequency: f32, sample_rate: u32, seconds: f32) -> Vec<f32> {
        let frames = (sample_rate as f32 * seconds) as usize;
        (0..frames)
            .map(|i| {
                0.5 * (i as f32 * frequency * std::f32::consts::TAU / sample_rate as f32).sin()
            })
            .collect()
    }

    fn clicks(sample_rate: u32, seconds: f32, every_seconds: f32) -> Vec<f32> {
        let frames = (sample_rate as f32 * seconds) as usize;
        let every = (sample_rate as f32 * every_seconds) as usize;
        (0..frames)
            .map(|i| if i % every == every / 2 { 0.9 } else { 0.0 })
            .collect()
    }

    #[test]
    fn a_pure_sine_is_harmonic() {
        let (summary, _) = run(SAMPLE_RATE, 1, &sine(1_000.0, SAMPLE_RATE, 4.0));

        assert!(summary.harmonic_ratio() > 0.90, "{summary:?}");
    }

    #[test]
    fn a_click_train_is_percussive() {
        let (summary, _) = run(SAMPLE_RATE, 1, &clicks(SAMPLE_RATE, 4.0, 0.5));

        assert!(summary.percussive_ratio() > 0.90, "{summary:?}");
    }

    #[test]
    fn a_sine_at_another_rate_is_resampled_and_still_harmonic() {
        let (summary, _) = run(48_000, 1, &sine(440.0, 48_000, 3.0));

        assert!(summary.harmonic_ratio() > 0.90, "{summary:?}");
        let samples = 3 * SAMPLE_RATE as u64;
        assert_eq!(summary.frames, samples.div_ceil(HOP as u64));
    }

    #[test]
    fn every_sample_starts_in_a_frame_and_frames_arrive_in_order() {
        let samples = 10 * HOP + 17;
        let (summary, indices) = run(SAMPLE_RATE, 1, &vec![0.1; samples]);

        assert_eq!(summary.frames, 11);
        assert_eq!(indices, (0..11).collect::<Vec<_>>());
    }

    #[test]
    fn a_track_shorter_than_one_window_still_makes_a_frame() {
        let (summary, indices) = run(SAMPLE_RATE, 2, &[0.2; 200]);

        assert_eq!(summary.frames, 1);
        assert_eq!(indices, [0]);
    }

    #[test]
    fn the_resampler_keeps_a_tone_where_it_was() {
        let rate = 48_000;
        let mut input = vec![0.0f32; rate as usize];
        let onset = rate as usize / 2;
        for (i, sample) in input[onset..].iter_mut().enumerate() {
            *sample = 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin();
        }
        let mut resampler = MonoResampler::new(rate, SAMPLE_RATE).expect("resampler");
        let mut out = Vec::new();
        resampler.push(&input, &mut out).expect("push");
        resampler.finish(&mut out).expect("finish");

        assert_eq!(out.len(), SAMPLE_RATE as usize);
        let first_loud = out
            .iter()
            .position(|sample| sample.abs() > 0.05)
            .expect("tone");
        let expected = SAMPLE_RATE as usize / 2;
        assert!(first_loud.abs_diff(expected) < 30, "{first_loud}");
    }

    #[test]
    fn a_track_that_fills_the_last_resampler_chunk_exactly_still_finishes() {
        let samples = 10 * RESAMPLE_CHUNK;
        let (summary, _) = run(48_000, 1, &vec![0.1; samples]);

        let resampled = (samples as u64 * u64::from(SAMPLE_RATE)).div_ceil(48_000);
        assert_eq!(summary.frames, resampled.div_ceil(HOP as u64));
    }

    #[test]
    fn stereo_width_reads_the_side_channel() {
        let tone = sine(200.0, SAMPLE_RATE, 0.5);
        let mono: Vec<f32> = tone.iter().flat_map(|&sample| [sample, sample]).collect();
        let wide: Vec<f32> = tone.iter().flat_map(|&sample| [sample, -sample]).collect();

        assert!(run(SAMPLE_RATE, 2, &mono).0.stereo_width() < 1e-9);
        assert!(run(SAMPLE_RATE, 2, &wide).0.stereo_width() > 0.999);
    }
}
