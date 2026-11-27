//! The waveform summary that views draw from: for each short slice of a track, its loudest sample
//! and the RMS level of its low, mid and high bands.
//!
//! The bands split at 150 Hz and 2.5 kHz with fourth-order Linkwitz-Riley filters on the mono
//! mix: kick and bass below, the body of most instruments and voices between, hats and air above.

/// Slices per second of audio. A deck showing ten seconds across a 2,400-pixel lane needs 240
/// columns a second, so each one-pixel bar draws from one or two slices.
pub const BINS_PER_SECOND: u32 = 300;

/// Columns in the overview a table row, the dock and the inspector draw.
pub const OVERVIEW_COLUMNS: usize = 140;

/// Bytes in one encoded bin: peak, low, mid and high.
pub const BIN_BYTES: usize = 4;

pub const LOW_MID_CROSSOVER_HZ: f64 = 150.0;
pub const MID_HIGH_CROSSOVER_HZ: f64 = 2_500.0;

/// One slice. Every level is linear amplitude, with 255 at full scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bin {
    pub peak: u8,
    pub low: u8,
    pub mid: u8,
    pub high: u8,
}

impl Bin {
    pub const fn to_bytes(self) -> [u8; BIN_BYTES] {
        [self.peak, self.low, self.mid, self.high]
    }

    pub const fn from_bytes([peak, low, mid, high]: [u8; BIN_BYTES]) -> Self {
        Self {
            peak,
            low,
            mid,
            high,
        }
    }

    fn louder(self, other: Self) -> Self {
        Self {
            peak: self.peak.max(other.peak),
            low: self.low.max(other.low),
            mid: self.mid.max(other.mid),
            high: self.high.max(other.high),
        }
    }
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

    /// The whole track in [`OVERVIEW_COLUMNS`] columns. Each column keeps the loudest of the bins
    /// it covers, so a short hit still shows.
    pub fn overview(&self) -> Overview {
        let mut columns = [Bin::default(); OVERVIEW_COLUMNS];
        let count = self.bins.len();
        if count == 0 {
            return Overview { columns };
        }
        for (index, column) in columns.iter_mut().enumerate() {
            let from = index * count / OVERVIEW_COLUMNS;
            let to = ((index + 1) * count / OVERVIEW_COLUMNS).max(from + 1).min(count);
            *column = self.bins[from.min(count - 1)..to]
                .iter()
                .fold(Bin::default(), |loudest, bin| loudest.louder(*bin));
        }
        Overview { columns }
    }
}

/// A track's waveform at the size a table row draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overview {
    pub columns: [Bin; OVERVIEW_COLUMNS],
}

impl Overview {
    pub const BYTES: usize = OVERVIEW_COLUMNS * BIN_BYTES;

    pub fn to_bytes(&self) -> Vec<u8> {
        self.columns.iter().flat_map(|bin| bin.to_bytes()).collect()
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (chunks, rest) = bytes.as_chunks::<BIN_BYTES>();
        if chunks.len() != OVERVIEW_COLUMNS || !rest.is_empty() {
            return None;
        }
        let mut columns = [Bin::default(); OVERVIEW_COLUMNS];
        for (column, chunk) in columns.iter_mut().zip(chunks) {
            *column = Bin::from_bytes(*chunk);
        }
        Some(Self { columns })
    }
}

/// A second-order section in direct form I. Coefficients are normalised so `a0` is 1.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    /// A Butterworth section (`Q` of one over root two) from the audio EQ cookbook. Two in series
    /// make a Linkwitz-Riley filter.
    fn butterworth(sample_rate: u32, cutoff_hz: f64, high_pass: bool) -> Self {
        let nyquist_safe = cutoff_hz.min(f64::from(sample_rate) * 0.45);
        let omega = std::f64::consts::TAU * nyquist_safe / f64::from(sample_rate.max(1));
        let alpha = omega.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let cos = omega.cos();
        let a0 = 1.0 + alpha;
        let b = if high_pass {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        } else {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        };
        Self {
            b: b.map(|value| value / a0),
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn process(&mut self, input: f64) -> f64 {
        let output = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [output, self.y[0]];
        output
    }
}

/// A fourth-order Linkwitz-Riley low or high pass.
#[derive(Debug, Clone, Copy)]
struct LinkwitzRiley([Biquad; 2]);

impl LinkwitzRiley {
    fn new(sample_rate: u32, cutoff_hz: f64, high_pass: bool) -> Self {
        let section = Biquad::butterworth(sample_rate, cutoff_hz, high_pass);
        Self([section, section])
    }

    fn process(&mut self, input: f64) -> f64 {
        let [first, second] = &mut self.0;
        second.process(first.process(input))
    }
}

#[derive(Debug)]
struct BandSplit {
    low: LinkwitzRiley,
    mid_high_pass: LinkwitzRiley,
    mid_low_pass: LinkwitzRiley,
    high: LinkwitzRiley,
}

impl BandSplit {
    fn new(sample_rate: u32) -> Self {
        Self {
            low: LinkwitzRiley::new(sample_rate, LOW_MID_CROSSOVER_HZ, false),
            mid_high_pass: LinkwitzRiley::new(sample_rate, LOW_MID_CROSSOVER_HZ, true),
            mid_low_pass: LinkwitzRiley::new(sample_rate, MID_HIGH_CROSSOVER_HZ, false),
            high: LinkwitzRiley::new(sample_rate, MID_HIGH_CROSSOVER_HZ, true),
        }
    }

    fn split(&mut self, mono: f64) -> [f64; 3] {
        let mid = self.mid_low_pass.process(self.mid_high_pass.process(mono));
        [self.low.process(mono), mid, self.high.process(mono)]
    }
}

/// Builds a [`Waveform`] from interleaved samples as they are decoded.
#[derive(Debug)]
pub struct Summariser {
    sample_rate: u32,
    channels: usize,
    bands: BandSplit,
    frames: u64,
    bin: u64,
    peak: f32,
    band_squares: [f64; 3],
    bin_frames: u32,
    bins: Vec<Bin>,
}

impl Summariser {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        let sample_rate = sample_rate.max(1);
        Self {
            sample_rate,
            channels: channels.max(1),
            bands: BandSplit::new(sample_rate),
            frames: 0,
            bin: 0,
            peak: 0.0,
            band_squares: [0.0; 3],
            bin_frames: 0,
            bins: Vec::new(),
        }
    }

    pub fn push(&mut self, interleaved: &[f32]) {
        let scale = 1.0 / self.channels as f64;
        for frame in interleaved.chunks_exact(self.channels) {
            let bin = self.frames * u64::from(BINS_PER_SECOND) / u64::from(self.sample_rate);
            if bin != self.bin {
                self.close_bin();
                self.bin = bin;
            }
            let mut sum = 0.0;
            for &sample in frame {
                self.peak = self.peak.max(sample.abs());
                sum += f64::from(sample);
            }
            for (squares, level) in self.band_squares.iter_mut().zip(self.bands.split(sum * scale))
            {
                *squares += level * level;
            }
            self.bin_frames += 1;
            self.frames += 1;
        }
    }

    pub fn finish(mut self) -> Waveform {
        if self.bin_frames > 0 {
            self.close_bin();
        }
        Waveform {
            bins: self.bins,
            frames: self.frames,
            sample_rate: self.sample_rate,
        }
    }

    fn close_bin(&mut self) {
        let frames = f64::from(self.bin_frames.max(1));
        let [low, mid, high] = self
            .band_squares
            .map(|squares| level((squares / frames).sqrt() as f32));
        self.bins.push(Bin {
            peak: level(self.peak),
            low,
            mid,
            high,
        });
        self.peak = 0.0;
        self.band_squares = [0.0; 3];
        self.bin_frames = 0;
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

    fn sine(frequency: f32, amplitude: f32, sample_rate: u32, seconds: f32) -> Vec<f32> {
        let frames = (sample_rate as f32 * seconds) as usize;
        (0..frames)
            .map(|i| (i as f32 * frequency * std::f32::consts::TAU / sample_rate as f32).sin())
            .map(|sample| sample * amplitude)
            .collect()
    }

    /// The bin halfway through, clear of the filters settling at the start.
    fn middle(waveform: &Waveform) -> Bin {
        waveform.bins[waveform.bins.len() / 2]
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
    fn a_full_scale_square_wave_has_full_peak() {
        let square: Vec<f32> = (0..48_000)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();

        let waveform = summarise(48_000, 1, &square);

        assert!(waveform.bins.iter().all(|bin| bin.peak == 255));
    }

    #[test]
    fn a_bass_tone_lands_in_the_low_band() {
        let waveform = summarise(44_100, 1, &sine(50.0, 0.5, 44_100, 2.0));

        // A 50 Hz cycle spans six bins, so one bin's level depends on where in the cycle it
        // falls. Over the cycles of the second second, the low band averages the sine's RMS.
        let settled = &waveform.bins[300..600];
        let mean = |band: fn(&Bin) -> u8| {
            settled.iter().map(|bin| f64::from(band(bin))).sum::<f64>() / settled.len() as f64
        };
        let low = mean(|bin| bin.low);
        assert!((80.0..=92.0).contains(&low), "{low}");
        assert!(mean(|bin| bin.mid) < 4.0 && mean(|bin| bin.high) < 1.0);
        assert!(settled.iter().all(|bin| bin.peak <= 128));
    }

    #[test]
    fn a_vocal_range_tone_lands_in_the_mid_band() {
        let waveform = summarise(48_000, 1, &sine(700.0, 0.5, 48_000, 2.0));

        let bin = middle(&waveform);
        assert!((85..=92).contains(&bin.mid), "{bin:?}");
        assert!(bin.low < 6 && bin.high < 10, "{bin:?}");
    }

    #[test]
    fn a_hat_range_tone_lands_in_the_high_band() {
        let waveform = summarise(44_100, 1, &sine(9_000.0, 0.5, 44_100, 2.0));

        let bin = middle(&waveform);
        assert!((85..=92).contains(&bin.high), "{bin:?}");
        assert!(bin.low < 2 && bin.mid < 6, "{bin:?}");
    }

    #[test]
    fn stereo_bands_read_the_mono_mix() {
        let left = sine(50.0, 0.5, 44_100, 1.0);
        let interleaved: Vec<f32> = left.iter().flat_map(|&sample| [sample, -sample]).collect();

        let waveform = summarise(44_100, 2, &interleaved);

        let bin = middle(&waveform);
        assert!(bin.peak > 0);
        assert_eq!((bin.low, bin.mid, bin.high), (0, 0, 0));
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

        assert_eq!(waveform.bins.len(), 1);
        assert_eq!(waveform.bins[0].peak, 64);
    }

    #[test]
    fn the_overview_keeps_the_loudest_bin_in_each_column() {
        let mut bins = vec![Bin::default(); 1_400];
        bins[15] = Bin {
            peak: 200,
            low: 9,
            mid: 0,
            high: 3,
        };
        bins[16].mid = 40;
        let waveform = Waveform {
            bins,
            frames: 1_400 * 160,
            sample_rate: 48_000,
        };

        let overview = waveform.overview();

        assert_eq!(
            overview.columns[1],
            Bin {
                peak: 200,
                low: 9,
                mid: 40,
                high: 3
            }
        );
        assert!(overview.columns.iter().enumerate().all(|(index, column)| {
            index == 1 || *column == Bin::default()
        }));
    }

    #[test]
    fn a_track_shorter_than_the_overview_repeats_its_bins() {
        let bins: Vec<Bin> = (1..=7)
            .map(|peak| Bin {
                peak,
                ..Bin::default()
            })
            .collect();
        let waveform = Waveform {
            bins,
            frames: 7 * 160,
            sample_rate: 48_000,
        };

        let overview = waveform.overview();

        assert_eq!(overview.columns[0].peak, 1);
        assert_eq!(overview.columns[OVERVIEW_COLUMNS - 1].peak, 7);
        assert!(overview.columns.iter().all(|column| column.peak > 0));
    }

    #[test]
    fn an_overview_round_trips_through_bytes() {
        let waveform = summarise(44_100, 1, &sine(440.0, 0.8, 44_100, 3.0));
        let overview = waveform.overview();

        let bytes = overview.to_bytes();

        assert_eq!(bytes.len(), Overview::BYTES);
        assert_eq!(Overview::from_bytes(&bytes), Some(overview));
        assert_eq!(Overview::from_bytes(&bytes[1..]), None);
    }
}
