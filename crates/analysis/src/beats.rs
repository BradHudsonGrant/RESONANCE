//! Beats, downbeats and tempo.
//!
//! `Beat This!` (Foscarin, Schlueter and Widmer, ISMIR 2024) finds beats and downbeats from a
//! log-mel spectrogram at 50 frames a second. A 20 ms frame is too coarse for two synced kicks,
//! which flam at that offset, so every beat is then moved onto the start of its kick: the steepest
//! rise in the zero-phase envelope under 150 Hz, then the upward zero-crossing at or just ahead of
//! it, so a cue or loop jumps there without a click. The bar phase is chosen over the whole
//! track at once (`resonance_core::beat_grid`), and a steady tempo gets a constant grid.

use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use resonance_core::analysis::BeatGrid;
use resonance_core::beat_grid::{
    BarPhase, BeatEvidence, GridEvidence, choose_bar_phase, fit_tempo, grid_confidence,
};

use crate::frontend::{FFT_SIZE, FrontEnd, HOP, MonoResampler, SAMPLE_RATE, SeparatedFrame};
use crate::registry::{InputSpec, ModelSpec};
use crate::runtime::{FloatTensor, ModelSession};
use crate::{Analyser, AnalyserTier, AnalysisError};

pub const MODEL_ID: &str = "beat_this";
const MODEL_VERSION: &str = "final0";
const MODEL_SHA256: &str = "dc834dd6e1d8c4c883a3f5ddf3f382a88af5aea07a11ab33b22c902596837469";

/// `Beat This!` `final0`, exported by `tools/models/export_beat_this.py` and checked against
/// the PyTorch checkpoint by `verify_beat_this.py`. Its code and weights are MIT licensed.
pub fn model_spec() -> ModelSpec {
    ModelSpec {
        id: MODEL_ID,
        version: MODEL_VERSION,
        url: "https://github.com/BradHudsonGrant/RESONANCE/releases/download/models-beat-this-final0/beat_this-final0.onnx",
        sha256: MODEL_SHA256,
        size_bytes: 83_162_655,
        licence: "MIT",
        commercial_ok: true,
        input: InputSpec {
            name: "spect",
            shape: &[1, -1, MEL_BINS as i64],
            sample_rate_hz: MEL_SAMPLE_RATE,
        },
    }
}

/// The model's input, as its training pipeline made it: torchaudio's `MelSpectrogram` with
/// these settings, then `log1p(1000 x)`.
pub const MEL_SAMPLE_RATE: u32 = 22_050;
const MEL_FFT: usize = 1_024;
pub const MEL_HOP: usize = 441;
pub const MEL_BINS: usize = 128;
pub const MEL_FRAMES_PER_SECOND: f64 = MEL_SAMPLE_RATE as f64 / MEL_HOP as f64;
const MEL_LOW_HZ: f64 = 30.0;
const MEL_HIGH_HZ: f64 = 11_000.0;
const LOG_MULTIPLIER: f32 = 1_000.0;

/// 30 s chunks, the length the model was trained on, overlapping by 5 s.
const CHUNK_FRAMES: usize = 1_500;
const CHUNK_OVERLAP: usize = 250;
/// Silent frames added at each end, since the model was not trained on the edges of its input.
const BORDER_FRAMES: usize = 6;
/// The shortest input the model takes.
const MIN_FRAMES: usize = 16;

const MIN_BPM: f64 = 68.0;
const MAX_BPM: f64 = 185.0;
/// How hard the beat path resists an interval away from the tempo period. At 128 BPM a beat
/// one frame early costs 0.16, and one a quarter early costs 5, more than any beat is worth.
const TRANSITION_WEIGHT: f64 = 100.0;
/// Beats at either end with less probability than this are silence or a fade, not the track.
const EDGE_STRENGTH: f32 = 0.1;

/// The search either side of a coarse beat: 15 ms.
const SNAP_REACH: usize = 661;
/// The low-pass for the kick envelope.
const KICK_BAND_HZ: f64 = 150.0;
/// A window's steepest rise against its median for it to count as a kick.
const KICK_PROMINENCE: f64 = 3.0;
/// And against the track's typical kick, so a quiet bass ripple is not taken for one.
const KICK_SHARE_OF_TYPICAL: f64 = 0.25;
/// Below this share of percussive energy a bar is beatless.
const PERCUSSIVE_BAR: f64 = 0.15;
/// How far a snapped beat on a steady grid may sit from the fitted line.
const LINE_TOLERANCE_S: f64 = 0.000_5;
/// Furthest to look for a zero-crossing: 0.5 ms.
const ZERO_CROSSING_REACH: usize = 22;
/// A broadband attack this many times the track's median flux stands in for a kick.
const FLUX_PROMINENCE: f32 = 3.0;
/// STFT frames either side of a beat read for its kick and snare.
const EVIDENCE_FRAMES: i64 = 2;

/// Spectrum bins at 44.1 kHz with a 2,048-point FFT, 21.5 Hz apart.
const LOW_BINS: std::ops::Range<usize> = 1..7;
const MID_BINS: std::ops::Range<usize> = 7..117;
const FLUX_BINS: std::ops::Range<usize> = 7..372;

/// The analyser's settings, hashed into its cache key.
const SETTINGS: &str = concat!(
    "beat_this final0 dc834dd6e1d8c4c883a3f5ddf3f382a88af5aea07a11ab33b22c902596837469;",
    "chunk 1500 overlap 250 border 6; bpm 68-185 transition 100 edge 0.1;",
    "snap 15ms kick<150Hz zero-phase hilbert 8192 taper 2048 steepest prominence 3 typical 0.25;",
    "bar 0.15 line 0.5ms zero-crossing 0.5ms flux 150-8000Hz 3x"
);

/// The log-mel front end the model reads.
pub struct LogMel {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    filters: Vec<MelFilter>,
}

struct MelFilter {
    first_bin: usize,
    weights: Vec<f32>,
}

impl Default for LogMel {
    fn default() -> Self {
        Self::new()
    }
}

impl LogMel {
    pub fn new() -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(MEL_FFT);
        let window = (0..MEL_FFT)
            .map(|n| 0.5 - 0.5 * (std::f32::consts::TAU * n as f32 / MEL_FFT as f32).cos())
            .collect();
        Self {
            fft,
            window,
            filters: slaney_filters(),
        }
    }

    /// Frames of `MEL_BINS` values in row-major order, from mono audio at `MEL_SAMPLE_RATE`.
    /// Frame `t` is centred on sample `t * MEL_HOP`, with the signal mirrored past each end.
    pub fn spectrogram(&self, signal: &[f32]) -> Vec<f32> {
        if signal.is_empty() {
            return Vec::new();
        }
        let frames = 1 + signal.len() / MEL_HOP;
        let half = (MEL_FFT / 2) as isize;
        let mut input = self.fft.make_input_vec();
        let mut spectrum: Vec<Complex<f32>> = self.fft.make_output_vec();
        let mut scratch = self.fft.make_scratch_vec();
        let mut magnitude = vec![0.0f32; MEL_FFT / 2 + 1];
        // torchaudio's "frame_length" normalisation.
        let scale = 1.0 / (MEL_FFT as f32).sqrt();
        let mut out = Vec::with_capacity(frames * MEL_BINS);
        for frame in 0..frames {
            let start = (frame * MEL_HOP) as isize - half;
            for (n, (slot, weight)) in input.iter_mut().zip(&self.window).enumerate() {
                *slot = signal[reflect(start + n as isize, signal.len())] * weight;
            }
            let _ = self
                .fft
                .process_with_scratch(&mut input, &mut spectrum, &mut scratch);
            for (value, bin) in magnitude.iter_mut().zip(&spectrum) {
                *value = bin.norm() * scale;
            }
            for filter in &self.filters {
                let energy: f32 = magnitude[filter.first_bin..]
                    .iter()
                    .zip(&filter.weights)
                    .map(|(value, weight)| value * weight)
                    .sum();
                out.push((LOG_MULTIPLIER * energy).ln_1p());
            }
        }
        out
    }
}

fn reflect(index: isize, len: usize) -> usize {
    if len == 1 {
        return 0;
    }
    let last = len as isize - 1;
    let mut index = index;
    while index < 0 || index > last {
        index = if index < 0 { -index } else { 2 * last - index };
    }
    index as usize
}

fn hz_to_slaney(hz: f64) -> f64 {
    let step = 200.0 / 3.0;
    let log_step = 6.4f64.ln() / 27.0;
    if hz < 1_000.0 {
        hz / step
    } else {
        1_000.0 / step + (hz / 1_000.0).ln() / log_step
    }
}

fn slaney_to_hz(mel: f64) -> f64 {
    let step = 200.0 / 3.0;
    let log_step = 6.4f64.ln() / 27.0;
    let linear_top = 1_000.0 / step;
    if mel < linear_top {
        mel * step
    } else {
        1_000.0 * (log_step * (mel - linear_top)).exp()
    }
}

/// Triangular filters evenly spaced on the Slaney mel scale, unnormalised, as torchaudio
/// builds them.
fn slaney_filters() -> Vec<MelFilter> {
    let bins = MEL_FFT / 2 + 1;
    let nyquist = f64::from(MEL_SAMPLE_RATE) / 2.0;
    let low = hz_to_slaney(MEL_LOW_HZ);
    let high = hz_to_slaney(MEL_HIGH_HZ);
    let points: Vec<f64> = (0..MEL_BINS + 2)
        .map(|i| slaney_to_hz(low + (high - low) * i as f64 / (MEL_BINS + 1) as f64))
        .collect();
    (0..MEL_BINS)
        .map(|m| {
            let weights: Vec<f64> = (0..bins)
                .map(|bin| {
                    let hz = nyquist * bin as f64 / (bins - 1) as f64;
                    let down = (hz - points[m]) / (points[m + 1] - points[m]);
                    let up = (points[m + 2] - hz) / (points[m + 2] - points[m + 1]);
                    down.min(up).max(0.0)
                })
                .collect();
            let first_bin = weights.iter().position(|w| *w > 0.0).unwrap_or(0);
            let last_bin = weights.iter().rposition(|w| *w > 0.0).unwrap_or(first_bin);
            MelFilter {
                first_bin,
                weights: weights[first_bin..=last_bin]
                    .iter()
                    .map(|w| *w as f32)
                    .collect(),
            }
        })
        .collect()
}

/// The model's beat and downbeat logits, one each per 20 ms frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Activations {
    pub beat: Vec<f32>,
    pub downbeat: Vec<f32>,
}

/// Runs the model over a whole track's log-mel frames in overlapping 30 s chunks. Each frame
/// takes its logits from the chunk where it sits furthest from an edge.
pub fn activations(session: &mut ModelSession, mel: &[f32]) -> Result<Activations, AnalysisError> {
    let frames = mel.len() / MEL_BINS;
    let total = frames + 2 * BORDER_FRAMES;
    let mut padded = vec![0.0f32; total * MEL_BINS];
    padded[BORDER_FRAMES * MEL_BINS..(BORDER_FRAMES + frames) * MEL_BINS]
        .copy_from_slice(&mel[..frames * MEL_BINS]);
    let padded_total = total.max(MIN_FRAMES);
    padded.resize(padded_total * MEL_BINS, 0.0);

    let mut starts = Vec::new();
    if padded_total <= CHUNK_FRAMES {
        starts.push(0);
    } else {
        let mut start = 0;
        while start + CHUNK_FRAMES < padded_total {
            starts.push(start);
            start += CHUNK_FRAMES - CHUNK_OVERLAP;
        }
        starts.push(padded_total - CHUNK_FRAMES);
    }

    let mut beat = vec![0.0f32; padded_total];
    let mut downbeat = vec![0.0f32; padded_total];
    let mut best_edge = vec![-1i64; padded_total];
    for start in starts {
        let len = CHUNK_FRAMES.min(padded_total - start);
        let input = FloatTensor {
            shape: vec![1, len as i64, MEL_BINS as i64],
            data: padded[start * MEL_BINS..(start + len) * MEL_BINS].to_vec(),
        };
        let outputs = session.run("spect", input)?;
        let [beat_out, downbeat_out] = outputs.as_slice() else {
            return Err(AnalysisError::UnexpectedOutput {
                output: format!("{} outputs", outputs.len()),
            });
        };
        if beat_out.data.len() != len || downbeat_out.data.len() != len {
            return Err(AnalysisError::UnexpectedOutput {
                output: format!("beat of shape {:?}", beat_out.shape),
            });
        }
        for offset in 0..len {
            let edge = offset.min(len - 1 - offset) as i64;
            let frame = start + offset;
            if edge > best_edge[frame] {
                best_edge[frame] = edge;
                beat[frame] = beat_out.data[offset];
                downbeat[frame] = downbeat_out.data[offset];
            }
        }
    }
    let keep = BORDER_FRAMES..BORDER_FRAMES + frames;
    Ok(Activations {
        beat: beat[keep.clone()].to_vec(),
        downbeat: downbeat[keep].to_vec(),
    })
}

/// Beats at the resolution of the model's frames, before they are moved onto the kicks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoarseBeats {
    pub beats_s: Vec<f64>,
    /// The beat probability at each beat.
    pub strength: Vec<f32>,
    /// The downbeat logit at each beat.
    pub downbeat_logit: Vec<f32>,
}

fn sigmoid(logit: f32) -> f32 {
    1.0 / (1.0 + (-logit).exp())
}

/// Picks one beat path through the beat probabilities by dynamic programming (Ellis, "Beat
/// tracking by dynamic programming", 2007): every frame on the path adds its probability, and
/// every interval away from the tempo period costs the square of its log ratio.
pub fn coarse_beats(activations: &Activations) -> CoarseBeats {
    let strength: Vec<f32> = activations.beat.iter().copied().map(sigmoid).collect();
    let frames = strength.len();
    let Some(period) = beat_period(&activations.beat, &strength) else {
        return CoarseBeats::default();
    };
    let shortest = (period / 2.0).round().max(1.0) as usize;
    let longest = (period * 2.0).round() as usize;
    let mut score = vec![0.0f64; frames];
    let mut previous = vec![usize::MAX; frames];
    for frame in 0..frames {
        // A path only links back when that adds to its score, so it can start afresh after
        // silence.
        let mut best = 0.0;
        for gap in shortest..=longest.min(frame) {
            let ratio = (gap as f64 / period).ln();
            let candidate = score[frame - gap] - TRANSITION_WEIGHT * ratio * ratio;
            if candidate > best {
                best = candidate;
                previous[frame] = frame - gap;
            }
        }
        score[frame] = f64::from(strength[frame]) + best;
    }
    let tail = frames.saturating_sub(period.ceil() as usize);
    let Some(mut frame) = (tail..frames).max_by(|a, b| score[*a].total_cmp(&score[*b])) else {
        return CoarseBeats::default();
    };
    let mut path = vec![frame];
    while previous[frame] != usize::MAX {
        frame = previous[frame];
        path.push(frame);
    }
    path.reverse();
    let first = path.iter().position(|&f| strength[f] >= EDGE_STRENGTH);
    let last = path.iter().rposition(|&f| strength[f] >= EDGE_STRENGTH);
    let (Some(first), Some(last)) = (first, last) else {
        return CoarseBeats::default();
    };

    let mut beats = CoarseBeats::default();
    for &frame in &path[first..=last] {
        let offset = if frame > 0 && frame + 1 < frames {
            let (a, b, c) = (
                f64::from(strength[frame - 1]),
                f64::from(strength[frame]),
                f64::from(strength[frame + 1]),
            );
            let curve = a - 2.0 * b + c;
            if curve < 0.0 {
                (0.5 * (a - c) / curve).clamp(-0.5, 0.5)
            } else {
                0.0
            }
        } else {
            0.0
        };
        let near = frame.saturating_sub(2)..(frame + 3).min(frames);
        beats
            .beats_s
            .push((frame as f64 + offset) / MEL_FRAMES_PER_SECOND);
        beats.strength.push(strength[frame]);
        beats.downbeat_logit.push(
            activations.downbeat[near]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max),
        );
    }
    beats
}

/// The beat period in frames: the median gap between confident peaks, or the strongest
/// autocorrelation lag when there are too few, folded into 68 to 185 BPM.
fn beat_period(logits: &[f32], strength: &[f32]) -> Option<f64> {
    let peaks: Vec<usize> = (0..logits.len())
        .filter(|&t| {
            let near = t.saturating_sub(3)..(t + 4).min(logits.len());
            logits[t] > 0.0 && logits[near].iter().all(|value| *value <= logits[t])
        })
        .collect();
    let shortest = MEL_FRAMES_PER_SECOND * 60.0 / MAX_BPM;
    let longest = MEL_FRAMES_PER_SECOND * 60.0 / MIN_BPM;
    let mut period = if peaks.len() >= 3 {
        let mut gaps: Vec<f64> = peaks.windows(2).map(|p| (p[1] - p[0]) as f64).collect();
        gaps.sort_by(f64::total_cmp);
        gaps[gaps.len() / 2]
    } else {
        let lags = shortest.floor() as usize..=longest.ceil() as usize;
        lags.max_by(|a, b| autocorrelation(strength, *a).total_cmp(&autocorrelation(strength, *b)))?
            as f64
    };
    if period <= 0.0 {
        return None;
    }
    while period < shortest {
        period *= 2.0;
    }
    while period > longest {
        period /= 2.0;
    }
    Some(period)
}

fn autocorrelation(values: &[f32], lag: usize) -> f64 {
    values
        .iter()
        .zip(values.iter().skip(lag))
        .map(|(a, b)| f64::from(a * b))
        .sum()
}

/// What the grid refinement reads from a track: its mono samples at 44.1 kHz, those samples
/// under 150 Hz, and the percussive and harmonic energy of each STFT frame.
pub struct BeatAudio {
    mono: Vec<f32>,
    low: Vec<f32>,
    frames: Vec<FrameBands>,
}

#[derive(Debug, Clone, Copy, Default)]
struct FrameBands {
    low: f32,
    mid: f32,
    percussive: f32,
    harmonic: f32,
    flux: f32,
}

impl BeatAudio {
    /// From decoded interleaved samples at any rate.
    pub fn from_interleaved(
        samples: &[f32],
        sample_rate: u32,
        channels: usize,
    ) -> Result<Self, AnalysisError> {
        let channels = channels.max(1);
        let scale = 1.0 / channels as f32;
        let mono: Vec<f32> = samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() * scale)
            .collect();
        if sample_rate == SAMPLE_RATE {
            return Self::from_mono(mono);
        }
        let mut resampler = MonoResampler::new(sample_rate, SAMPLE_RATE)?;
        let mut resampled = Vec::with_capacity(mono.len());
        resampler.push(&mono, &mut resampled)?;
        resampler.finish(&mut resampled)?;
        Self::from_mono(resampled)
    }

    /// From mono samples at 44.1 kHz.
    pub fn from_mono(mono: Vec<f32>) -> Result<Self, AnalysisError> {
        let mut frames = Vec::with_capacity(mono.len() / HOP + 1);
        let mut last = vec![0.0f32; FFT_SIZE / 2 + 1];
        let mut on_frame = |frame: SeparatedFrame<'_>| {
            let energy = |bins: &[f32]| bins.iter().map(|value| value * value).sum::<f32>();
            let flux = frame.magnitude[FLUX_BINS]
                .iter()
                .zip(&last[FLUX_BINS])
                .map(|(now, before)| (now - before).max(0.0))
                .sum();
            last.copy_from_slice(frame.magnitude);
            frames.push(FrameBands {
                low: energy(&frame.percussive[LOW_BINS]),
                mid: energy(&frame.percussive[MID_BINS]),
                percussive: energy(frame.percussive),
                harmonic: energy(frame.harmonic),
                flux,
            });
        };
        let mut front_end = FrontEnd::new(SAMPLE_RATE, 1)?;
        front_end.push(&mono, &mut on_frame)?;
        front_end.finish(&mut on_frame)?;
        let low = kick_band(&mono);
        Ok(Self { mono, low, frames })
    }

    pub fn duration_s(&self) -> f64 {
        self.mono.len() as f64 / f64::from(SAMPLE_RATE)
    }

    /// The model's input: the track at 22.05 kHz as log-mel frames.
    pub fn log_mel(&self) -> Result<Vec<f32>, AnalysisError> {
        let mut resampler = MonoResampler::new(SAMPLE_RATE, MEL_SAMPLE_RATE)?;
        let mut signal = Vec::with_capacity(self.mono.len() / 2 + 1);
        resampler.push(&self.mono, &mut signal)?;
        resampler.finish(&mut signal)?;
        Ok(LogMel::new().spectrogram(&signal))
    }

    fn frame_centre_s(frame: usize) -> f64 {
        (frame * HOP + FFT_SIZE / 2) as f64 / f64::from(SAMPLE_RATE)
    }

    /// STFT frames whose centres fall from `from_s` up to `to_s`.
    fn frames_between(&self, from_s: f64, to_s: f64) -> std::ops::Range<usize> {
        let to_frame = |time: f64| {
            let sample = time * f64::from(SAMPLE_RATE) - (FFT_SIZE / 2) as f64;
            ((sample / HOP as f64).ceil().max(0.0) as usize).min(self.frames.len())
        };
        let start = to_frame(from_s);
        start..to_frame(to_s).max(start)
    }

    fn percussive_share(&self, from_s: f64, to_s: f64) -> f64 {
        let (mut percussive, mut harmonic) = (0.0f64, 0.0f64);
        for bands in &self.frames[self.frames_between(from_s, to_s)] {
            percussive += f64::from(bands.percussive);
            harmonic += f64::from(bands.harmonic);
        }
        let total = percussive + harmonic;
        if total > 0.0 { percussive / total } else { 0.0 }
    }

    fn evidence(&self, beat_s: f64, next_s: f64) -> (f32, f32, f32) {
        let centre =
            ((beat_s * f64::from(SAMPLE_RATE) - (FFT_SIZE / 2) as f64) / HOP as f64).round() as i64;
        let last = self.frames.len() as i64;
        let from = (centre - EVIDENCE_FRAMES).clamp(0, last) as usize;
        let to = (centre + EVIDENCE_FRAMES + 1).clamp(0, last) as usize;
        let near = &self.frames[from..to.max(from)];
        let low = near.iter().map(|bands| bands.low).fold(0.0, f32::max);
        let mid = near.iter().map(|bands| bands.mid).fold(0.0, f32::max);
        let over = &self.frames[self.frames_between(beat_s, next_s)];
        let harmonic = if over.is_empty() {
            0.0
        } else {
            over.iter().map(|bands| bands.harmonic).sum::<f32>() / over.len() as f32
        };
        (low, mid, harmonic)
    }

    /// The upward zero-crossing at or up to 0.5 ms before `sample`. When the mix has none
    /// there, the onset estimate may sit a few samples ahead of the kick, so the first one up
    /// to 0.5 ms after it is the kick's own start. With neither, the quietest sample before it.
    fn zero_crossing(&self, sample: usize) -> usize {
        let sample = sample.min(self.mono.len().saturating_sub(1));
        if sample == 0 {
            return 0;
        }
        let rises_at = |at: usize| self.mono[at - 1] <= 0.0 && self.mono[at] > 0.0;
        let back = ZERO_CROSSING_REACH.min(sample - 1);
        if let Some(at) = (sample - back..=sample).rev().find(|at| rises_at(*at)) {
            return at;
        }
        let ahead = (sample + ZERO_CROSSING_REACH).min(self.mono.len() - 1);
        if let Some(at) = (sample + 1..=ahead).find(|at| rises_at(*at)) {
            return at;
        }
        (sample - back..=sample)
            .min_by(|a, b| self.mono[*a].abs().total_cmp(&self.mono[*b].abs()))
            .unwrap_or(sample)
    }

    /// The steepest rise in the kick envelope near `centre`, and how steep it is against the
    /// window's median rise. The envelope is the squared magnitude of the analytic
    /// signal under 150 Hz, which has none of the ripple a low sine leaves in its square.
    fn kick_onset(&self, centre: usize, hilbert: &mut Hilbert) -> Option<KickRise> {
        let half = HILBERT_SIZE / 2;
        let start = centre.checked_sub(half)?;
        let end = start + HILBERT_SIZE;
        if end > self.low.len() {
            return None;
        }
        let envelope = hilbert.energy(&self.low[start..end]);
        let first = half - SNAP_REACH;
        // `rise[i]` is the step into envelope sample `first + i`.
        let rise: Vec<f64> = (first..=half + SNAP_REACH)
            .map(|i| (envelope[i] - envelope[i - 1]).max(0.0))
            .collect();
        let (peak_at, peak) = rise
            .iter()
            .copied()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        // A rise still climbing at the edge is a kick outside the window, not one inside it.
        if peak <= 0.0 || peak_at + 1 == rise.len() {
            return None;
        }
        let mut sorted = rise.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        Some(KickRise {
            sample: start + first + peak_at,
            peak,
            median,
        })
    }

    fn flux_attack(&self, beat_s: f64, median_flux: f32) -> Option<usize> {
        let reach = SNAP_REACH as f64 / f64::from(SAMPLE_RATE);
        let range = self.frames_between(beat_s - reach, beat_s + reach);
        let (frame, flux) = range
            .map(|frame| (frame, self.frames[frame].flux))
            .max_by(|a, b| a.1.total_cmp(&b.1))?;
        (flux > FLUX_PROMINENCE * median_flux && flux > 0.0)
            .then(|| (Self::frame_centre_s(frame) * f64::from(SAMPLE_RATE)).round() as usize)
    }
}

/// Samples in the analytic-signal window: 186 ms, long enough that its ends, where the FFT
/// wraps round, sit well away from the 30 ms search in the middle.
const HILBERT_SIZE: usize = 8_192;
/// Samples at each end eased in and out, so a kick cut off by the window does not wrap round as
/// a step.
const HILBERT_TAPER: usize = 2_048;

/// The analytic signal of short windows by FFT.
struct Hilbert {
    forward: Arc<dyn RealToComplex<f64>>,
    inverse: Arc<dyn ComplexToReal<f64>>,
    input: Vec<f64>,
    tapered: Vec<f64>,
    spectrum: Vec<Complex<f64>>,
    output: Vec<f64>,
}

impl Hilbert {
    fn new() -> Self {
        let mut planner = RealFftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(HILBERT_SIZE);
        let inverse = planner.plan_fft_inverse(HILBERT_SIZE);
        Self {
            input: forward.make_input_vec(),
            tapered: vec![0.0; HILBERT_SIZE],
            spectrum: forward.make_output_vec(),
            output: inverse.make_output_vec(),
            forward,
            inverse,
        }
    }

    /// `x^2 + H{x}^2` at each sample of `window`.
    fn energy(&mut self, window: &[f32]) -> Vec<f64> {
        for (index, (slot, value)) in self.input.iter_mut().zip(window).enumerate() {
            let edge = index.min(HILBERT_SIZE - 1 - index);
            let taper = if edge < HILBERT_TAPER {
                0.5 - 0.5 * (std::f64::consts::PI * edge as f64 / HILBERT_TAPER as f64).cos()
            } else {
                1.0
            };
            *slot = f64::from(*value) * taper;
        }
        // The transform uses its input as scratch space.
        self.tapered.copy_from_slice(&self.input);
        let _ = self.forward.process(&mut self.input, &mut self.spectrum);
        let last = self.spectrum.len() - 1;
        for (bin, value) in self.spectrum.iter_mut().enumerate() {
            // The Hilbert transform turns each positive frequency a quarter cycle back.
            *value = if bin == 0 || bin == last {
                Complex::new(0.0, 0.0)
            } else {
                Complex::new(value.im, -value.re)
            };
        }
        let _ = self.inverse.process(&mut self.spectrum, &mut self.output);
        let scale = 1.0 / HILBERT_SIZE as f64;
        window
            .iter()
            .zip(&self.output)
            .zip(&self.tapered)
            .map(|((_, h), x)| {
                let h = h * scale;
                x * x + h * h
            })
            .collect()
    }
}

struct KickRise {
    sample: usize,
    peak: f64,
    median: f64,
}

/// A fourth-order Butterworth low-pass at 150 Hz, run forwards then backwards. With no delay and a
/// symmetric response, the envelope's steepest rise sits on the kick's start. Through the filter
/// forwards only it sits 3.8 ms later, and walking back to a fifth of that slope still lands
/// 1.2 ms late; on this envelope the same walk lands 3.5 ms early, so none is taken.
fn kick_band(signal: &[f32]) -> Vec<f32> {
    let sections = [0.541_196_1, 1.306_563].map(|q| Biquad::low_pass(KICK_BAND_HZ, q));
    let mut out: Vec<f64> = signal.iter().map(|value| f64::from(*value)).collect();
    for section in &sections {
        section.run(out.iter_mut());
    }
    for section in &sections {
        section.run(out.iter_mut().rev());
    }
    out.into_iter().map(|value| value as f32).collect()
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    /// The low-pass from Bristow-Johnson's audio EQ cookbook.
    fn low_pass(cutoff_hz: f64, q: f64) -> Self {
        let w0 = std::f64::consts::TAU * cutoff_hz / f64::from(SAMPLE_RATE);
        let alpha = w0.sin() / (2.0 * q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    fn run<'a>(&self, samples: impl Iterator<Item = &'a mut f64>) {
        let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
        for sample in samples {
            let x = *sample;
            let y = self.b0 * x + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2;
            x2 = x1;
            x1 = x;
            y2 = y1;
            y1 = y;
            *sample = y;
        }
    }
}

/// A finished beat grid and what it rests on.
#[derive(Debug, Clone, PartialEq)]
pub struct BeatEstimate {
    pub grid: BeatGrid,
    pub variable: bool,
    pub confidence: f32,
    pub phase: BarPhase,
    /// Beats that landed on a kick or a clear attack rather than between them.
    pub snapped: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Snap {
    Kick,
    Attack,
    None,
}

/// Moves coarse beats onto their kicks, chooses the bar phase and fits the tempo.
pub fn estimate_grid(audio: &BeatAudio, coarse: &CoarseBeats) -> Option<BeatEstimate> {
    let count = coarse.beats_s.len();
    if count < 2 || audio.mono.is_empty() {
        return None;
    }
    let rate = f64::from(SAMPLE_RATE);
    let bar_share: Vec<f64> = (0..count)
        .map(|index| {
            let start = index - index % 4;
            let from = coarse.beats_s[start];
            let to = coarse
                .beats_s
                .get(start + 4)
                .copied()
                .unwrap_or_else(|| audio.duration_s());
            audio.percussive_share(from, to)
        })
        .collect();

    let mut hilbert = Hilbert::new();
    let rises: Vec<Option<KickRise>> = coarse
        .beats_s
        .iter()
        .map(|beat| audio.kick_onset((beat * rate).round().max(0.0) as usize, &mut hilbert))
        .collect();
    let mut peaks: Vec<f64> = rises.iter().flatten().map(|rise| rise.peak).collect();
    peaks.sort_by(f64::total_cmp);
    let typical = peaks.get(peaks.len() / 2).copied().unwrap_or(0.0);
    let mut fluxes: Vec<f32> = audio.frames.iter().map(|bands| bands.flux).collect();
    fluxes.sort_by(f32::total_cmp);
    let median_flux = fluxes.get(fluxes.len() / 2).copied().unwrap_or(0.0);

    let mut samples: Vec<Option<usize>> = vec![None; count];
    let mut snaps = vec![Snap::None; count];
    let is_kick = |rise: &KickRise, index: usize| {
        bar_share[index] >= PERCUSSIVE_BAR
            && rise.peak > KICK_PROMINENCE * rise.median
            && rise.peak >= KICK_SHARE_OF_TYPICAL * typical
    };
    for index in 0..count {
        if let Some(rise) = rises[index].as_ref().filter(|rise| is_kick(rise, index)) {
            samples[index] = Some(audio.zero_crossing(rise.sample));
            snaps[index] = Snap::Kick;
        } else if bar_share[index] < PERCUSSIVE_BAR
            && let Some(onset) = audio.flux_attack(coarse.beats_s[index], median_flux)
        {
            samples[index] = Some(audio.zero_crossing(onset));
            snaps[index] = Snap::Attack;
        }
    }
    let mut beats_s = interpolate(&samples, &coarse.beats_s, rate);
    for (index, beat) in beats_s.iter_mut().enumerate() {
        if snaps[index] == Snap::None {
            *beat = audio.zero_crossing((*beat * rate).round().max(0.0) as usize) as f64 / rate;
        }
    }

    let evidence: Vec<BeatEvidence> = (0..count)
        .map(|index| {
            let next = beats_s
                .get(index + 1)
                .copied()
                .unwrap_or_else(|| audio.duration_s());
            let (low, mid, harmonic) = audio.evidence(beats_s[index], next);
            BeatEvidence {
                downbeat_logit: coarse.downbeat_logit[index],
                full_drums: snaps[index] == Snap::Kick,
                low,
                mid,
                harmonic,
            }
        })
        .collect();
    let phase = choose_bar_phase(&evidence);
    let first_downbeat = phase.first_downbeat(count);
    let anchored: Vec<bool> = snaps.iter().map(|snap| *snap == Snap::Kick).collect();
    let fit = fit_tempo(&beats_s, first_downbeat, &anchored)?;
    if fit.steady {
        // A snapped beat already on the line keeps its own zero-crossing. One that found
        // nothing, or strayed to a fill, a flam or the edge of its window, looks again from the
        // line, a closer start than the model's frame, and settles on the line if that fails.
        for (index, beat) in beats_s.iter_mut().enumerate() {
            let on_line = fit.offset_s + fit.period_s * index as f64;
            let line_sample = (on_line * rate).round().max(0.0) as usize;
            if snaps[index] != Snap::None && (*beat - on_line).abs() <= LINE_TOLERANCE_S {
                continue;
            }
            let found = audio
                .kick_onset(line_sample, &mut hilbert)
                .filter(|rise| is_kick(rise, index))
                .map(|rise| audio.zero_crossing(rise.sample) as f64 / rate)
                .filter(|again| (again - on_line).abs() <= LINE_TOLERANCE_S);
            if let Some(again) = found {
                snaps[index] = Snap::Kick;
                *beat = again;
            } else {
                *beat = audio.zero_crossing(line_sample) as f64 / rate;
            }
        }
    }
    let snapped = snaps.iter().filter(|snap| **snap != Snap::None).count();
    let confidence = grid_confidence(GridEvidence {
        beat_strength: coarse.strength.iter().map(|s| f64::from(*s)).sum::<f64>() / count as f64,
        snapped_share: snapped as f64 / count as f64,
        phase_margin: phase.margin,
        spread_bpm: fit.spread_bpm,
    });
    Some(BeatEstimate {
        grid: BeatGrid {
            bpm: fit.bpm,
            beats_s,
            first_downbeat: first_downbeat as u32,
        },
        variable: fit.variable,
        confidence,
        phase,
        snapped,
    })
}

/// Snapped beats in seconds, with each beat that found nothing placed by index between its
/// snapped neighbours, or left where the model put it when it has none on one side.
fn interpolate(samples: &[Option<usize>], coarse_s: &[f64], rate: f64) -> Vec<f64> {
    let known: Vec<(usize, f64)> = samples
        .iter()
        .enumerate()
        .filter_map(|(index, sample)| sample.map(|s| (index, s as f64 / rate)))
        .collect();
    let mut next = 0;
    (0..samples.len())
        .map(|index| {
            if let Some(sample) = samples[index] {
                return sample as f64 / rate;
            }
            while next < known.len() && known[next].0 < index {
                next += 1;
            }
            match (next.checked_sub(1).map(|i| known[i]), known.get(next)) {
                (Some((before, at)), Some(&(after, then))) => {
                    at + (then - at) * (index - before) as f64 / (after - before) as f64
                }
                _ => coarse_s[index],
            }
        })
        .collect()
}

/// The beat tracker: `Beat This!` for the coarse beats, then the refinement above.
pub struct BeatTracker {
    session: ModelSession,
}

impl BeatTracker {
    pub fn new(session: ModelSession) -> Self {
        Self { session }
    }

    pub fn track(&mut self, audio: &BeatAudio) -> Result<Option<BeatEstimate>, AnalysisError> {
        let mel = audio.log_mel()?;
        let activations = activations(&mut self.session, &mel)?;
        Ok(estimate_grid(audio, &coarse_beats(&activations)))
    }
}

impl Analyser for BeatTracker {
    fn name(&self) -> &'static str {
        "beat_grid"
    }

    fn version(&self) -> &'static str {
        "1"
    }

    fn param_hash(&self) -> [u8; 32] {
        blake3::hash(SETTINGS.as_bytes()).into()
    }

    fn tier(&self) -> AnalyserTier {
        AnalyserTier::Gpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// torchaudio's values for the test tone in `tools/models/verify_beat_this.py`.
    #[test]
    fn the_log_mel_matches_the_training_front_end() {
        let tone: Vec<f32> = (0..44_100)
            .map(|n| {
                let t = f64::from(n) / 22_050.0;
                (0.5 * (std::f64::consts::TAU * 220.0 * t).sin()
                    + 0.25 * (std::f64::consts::TAU * 3_000.0 * t).sin()) as f32
            })
            .collect();
        let mel = LogMel::new().spectrogram(&tone);
        assert_eq!(mel.len(), 101 * MEL_BINS);
        let expected = [
            0.994_008, 0.518_913, 0.040_220, 0.015_340, 0.019_578, 3.982_67, 0.005_187, 0.000_355,
        ];
        for frame in [10, 50] {
            for (i, want) in expected.iter().enumerate() {
                let found = mel[frame * MEL_BINS + i * 16];
                assert!(
                    (found - want).abs() < 1e-3,
                    "frame {frame} bin {}: {found} against {want}",
                    i * 16
                );
            }
        }
    }

    #[test]
    fn the_beat_path_follows_the_peaks_at_the_tempo() {
        // Peaks every 23.4375 frames: 128 BPM.
        let frames = 1_500;
        let mut beat = vec![-6.0f32; frames];
        let mut downbeat = vec![-6.0f32; frames];
        let truth: Vec<f64> = (0..64).map(|i| 10.0 + i as f64 * 23.4375).collect();
        for (i, frame) in truth.iter().enumerate() {
            let at = frame.round() as usize;
            if at < frames {
                beat[at] = 6.0;
                if i % 4 == 0 {
                    downbeat[at] = 6.0;
                }
            }
        }
        let coarse = coarse_beats(&Activations { beat, downbeat });
        assert_eq!(coarse.beats_s.len(), 64);
        for (found, want) in coarse.beats_s.iter().zip(&truth) {
            assert!((found - want / MEL_FRAMES_PER_SECOND).abs() <= 0.011);
        }
        assert!(coarse.downbeat_logit[0] > 0.0 && coarse.downbeat_logit[1] < 0.0);
    }
}
