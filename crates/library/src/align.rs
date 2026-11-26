//! Lines up two files of the same recording to a fraction of a millisecond, so cues, grids and
//! phrases can move from one to the other.
//!
//! Both files are decoded to mono and the first and last 45 s of each resampled to 44.1 kHz.
//! A spectral-flux onset envelope at 1 kHz, cross-correlated over 10 s either way, gives a lag
//! good to about a millisecond. A high-passed sample cross-correlation within 256 samples of
//! that lag, with a parabola through the peak, gives the lag to a fraction of a sample. Doing
//! the same at the end of the track shows whether one file plays at another speed.

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::fmt;
use std::path::Path;

use resonance_core::identity::MeasuredAlignment;
use rubato::{FftFixedIn, Resampler};
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;

use crate::identity::{LibraryAudioError, decode_mono};

/// Every excerpt is compared at this rate.
pub const REFERENCE_RATE: u32 = 44_100;
const EXCERPT_S: f64 = 45.0;
const ENVELOPE_RATE: f64 = 1_000.0;
const ENVELOPE_WINDOW: usize = 256;
const COARSE_MAX_LAG_S: f64 = 10.0;
const FINE_WINDOW_S: f64 = 10.0;
const FINE_MIN_WINDOW_S: f64 = 2.0;
const FINE_MAX_LAG: usize = 256;
/// Removes DC and the sub-bass whose phase lossy codecs smear.
const HIGH_PASS_HZ: f64 = 80.0;
/// Start and end lags this close mean both files play at one speed.
const SAME_SPEED_SAMPLES: f64 = 2.0;
/// The end is only measured when it is this far from the start, or the speed is guesswork.
const MIN_SPAN_FOR_SPEED_S: f64 = 30.0;

#[derive(Debug)]
pub enum AlignError {
    Decode(LibraryAudioError),
    Resample(String),
    /// Too short, or silent where it was measured.
    NothingToMeasure,
}

impl fmt::Display for AlignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(source) => source.fmt(f),
            Self::Resample(reason) => write!(f, "The audio could not be resampled: {reason}"),
            Self::NothingToMeasure => {
                f.write_str("The files are too short or too quiet to line up.")
            }
        }
    }
}

impl std::error::Error for AlignError {}

impl From<LibraryAudioError> for AlignError {
    fn from(source: LibraryAudioError) -> Self {
        Self::Decode(source)
    }
}

/// The opening and closing stretch of one file at the reference rate.
struct Excerpts {
    head: Vec<f32>,
    tail: Vec<f32>,
    /// Where `tail` starts in the file, in seconds.
    tail_start_s: f64,
}

fn excerpts(path: &Path) -> Result<Excerpts, AlignError> {
    let mut head: Vec<f32> = Vec::new();
    let mut tail: VecDeque<f32> = VecDeque::new();
    let mut total = 0usize;
    let sample_rate = decode_mono(path, |sample_rate, mono| {
        let excerpt_frames = (EXCERPT_S * f64::from(sample_rate)).round() as usize;
        for &sample in mono {
            if head.len() < excerpt_frames {
                head.push(sample);
            }
            if tail.len() == excerpt_frames {
                tail.pop_front();
            }
            tail.push_back(sample);
        }
        total += mono.len();
    })?;
    let tail: Vec<f32> = tail.into();
    let tail_start_s = (total - tail.len()) as f64 / f64::from(sample_rate);
    Ok(Excerpts {
        head: resample(&head, sample_rate)?,
        tail: resample(&tail, sample_rate)?,
        tail_start_s,
    })
}

fn resample(samples: &[f32], sample_rate: u32) -> Result<Vec<f32>, AlignError> {
    if sample_rate == REFERENCE_RATE || samples.is_empty() {
        return Ok(samples.to_vec());
    }
    let wanted = (samples.len() as f64 * f64::from(REFERENCE_RATE) / f64::from(sample_rate)).round()
        as usize;
    let mut resampler =
        FftFixedIn::<f32>::new(sample_rate as usize, REFERENCE_RATE as usize, 1024, 2, 1)
            .map_err(|error| AlignError::Resample(error.to_string()))?;
    let delay = resampler.output_delay();
    let fail = |error: rubato::ResampleError| AlignError::Resample(error.to_string());
    let mut out: Vec<f32> = Vec::with_capacity(wanted + delay + 4096);
    let mut position = 0;
    while samples.len() - position >= resampler.input_frames_next() {
        let take = resampler.input_frames_next();
        let chunk = resampler
            .process(&[&samples[position..position + take]], None)
            .map_err(fail)?;
        out.extend_from_slice(&chunk[0]);
        position += take;
    }
    let rest = &samples[position..];
    let chunk = resampler
        .process_partial(Some(&[rest]), None)
        .map_err(fail)?;
    out.extend_from_slice(&chunk[0]);
    while out.len() < wanted + delay {
        let chunk = resampler
            .process_partial::<&[f32]>(None, None)
            .map_err(fail)?;
        if chunk[0].is_empty() {
            break;
        }
        out.extend_from_slice(&chunk[0]);
    }
    let end = (wanted + delay).min(out.len());
    Ok(out[delay.min(end)..end].to_vec())
}

/// A second-order Butterworth high-pass. Both files go through the same filter, so its phase
/// shift cancels out of the lag.
fn high_pass(samples: &[f32]) -> Vec<f64> {
    let omega = 2.0 * PI * HIGH_PASS_HZ / f64::from(REFERENCE_RATE);
    let alpha = omega.sin() / 2.0_f64.sqrt();
    let cos = omega.cos();
    let a0 = 1.0 + alpha;
    let b0 = (1.0 + cos) / 2.0 / a0;
    let b1 = -(1.0 + cos) / a0;
    let b2 = b0;
    let a1 = -2.0 * cos / a0;
    let a2 = (1.0 - alpha) / a0;
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    samples
        .iter()
        .map(|&sample| {
            let x0 = f64::from(sample);
            let y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x0, y1, y0);
            y0
        })
        .collect()
}

/// Half-wave rectified spectral flux, one value per millisecond.
fn onset_envelope(samples: &[f32]) -> Vec<f64> {
    let hop = f64::from(REFERENCE_RATE) / ENVELOPE_RATE;
    if samples.len() < ENVELOPE_WINDOW {
        return Vec::new();
    }
    let frames = ((samples.len() - ENVELOPE_WINDOW) as f64 / hop).floor() as usize + 1;
    let fft = FftPlanner::<f32>::new().plan_fft_forward(ENVELOPE_WINDOW);
    let window: Vec<f32> = (0..ENVELOPE_WINDOW)
        .map(|n| (0.5 - 0.5 * (2.0 * PI * n as f64 / ENVELOPE_WINDOW as f64).cos()) as f32)
        .collect();
    let bins = ENVELOPE_WINDOW / 2 + 1;
    let mut previous = vec![0.0f32; bins];
    let mut buffer = vec![Complex::new(0.0f32, 0.0); ENVELOPE_WINDOW];
    let mut scratch = vec![Complex::new(0.0f32, 0.0); fft.get_inplace_scratch_len()];
    let mut envelope = Vec::with_capacity(frames);
    for frame in 0..frames {
        let start = (frame as f64 * hop).round() as usize;
        for (slot, (&sample, &weight)) in buffer
            .iter_mut()
            .zip(samples[start..start + ENVELOPE_WINDOW].iter().zip(&window))
        {
            *slot = Complex::new(sample * weight, 0.0);
        }
        fft.process_with_scratch(&mut buffer, &mut scratch);
        let mut flux = 0.0f64;
        for (bin, last) in buffer[..bins].iter().zip(previous.iter_mut()) {
            let level = (1.0 + 1000.0 * bin.norm()).ln();
            flux += f64::from((level - *last).max(0.0));
            *last = level;
        }
        envelope.push(if frame == 0 { 0.0 } else { flux });
    }
    envelope
}

/// `correlation[k]` is the sum of `a[i] * b[i + k]` for every `k` in `-max_lag..=max_lag`,
/// at index `k + max_lag`.
fn cross_correlate(a: &[f64], b: &[f64], max_lag: usize) -> Vec<f64> {
    let size = (a.len() + b.len() + 1).next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(size);
    let inverse = planner.plan_fft_inverse(size);
    let mut left: Vec<Complex<f64>> = a.iter().map(|&x| Complex::new(x, 0.0)).collect();
    left.resize(size, Complex::new(0.0, 0.0));
    let mut right: Vec<Complex<f64>> = b.iter().map(|&x| Complex::new(x, 0.0)).collect();
    right.resize(size, Complex::new(0.0, 0.0));
    forward.process(&mut left);
    forward.process(&mut right);
    for (l, r) in left.iter_mut().zip(&right) {
        *l = l.conj() * r;
    }
    inverse.process(&mut left);
    let scale = size as f64;
    (0..=2 * max_lag)
        .map(|index| {
            let lag = index as isize - max_lag as isize;
            let at = if lag >= 0 {
                lag as usize
            } else {
                size - lag.unsigned_abs()
            };
            left[at].re / scale
        })
        .collect()
}

/// The offset of the peak from the middle sample, from a parabola through it and its
/// neighbours.
fn parabolic_peak(left: f64, centre: f64, right: f64) -> f64 {
    let curvature = left - 2.0 * centre + right;
    if curvature.abs() < f64::EPSILON {
        return 0.0;
    }
    (0.5 * (left - right) / curvature).clamp(-0.5, 0.5)
}

/// Where `b` holds `a`'s audio: the lag in samples, so that `a[i]` is heard at `b[i + lag]`,
/// the point in `a` it was measured around, and the correlation at the peak.
struct Lag {
    samples: f64,
    at: f64,
    confidence: f32,
}

/// Spreads each onset over a few milliseconds, so a hit that lands across two envelope frames
/// in one file and in one frame in the other still lines up.
fn smoothed(envelope: &[f64]) -> Vec<f64> {
    const REACH: usize = 4;
    (0..envelope.len())
        .map(|centre| {
            let from = centre.saturating_sub(REACH);
            let to = (centre + REACH).min(envelope.len() - 1);
            (from..=to)
                .map(|index| {
                    let weight = (REACH + 1 - centre.abs_diff(index)) as f64;
                    weight * envelope[index]
                })
                .sum()
        })
        .collect()
}

/// The lags, in samples, at which `b`'s onsets best match `a`'s, strongest first. Music that
/// loops makes a lag a bar or two away score nearly as well as the right one, so several are
/// kept for the sample correlation to decide between.
fn coarse_candidates(a: &[f32], b: &[f32]) -> Vec<f64> {
    const CANDIDATES: usize = 5;
    const APART_FRAMES: usize = 20;
    let mut envelopes = [smoothed(&onset_envelope(a)), smoothed(&onset_envelope(b))];
    if envelopes.iter().any(Vec::is_empty) {
        return Vec::new();
    }
    for envelope in &mut envelopes {
        let mean = envelope.iter().sum::<f64>() / envelope.len() as f64;
        envelope.iter_mut().for_each(|value| *value -= mean);
    }
    let max_lag = (COARSE_MAX_LAG_S * ENVELOPE_RATE) as usize;
    let correlation = cross_correlate(&envelopes[0], &envelopes[1], max_lag);
    let mut peaks: Vec<usize> = (1..correlation.len() - 1)
        .filter(|&index| {
            correlation[index] > 0.0
                && correlation[index] >= correlation[index - 1]
                && correlation[index] >= correlation[index + 1]
        })
        .collect();
    peaks.sort_by(|&x, &y| correlation[y].total_cmp(&correlation[x]));
    let mut chosen: Vec<usize> = Vec::with_capacity(CANDIDATES);
    for peak in peaks {
        if chosen
            .iter()
            .all(|&taken| taken.abs_diff(peak) >= APART_FRAMES)
        {
            chosen.push(peak);
            if chosen.len() == CANDIDATES {
                break;
            }
        }
    }
    chosen
        .into_iter()
        .map(|peak| (peak as f64 - max_lag as f64) * f64::from(REFERENCE_RATE) / ENVELOPE_RATE)
        .collect()
}

/// The start of the first passage loud enough to carry transients, less half a second.
fn first_strong_passage(samples: &[f64]) -> usize {
    let block = (f64::from(REFERENCE_RATE) / 100.0) as usize;
    let energies: Vec<f64> = samples
        .chunks(block)
        .map(|chunk| chunk.iter().map(|x| x * x).sum())
        .collect();
    let loudest = energies.iter().copied().fold(0.0, f64::max);
    let first = energies
        .iter()
        .position(|&energy| energy >= loudest * 0.25)
        .unwrap_or(0);
    (first * block).saturating_sub(REFERENCE_RATE as usize / 2)
}

/// Correlates `window_s` of high-passed `a`, from its first strong passage, with `b` within
/// [`FINE_MAX_LAG`] samples of `guess`.
fn fine_lag(a: &[f64], b: &[f64], guess: f64, window_s: f64) -> Option<Lag> {
    let coarse = guess.round() as isize;
    let reach = FINE_MAX_LAG as isize;
    // `a[start..start + len]` against `b[start + coarse - reach..start + coarse + len + reach]`.
    let earliest = (reach - coarse).max(0) as usize;
    let latest_end_a = a
        .len()
        .min((b.len() as isize - coarse - reach).max(0) as usize);
    let mut start = first_strong_passage(a).max(earliest);
    let mut len = (window_s * f64::from(REFERENCE_RATE)) as usize;
    if start + len > latest_end_a {
        start = latest_end_a.saturating_sub(len).max(earliest);
        len = latest_end_a.saturating_sub(start);
    }
    if len < (FINE_MIN_WINDOW_S * f64::from(REFERENCE_RATE)) as usize {
        return None;
    }
    let segment_a = &a[start..start + len];
    let from_b = (start as isize + coarse - reach) as usize;
    let segment_b = &b[from_b..from_b + len + 2 * FINE_MAX_LAG];
    let raw = cross_correlate(segment_a, segment_b, 2 * FINE_MAX_LAG);
    // Only non-negative shifts of `segment_b` are wanted: index `k` is lag `coarse - reach + k`.
    let raw = &raw[2 * FINE_MAX_LAG..];
    let energy_a: f64 = segment_a.iter().map(|x| x * x).sum();
    let mut prefix = vec![0.0f64; segment_b.len() + 1];
    for (index, x) in segment_b.iter().enumerate() {
        prefix[index + 1] = prefix[index] + x * x;
    }
    let normalised: Vec<f64> = (0..=2 * FINE_MAX_LAG)
        .map(|k| {
            let energy_b = prefix[k + len] - prefix[k];
            let scale = (energy_a * energy_b).sqrt();
            if scale > 0.0 { raw[k] / scale } else { 0.0 }
        })
        .collect();
    let (peak, &best) = normalised
        .iter()
        .enumerate()
        .max_by(|(_, x), (_, y)| x.total_cmp(y))?;
    let refined = if peak > 0 && peak < normalised.len() - 1 {
        peak as f64 + parabolic_peak(normalised[peak - 1], best, normalised[peak + 1])
    } else {
        peak as f64
    };
    Some(Lag {
        samples: (coarse - reach) as f64 + refined,
        at: (start as f64 + len as f64 / 2.0) / f64::from(REFERENCE_RATE),
        confidence: best.clamp(0.0, 1.0) as f32,
    })
}

/// One end of both files, high-passed, with the onset lags worth trying.
struct End {
    a: Vec<f64>,
    b: Vec<f64>,
    candidates: Vec<f64>,
}

impl End {
    fn new(a: &[f32], b: &[f32]) -> Self {
        Self {
            candidates: coarse_candidates(a, b),
            a: high_pass(a),
            b: high_pass(b),
        }
    }

    /// The candidate whose samples agree best.
    fn best(&self, window_s: f64) -> Option<Lag> {
        self.candidates
            .iter()
            .filter_map(|&guess| fine_lag(&self.a, &self.b, guess, window_s))
            .max_by(|x, y| x.confidence.total_cmp(&y.confidence))
    }
}

/// Where a moment `at` seconds into the old file sits in the new one, from two measured lags.
fn through(x1: f64, y1: f64, x2: f64, y2: f64) -> MeasuredAlignment {
    let old_per_new = (x2 - x1) / (y2 - y1);
    MeasuredAlignment {
        new_at_old_zero_s: y1 - x1 / old_per_new,
        old_per_new,
        confidence: 0.0,
    }
}

/// Measures where `old`'s audio sits in `new`.
pub fn measure_alignment(old: &Path, new: &Path) -> Result<MeasuredAlignment, AlignError> {
    let old = excerpts(old)?;
    let new = excerpts(new)?;
    let rate = f64::from(REFERENCE_RATE);
    let head = End::new(&old.head, &new.head);
    let start = head.best(FINE_WINDOW_S);
    let single = start.as_ref().map(|start| MeasuredAlignment {
        new_at_old_zero_s: start.samples / rate,
        old_per_new: 1.0,
        confidence: start.confidence,
    });
    let midpoint = EXCERPT_S / 2.0;
    if old.tail_start_s < MIN_SPAN_FOR_SPEED_S {
        return single.ok_or(AlignError::NothingToMeasure);
    }
    let tail = End::new(&old.tail, &new.tail);
    let end = tail.best(FINE_WINDOW_S);
    if let (Some(start), Some(end)) = (&start, &end) {
        let drift = (new.tail_start_s - old.tail_start_s) * rate + end.samples - start.samples;
        let confidence = start.confidence.min(end.confidence);
        if drift.abs() <= SAME_SPEED_SAMPLES && end.confidence >= start.confidence.min(0.5) {
            return Ok(MeasuredAlignment {
                new_at_old_zero_s: (start.samples / rate + new.tail_start_s + end.samples / rate
                    - old.tail_start_s)
                    / 2.0,
                old_per_new: 1.0,
                confidence,
            });
        }
    }
    // The ends disagree, so one file plays at another speed and a 10 s window smears. A line
    // through the onset lags, measured at the middle of each excerpt, predicts the lag closely
    // enough to measure it again over 2 s at each end.
    let (Some(&head_guess), Some(&tail_guess)) = (head.candidates.first(), tail.candidates.first())
    else {
        return single.ok_or(AlignError::NothingToMeasure);
    };
    let rough = through(
        midpoint,
        midpoint + head_guess / rate,
        old.tail_start_s + midpoint,
        new.tail_start_s + midpoint + tail_guess / rate,
    );
    let predicted = |old_s: f64, new_excerpt_start_s: f64, old_excerpt_start_s: f64| {
        (rough.new_seconds(old_s) - new_excerpt_start_s - (old_s - old_excerpt_start_s)) * rate
    };
    let head_at = first_strong_passage(&head.a) as f64 / rate + FINE_MIN_WINDOW_S / 2.0;
    let tail_at =
        old.tail_start_s + first_strong_passage(&tail.a) as f64 / rate + FINE_MIN_WINDOW_S / 2.0;
    let short_start = fine_lag(
        &head.a,
        &head.b,
        predicted(head_at, 0.0, 0.0),
        FINE_MIN_WINDOW_S,
    );
    let short_end = fine_lag(
        &tail.a,
        &tail.b,
        predicted(tail_at, new.tail_start_s, old.tail_start_s),
        FINE_MIN_WINDOW_S,
    );
    match (short_start, short_end) {
        (Some(start), Some(end)) => {
            let x1 = start.at;
            let x2 = old.tail_start_s + end.at;
            let mut measured = through(
                x1,
                x1 + start.samples / rate,
                x2,
                new.tail_start_s + end.at + end.samples / rate,
            );
            measured.confidence = start.confidence.min(end.confidence);
            Ok(measured)
        }
        _ => single.ok_or(AlignError::NothingToMeasure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parabola_finds_the_peak_between_samples() {
        // Samples of -(x - 0.3)^2 at -1, 0 and 1.
        let at = |x: f64| -(x - 0.3) * (x - 0.3);
        let offset = parabolic_peak(at(-1.0), at(0.0), at(1.0));
        assert!((offset - 0.3).abs() < 1e-12);
    }

    #[test]
    fn cross_correlation_peaks_at_the_lag() {
        let a: Vec<f64> = (0..64).map(|n| ((n * 7919) % 13) as f64 - 6.0).collect();
        let mut b = vec![0.0; 5];
        b.extend_from_slice(&a);
        let correlation = cross_correlate(&a, &b, 8);
        let peak = correlation
            .iter()
            .enumerate()
            .max_by(|(_, x), (_, y)| x.total_cmp(y))
            .map(|(index, _)| index as isize - 8);
        assert_eq!(peak, Some(5));
    }

    #[test]
    fn resampling_keeps_time() {
        // A 1 kHz tone at 48 kHz comes out at 44.1 kHz with its phase where time puts it.
        let tone = |rate: f64, n: usize| (2.0 * PI * 1000.0 * n as f64 / rate).sin() as f32;
        let input: Vec<f32> = (0..48_000).map(|n| tone(48_000.0, n)).collect();
        let output = resample(&input, 48_000).expect("resampled");
        assert_eq!(output.len(), 44_100);
        for (n, sample) in output.iter().enumerate().take(42_000).skip(2_000) {
            assert!((sample - tone(44_100.0, n)).abs() < 0.01, "sample {n}");
        }
    }
}
