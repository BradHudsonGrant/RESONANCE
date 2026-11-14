//! The waveform summary that views draw from: for each short slice of a track, its loudest sample
//! and its RMS level.

/// Slices per second of audio. A deck showing ten seconds across a 2,400-pixel lane needs 240
/// columns a second, so each one-pixel bar draws from one or two slices.
pub const BINS_PER_SECOND: u32 = 300;

/// One slice. Both levels are linear amplitude, with 255 at full scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bin {
    pub peak: u8,
    pub rms: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waveform {
    pub bins: Vec<Bin>,
    /// Frames of audio summarised, at the source's own sample rate.
    pub frames: u64,
    pub sample_rate: u32,
}

impl Waveform {
    pub fn duration_seconds(&self) -> f64 {
        self.frames as f64 / f64::from(self.sample_rate.max(1))
    }
}

/// Builds a [`Waveform`] from interleaved samples as they are decoded.
#[derive(Debug)]
pub struct Summariser {
    sample_rate: u32,
    channels: usize,
    frames: u64,
    bin: u64,
    peak: f32,
    sum_of_squares: f64,
    samples: u32,
    bins: Vec<Bin>,
}

impl Summariser {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        Self {
            sample_rate: sample_rate.max(1),
            channels: channels.max(1),
            frames: 0,
            bin: 0,
            peak: 0.0,
            sum_of_squares: 0.0,
            samples: 0,
            bins: Vec::new(),
        }
    }

    pub fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            let bin = self.frames * u64::from(BINS_PER_SECOND) / u64::from(self.sample_rate);
            if bin != self.bin {
                self.close_bin();
                self.bin = bin;
            }
            for &sample in frame {
                self.peak = self.peak.max(sample.abs());
                self.sum_of_squares += f64::from(sample) * f64::from(sample);
                self.samples += 1;
            }
            self.frames += 1;
        }
    }

    pub fn finish(mut self) -> Waveform {
        if self.samples > 0 {
            self.close_bin();
        }
        Waveform {
            bins: self.bins,
            frames: self.frames,
            sample_rate: self.sample_rate,
        }
    }

    fn close_bin(&mut self) {
        let rms = (self.sum_of_squares / f64::from(self.samples.max(1))).sqrt() as f32;
        self.bins.push(Bin {
            peak: level(self.peak),
            rms: level(rms),
        });
        self.peak = 0.0;
        self.sum_of_squares = 0.0;
        self.samples = 0;
    }
}

fn level(amplitude: f32) -> u8 {
    (amplitude.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summarise(sample_rate: u32, channels: usize, samples: &[f32]) -> Waveform {
        let mut summariser = Summariser::new(sample_rate, channels);
        summariser.push(samples);
        summariser.finish()
    }

    #[test]
    fn one_second_makes_one_bin_per_slice() {
        let silence = vec![0.0; 44_100 * 2];

        let waveform = summarise(44_100, 2, &silence);

        assert_eq!(waveform.bins.len(), BINS_PER_SECOND as usize);
        assert_eq!(waveform.frames, 44_100);
        assert!((waveform.duration_seconds() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_full_scale_square_wave_has_full_peak_and_full_rms() {
        let square: Vec<f32> = (0..48_000)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();

        let waveform = summarise(48_000, 1, &square);

        assert!(waveform.bins.iter().all(|bin| *bin
            == Bin {
                peak: 255,
                rms: 255
            }));
    }

    #[test]
    fn a_sine_has_an_rms_level_of_peak_over_root_two() {
        // At 300 Hz one cycle fills one slice exactly.
        let sine: Vec<f32> = (0..48_000)
            .map(|i| (i as f32 * 300.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5)
            .collect();

        let waveform = summarise(48_000, 1, &sine);

        let bin = waveform.bins[10];
        assert_eq!(bin.peak, 128);
        assert_eq!(bin.rms, 90);
    }

    #[test]
    fn samples_pushed_in_pieces_summarise_as_they_would_in_one() {
        let ramp: Vec<f32> = (0..9_000).map(|i| (i as f32 / 9_000.0) - 0.5).collect();
        let mut pieces = Summariser::new(22_050, 2);
        for piece in ramp.chunks(338) {
            pieces.push(piece);
        }

        assert_eq!(pieces.finish(), summarise(22_050, 2, &ramp));
    }

    #[test]
    fn a_short_last_slice_still_makes_a_bin() {
        let waveform = summarise(48_000, 1, &[0.25; 10]);

        assert_eq!(waveform.bins, vec![Bin { peak: 64, rms: 64 }]);
    }
}
