//! Measures loudness, peaks, clipping, dropouts and the high end of the spectrum while the
//! identity pass decodes a file, so quality costs no second decode. The rules that judge the
//! numbers are in `resonance_core::quality`.

use std::f64::consts::PI;
use std::sync::Arc;

use ebur128::{EbuR128, Mode};
use resonance_core::quality::{
    ClipRuns, DropoutFinder, QualityMeasurements, SPECTRUM_BAND_HZ, SPECTRUM_HIGH_HZ,
    SPECTRUM_LOW_HZ, spectral_cliff,
};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

const FFT_SIZE: usize = 4_096;
/// About 18 minutes at 44.1 kHz. Longer files are judged on their first 18 minutes.
const MAX_SPECTRUM_FRAMES: usize = 12_000;
/// Quieter FFT frames say nothing about the encoder's low-pass and are left out.
const SPECTRUM_GATE_DBFS: f64 = -40.0;
/// The level in each band that only the loudest tenth of frames beats.
const SPECTRUM_PERCENTILE: f64 = 0.9;
/// Crest factors are taken over blocks this long that are louder than `ACTIVE_BLOCK_DBFS`.
const CREST_BLOCK_MS: u32 = 400;
const ACTIVE_BLOCK_DBFS: f64 = -35.0;
const LOW_BAND_HZ: f64 = 150.0;
const MID_BAND_HZ: (f64, f64) = (400.0, 4_000.0);

/// One pass over a file's decoded samples.
pub struct QualityMeter {
    channels: usize,
    loudness: Option<EbuR128>,
    clips: ClipRuns,
    dropouts: DropoutFinder,
    crest: Crest,
    spectrum: Option<Spectrum>,
}

impl QualityMeter {
    /// `spectrum` turns on the transcode check, which only lossless files need.
    pub fn new(sample_rate: u32, channels: usize, spectrum: bool) -> Self {
        let mode = Mode::I | Mode::LRA | Mode::SAMPLE_PEAK | Mode::TRUE_PEAK;
        let loudness = u32::try_from(channels)
            .ok()
            .and_then(|count| EbuR128::new(count, sample_rate, mode).ok());
        Self {
            channels,
            loudness,
            clips: ClipRuns::new(channels),
            dropouts: DropoutFinder::new(sample_rate, channels),
            crest: Crest::new(sample_rate),
            spectrum: spectrum.then(|| Spectrum::new(sample_rate)),
        }
    }

    /// Takes whole interleaved frames scaled to `-1.0..=1.0`.
    pub fn push(&mut self, interleaved: &[f32]) {
        if let Some(loudness) = &mut self.loudness
            && loudness.add_frames_f32(interleaved).is_err()
        {
            self.loudness = None;
        }
        let scale = 1.0 / self.channels as f32;
        for frame in interleaved.chunks_exact(self.channels) {
            self.clips.push_frame(frame);
            self.dropouts.push_frame(frame);
            let mono = frame.iter().sum::<f32>() * scale;
            self.crest.push(mono);
            if let Some(spectrum) = &mut self.spectrum {
                spectrum.push(mono);
            }
        }
    }

    pub fn finish(self) -> QualityMeasurements {
        let channels = u32::try_from(self.channels).unwrap_or(0);
        let finite = |value: Result<f64, ebur128::Error>| value.ok().filter(|v| v.is_finite());
        let peak_db = |peak: &dyn Fn(u32) -> Result<f64, ebur128::Error>| {
            (0..channels)
                .filter_map(|channel| peak(channel).ok())
                .fold(None, |best: Option<f64>, value| {
                    Some(best.map_or(value, |b| b.max(value)))
                })
                .map(amplitude_to_db)
                .filter(|db| db.is_finite())
        };
        let (integrated_lufs, loudness_range_lu, sample_peak_dbfs, true_peak_dbtp) =
            match &self.loudness {
                Some(meter) => (
                    finite(meter.loudness_global()),
                    finite(meter.loudness_range()),
                    peak_db(&|channel| meter.sample_peak(channel)),
                    peak_db(&|channel| meter.true_peak(channel)),
                ),
                None => (None, None, None, None),
            };
        let (crest_low_db, crest_mid_db) = self.crest.finish();
        QualityMeasurements {
            integrated_lufs,
            loudness_range_lu,
            sample_peak_dbfs,
            true_peak_dbtp,
            crest_low_db,
            crest_mid_db,
            clipped_runs: self.clips.runs(),
            dropouts: self.dropouts.dropouts(),
            first_dropout_ms: self.dropouts.first_dropout_ms(),
            spectral_cutoff_hz: self.spectrum.and_then(Spectrum::finish),
        }
    }
}

fn amplitude_to_db(amplitude: f64) -> f64 {
    20.0 * amplitude.log10()
}

fn power_to_db(power: f64) -> f64 {
    10.0 * power.max(1e-20).log10()
}

/// A second-order filter from the Audio EQ Cookbook.
#[derive(Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(sample_rate: u32, cutoff_hz: f64, high_pass: bool) -> Self {
        let w0 = 2.0 * PI * cutoff_hz / f64::from(sample_rate.max(1));
        let alpha = w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let cos = w0.cos();
        let (b0, b1, b2) = if high_pass {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        };
        let a0 = 1.0 + alpha;
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[derive(Clone, Copy, Default)]
struct BandSum {
    squares: f64,
    peak: f64,
}

impl BandSum {
    fn add(&mut self, x: f64) {
        self.squares += x * x;
        self.peak = self.peak.max(x.abs());
    }

    fn merge(&mut self, block: Self) {
        self.squares += block.squares;
        self.peak = self.peak.max(block.peak);
    }

    fn crest(self, samples: u64) -> Option<f64> {
        if samples == 0 || self.squares <= 0.0 || self.peak <= 0.0 {
            return None;
        }
        let rms = (self.squares / samples as f64).sqrt();
        Some(amplitude_to_db(self.peak / rms))
    }
}

/// Peak over RMS in the kick and bass band and in the vocal and lead band.
struct Crest {
    low_pass: Biquad,
    mid_high_pass: Biquad,
    mid_low_pass: Biquad,
    block_len: u64,
    in_block: u64,
    block_power: f64,
    block_low: BandSum,
    block_mid: BandSum,
    active_samples: u64,
    low: BandSum,
    mid: BandSum,
}

impl Crest {
    fn new(sample_rate: u32) -> Self {
        Self {
            low_pass: Biquad::new(sample_rate, LOW_BAND_HZ, false),
            mid_high_pass: Biquad::new(sample_rate, MID_BAND_HZ.0, true),
            mid_low_pass: Biquad::new(sample_rate, MID_BAND_HZ.1, false),
            block_len: (u64::from(sample_rate) * u64::from(CREST_BLOCK_MS) / 1000).max(1),
            in_block: 0,
            block_power: 0.0,
            block_low: BandSum::default(),
            block_mid: BandSum::default(),
            active_samples: 0,
            low: BandSum::default(),
            mid: BandSum::default(),
        }
    }

    fn push(&mut self, mono: f32) {
        let x = f64::from(mono);
        self.block_power += x * x;
        self.block_low.add(self.low_pass.process(x));
        let mid = self.mid_low_pass.process(self.mid_high_pass.process(x));
        self.block_mid.add(mid);
        self.in_block += 1;
        if self.in_block == self.block_len {
            self.end_block();
        }
    }

    fn end_block(&mut self) {
        let mean = self.block_power / self.in_block.max(1) as f64;
        if power_to_db(mean) > ACTIVE_BLOCK_DBFS {
            self.active_samples += self.in_block;
            self.low.merge(self.block_low);
            self.mid.merge(self.block_mid);
        }
        self.in_block = 0;
        self.block_power = 0.0;
        self.block_low = BandSum::default();
        self.block_mid = BandSum::default();
    }

    fn finish(self) -> (Option<f64>, Option<f64>) {
        (
            self.low.crest(self.active_samples),
            self.mid.crest(self.active_samples),
        )
    }
}

/// Band levels from 8 to 21.5 kHz over the loud frames, for finding an encoder's low-pass.
struct Spectrum {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    frame: Vec<f32>,
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// The FFT bins each band averages over.
    bands: Vec<(usize, usize)>,
    /// One list of levels per band, one level per loud frame.
    levels: Vec<Vec<f32>>,
    frames: usize,
    gate: f64,
    norm: f64,
}

impl Spectrum {
    fn new(sample_rate: u32) -> Self {
        let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|n| (0.5 - 0.5 * (2.0 * PI * n as f64 / FFT_SIZE as f64).cos()) as f32)
            .collect();
        let window_sum: f64 = window.iter().map(|&w| f64::from(w)).sum();
        let bin_hz = f64::from(sample_rate) / FFT_SIZE as f64;
        let nyquist = f64::from(sample_rate) / 2.0;
        let mut bands = Vec::new();
        let mut low = SPECTRUM_LOW_HZ;
        while low + SPECTRUM_BAND_HZ <= SPECTRUM_HIGH_HZ && low + SPECTRUM_BAND_HZ < nyquist {
            let first = (low / bin_hz).ceil() as usize;
            let last = (((low + SPECTRUM_BAND_HZ) / bin_hz).ceil() as usize).max(first + 1);
            bands.push((first, last));
            low += SPECTRUM_BAND_HZ;
        }
        let scratch_len = fft.get_inplace_scratch_len();
        Self {
            fft,
            window,
            frame: Vec::with_capacity(FFT_SIZE),
            buffer: vec![Complex::default(); FFT_SIZE],
            scratch: vec![Complex::default(); scratch_len],
            levels: vec![Vec::new(); bands.len()],
            bands,
            frames: 0,
            gate: 10f64.powf(SPECTRUM_GATE_DBFS / 10.0),
            // A full-scale sine reads 0 dB in its bin.
            norm: (2.0 / window_sum).powi(2),
        }
    }

    fn push(&mut self, mono: f32) {
        if self.frames >= MAX_SPECTRUM_FRAMES {
            return;
        }
        self.frame.push(mono);
        if self.frame.len() == FFT_SIZE {
            self.analyse_frame();
            self.frame.clear();
        }
    }

    fn analyse_frame(&mut self) {
        let power: f64 = self
            .frame
            .iter()
            .map(|&x| f64::from(x) * f64::from(x))
            .sum::<f64>()
            / FFT_SIZE as f64;
        if power < self.gate {
            return;
        }
        for ((slot, &x), &w) in self.buffer.iter_mut().zip(&self.frame).zip(&self.window) {
            *slot = Complex::new(x * w, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);
        for (levels, &(first, last)) in self.levels.iter_mut().zip(&self.bands) {
            let bins = &self.buffer[first..last.min(FFT_SIZE / 2)];
            if bins.is_empty() {
                continue;
            }
            let mean =
                bins.iter().map(|c| f64::from(c.norm_sqr())).sum::<f64>() / bins.len() as f64;
            levels.push(power_to_db(mean * self.norm) as f32);
        }
        self.frames += 1;
    }

    fn finish(mut self) -> Option<f64> {
        if self.frames == 0 {
            return None;
        }
        let bands: Vec<f32> = self
            .levels
            .iter_mut()
            .map(|levels| {
                if levels.is_empty() {
                    return f32::NEG_INFINITY;
                }
                let rank = ((levels.len() - 1) as f64 * SPECTRUM_PERCENTILE).round() as usize;
                *levels.select_nth_unstable_by(rank, f32::total_cmp).1
            })
            .collect();
        spectral_cliff(&bands)
    }
}
