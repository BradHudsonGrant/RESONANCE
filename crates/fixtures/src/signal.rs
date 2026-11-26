use std::f64::consts::TAU;

/// Interleaved samples between -1.0 and 1.0.
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

fn amplitude(dbfs: f64) -> f64 {
    10f64.powf(dbfs / 20.0)
}

fn frames_in(seconds: f64, sample_rate: u32) -> usize {
    (seconds * f64::from(sample_rate)).round() as usize
}

impl Signal {
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// A mono sine wave whose peak sits at `peak_dbfs`.
    pub fn tone(frequency_hz: f64, seconds: f64, peak_dbfs: f64, sample_rate: u32) -> Self {
        let peak = amplitude(peak_dbfs);
        let step = TAU * frequency_hz / f64::from(sample_rate);
        let samples = (0..frames_in(seconds, sample_rate))
            .map(|frame| (peak * (step * frame as f64).sin()) as f32)
            .collect();
        Self {
            sample_rate,
            channels: 1,
            samples,
        }
    }

    /// A stereo 997 Hz sine that a BS.1770 meter reads as `lufs`.
    ///
    /// The standard calibrates its weighting on this tone: at full scale in one channel it reads
    /// -3.01 LUFS, so in both channels it reads 0, and its peak in dBFS is its loudness.
    pub fn tone_at_loudness(lufs: f64, seconds: f64, sample_rate: u32) -> Self {
        let mono = Self::tone(997.0, seconds, lufs, sample_rate);
        Self {
            sample_rate,
            channels: 2,
            samples: mono
                .samples
                .iter()
                .flat_map(|&sample| [sample, sample])
                .collect(),
        }
    }

    /// The same audio starting `frames` later, with silence in front.
    pub fn delayed_by(&self, frames: usize) -> Self {
        let mut samples = vec![0.0; frames * usize::from(self.channels)];
        samples.extend_from_slice(&self.samples);
        Self {
            sample_rate: self.sample_rate,
            channels: self.channels,
            samples,
        }
    }
}

/// Mono clicks at a steady tempo, with the frame each one starts on.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickTrack {
    pub signal: Signal,
    pub beat_frames: Vec<usize>,
}

const CLICK_SECONDS: f64 = 0.010;
const CLICK_DECAY_SECONDS: f64 = 0.002;
const CLICK_HZ: f64 = 1000.0;
const CLICK_PEAK: f64 = 0.5;

impl ClickTrack {
    /// The first click starts on frame 0. Each later one starts on the frame nearest its exact
    /// time, so a long track does not drift.
    pub fn at_tempo(tempo_bpm: f64, beats: usize, sample_rate: u32) -> Self {
        let frames_per_beat = 60.0 / tempo_bpm * f64::from(sample_rate);
        let beat_frames: Vec<usize> = (0..beats)
            .map(|beat| (beat as f64 * frames_per_beat).round() as usize)
            .collect();
        let mut samples = vec![0.0f32; (beats as f64 * frames_per_beat).round() as usize];
        let click_frames = frames_in(CLICK_SECONDS, sample_rate);
        for &start in &beat_frames {
            for (offset, sample) in samples[start..].iter_mut().take(click_frames).enumerate() {
                let seconds = offset as f64 / f64::from(sample_rate);
                let envelope = (-seconds / CLICK_DECAY_SECONDS).exp();
                *sample = (CLICK_PEAK * envelope * (TAU * CLICK_HZ * seconds).cos()) as f32;
            }
        }
        Self {
            signal: Signal {
                sample_rate,
                channels: 1,
                samples,
            },
            beat_frames,
        }
    }
}

/// A short loop of kick, snare, hats and chords that never repeats exactly, written as a
/// function of time so the same music can be rendered at any fractional shift or speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Groove {
    pub tempo_bpm: f64,
    /// Picks the hat accents, snare fills and chords, so two seeds give unrelated music.
    pub seed: u64,
}

/// A number from 0 to 1 that depends only on `seed` and `index`.
fn scatter(seed: u64, index: i64) -> f64 {
    let mut x = seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// A 1 ms raised-cosine attack, so a hit has no step in it.
fn attack(t: f64) -> f64 {
    const RISE_S: f64 = 0.001;
    if t >= RISE_S {
        1.0
    } else {
        0.5 - 0.5 * (std::f64::consts::PI * t / RISE_S).cos()
    }
}

impl Groove {
    /// The groove `t` seconds in. Silent before 0.
    pub fn value(&self, t: f64) -> f64 {
        if t < 0.0 {
            return 0.0;
        }
        let beat_s = 60.0 / self.tempo_bpm;
        let beat = (t / beat_s).floor() as i64;
        let mut sum = 0.0;
        for hit in (beat - 2)..=beat {
            if hit < 0 {
                continue;
            }
            let since = t - hit as f64 * beat_s;
            let kick_hz = 48.0 + 110.0 * (-since / 0.03).exp();
            sum += 0.5 * attack(since) * (-since / 0.25).exp() * (TAU * kick_hz * since).sin();
            if hit % 2 == 1 || scatter(self.seed, hit) > 0.8 {
                let snare = 0.3 * attack(since) * (-since / 0.08).exp();
                sum += snare
                    * ((TAU * 185.0 * since).sin()
                        + 0.6 * (TAU * 4_400.0 * since).sin()
                        + 0.5 * (TAU * 5_730.0 * since).sin());
            }
            let off = since - beat_s / 2.0;
            if off >= 0.0 {
                let accent = 0.05 + 0.15 * scatter(self.seed.wrapping_add(1), hit);
                sum += accent
                    * attack(off)
                    * (-off / 0.02).exp()
                    * ((TAU * 7_100.0 * off).sin()
                        + (TAU * 9_350.0 * off).sin()
                        + (TAU * 11_830.0 * off).sin());
            }
        }
        let bar_s = 4.0 * beat_s;
        let bar = (t / bar_s).floor() as i64;
        let in_bar = t - bar as f64 * bar_s;
        let root =
            110.0 * 2f64.powf((scatter(self.seed.wrapping_add(2), bar) * 12.0).floor() / 12.0);
        let swell = (std::f64::consts::PI * in_bar / bar_s).sin();
        sum += 0.08
            * swell
            * ((TAU * root * in_bar).sin()
                + (TAU * root * 1.5 * in_bar).sin()
                + (TAU * root * 2.52 * in_bar).sin());
        sum
    }

    /// `seconds` of the groove as a mono signal, with the groove's start `shift_s` seconds in
    /// and played `speed` times as fast: frame `n` holds `value((n / rate - shift_s) * speed)`.
    pub fn render(&self, seconds: f64, sample_rate: u32, shift_s: f64, speed: f64) -> Signal {
        let rate = f64::from(sample_rate);
        let samples = (0..frames_in(seconds, sample_rate))
            .map(|frame| (0.45 * self.value((frame as f64 / rate - shift_s) * speed)) as f32)
            .collect();
        Signal {
            sample_rate,
            channels: 1,
            samples,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(samples: &[f32]) -> f32 {
        samples
            .iter()
            .fold(0.0, |peak, sample| sample.abs().max(peak))
    }

    #[test]
    fn a_tone_peaks_at_the_level_asked_for() {
        let tone = Signal::tone(440.0, 1.0, -6.0, 48_000);

        assert_eq!(tone.frames(), 48_000);
        assert!((f64::from(peak(&tone.samples)) - amplitude(-6.0)).abs() < 1e-4);
    }

    #[test]
    fn a_tone_has_the_frequency_asked_for() {
        let tone = Signal::tone(440.0, 1.0, -6.0, 48_000);

        let rising_crossings = tone
            .samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();

        assert_eq!(rising_crossings, 439);
    }

    #[test]
    fn a_tone_at_a_loudness_is_stereo_with_its_peak_at_that_level() {
        let tone = Signal::tone_at_loudness(-14.0, 2.0, 48_000);

        assert_eq!(tone.channels, 2);
        assert_eq!(tone.frames(), 96_000);
        assert!((f64::from(peak(&tone.samples)) - amplitude(-14.0)).abs() < 1e-4);
        assert!(tone.samples.chunks(2).all(|frame| frame[0] == frame[1]));
    }

    #[test]
    fn a_delayed_signal_is_the_original_behind_that_many_silent_frames() {
        let tone = Signal::tone_at_loudness(-14.0, 0.1, 44_100);

        let delayed = tone.delayed_by(441);

        assert_eq!(delayed.frames(), tone.frames() + 441);
        assert!(delayed.samples[..882].iter().all(|&sample| sample == 0.0));
        assert_eq!(delayed.samples[882..], tone.samples[..]);
    }

    #[test]
    fn clicks_start_on_the_frames_the_tempo_gives() {
        let clicks = ClickTrack::at_tempo(120.0, 8, 44_100);

        assert_eq!(
            clicks.beat_frames,
            [0, 22_050, 44_100, 66_150, 88_200, 110_250, 132_300, 154_350]
        );
        assert_eq!(clicks.signal.frames(), 176_400);
    }

    #[test]
    fn a_tempo_that_does_not_divide_the_sample_rate_does_not_drift() {
        let clicks = ClickTrack::at_tempo(174.0, 1_000, 44_100);

        let last = clicks.beat_frames[999];
        let exact = 999.0 * 60.0 / 174.0 * 44_100.0;

        assert!((last as f64 - exact).abs() <= 0.5);
    }

    #[test]
    fn each_click_is_loudest_on_its_first_frame_and_silent_before_the_next() {
        let clicks = ClickTrack::at_tempo(120.0, 4, 44_100);
        let samples = &clicks.signal.samples;

        for &start in &clicks.beat_frames {
            assert_eq!(samples[start], CLICK_PEAK as f32);
            assert!(
                samples[start + 441..start + 22_050]
                    .iter()
                    .all(|&s| s == 0.0)
            );
        }
    }
}
