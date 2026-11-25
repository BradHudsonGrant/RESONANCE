//! What counts as a damaged, clipped, broken-up or fake-lossless file, and how sure the app is.
//!
//! The decode pass measures; these rules judge. A `Confirmed` flag is a fact about the file, such
//! as frames that failed to decode. A `Likely` flag is a strong sign that still needs an ear.

use std::fmt;

use crate::identity::DecodedAudioFacts;
use crate::library::AudioFormat;

/// A header length further than this from the decoded length means the stream was cut short.
pub const HEADER_LENGTH_TOLERANCE_MS: u64 = 500;
/// A sample at or above this magnitude sits at digital full scale. The top 16-bit value,
/// 32,767 over 32,768, is just above it.
pub const FULL_SCALE: f32 = 0.999_96;
/// Consecutive full-scale samples that make one clipped run.
pub const MIN_CLIPPED_RUN: u32 = 4;
/// Clipped runs that make clipping a fact rather than a stray peak.
pub const CONFIRMED_CLIPPED_RUNS: u32 = 10;
/// True peak above this on a lossy file means the encode clips between samples.
pub const LOSSY_TRUE_PEAK_LIMIT_DBTP: f64 = 0.5;
/// Below this level a stretch of audio counts as a gap.
pub const DROPOUT_FLOOR_DBFS: f32 = -75.0;
/// The music either side of a gap must be at least this loud.
pub const DROPOUT_WALL_DBFS: f32 = -24.0;
/// How far either side of a gap the louder music is looked for.
pub const DROPOUT_WALL_MS: u32 = 50;
/// The jump between two samples at the edge of a gap that shows a cut rather than a fade.
pub const DROPOUT_STEP: f32 = 0.25;
pub const DROPOUT_MIN_MS: u32 = 10;
pub const DROPOUT_MAX_MS: u32 = 500;
/// Lossless files below this rate are not checked for a lossy past.
pub const TRANSCODE_MIN_SAMPLE_RATE: u32 = 44_100;
/// The spectrum is judged in bands this wide, from `SPECTRUM_LOW_HZ` up to `SPECTRUM_HIGH_HZ`.
pub const SPECTRUM_BAND_HZ: f64 = 200.0;
pub const SPECTRUM_LOW_HZ: f64 = 8_000.0;
pub const SPECTRUM_HIGH_HZ: f64 = 21_500.0;
/// A lossy encoder's low-pass drops at least this far within `CLIFF_WIDTH_HZ`. A record's
/// natural roll-off of 6 to 12 dB an octave falls well short of it.
pub const CLIFF_DROP_DB: f32 = 28.0;
pub const CLIFF_WIDTH_HZ: f64 = 600.0;
/// Above the cliff, every band stays below this level.
pub const CLIFF_FLOOR_DBFS: f32 = -85.0;
/// Cliffs above this are where converters filter every lossless recording, so they prove
/// nothing.
pub const CLIFF_MAX_HZ: f64 = 20_600.0;
/// At least this much quiet spectrum above the cliff, so one dark band is not mistaken for one.
pub const CLIFF_MIN_FLOOR_HZ: f64 = 400.0;

/// How sure a flag is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FlagCertainty {
    Likely,
    Confirmed,
}

impl FlagCertainty {
    pub fn name(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Likely => "likely",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "confirmed" => Some(Self::Confirmed),
            "likely" => Some(Self::Likely),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Confirmed => "Confirmed",
            Self::Likely => "Likely",
        }
    }
}

/// What a flag says is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QualityKind {
    Damaged,
    Clipping,
    Dropout,
    Transcode,
}

impl QualityKind {
    pub const ALL: [Self; 4] = [
        Self::Damaged,
        Self::Clipping,
        Self::Dropout,
        Self::Transcode,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Damaged => "damaged",
            Self::Clipping => "clipping",
            Self::Dropout => "dropout",
            Self::Transcode => "transcode",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Damaged => "Damaged",
            Self::Clipping => "Clipping",
            Self::Dropout => "Dropouts",
            Self::Transcode => "Fake lossless",
        }
    }
}

/// One problem found in a file, with the reason in plain words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityFlag {
    pub kind: QualityKind,
    pub certainty: FlagCertainty,
    pub reason: String,
}

/// What one full decode of a file measured.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct QualityMeasurements {
    pub integrated_lufs: Option<f64>,
    pub loudness_range_lu: Option<f64>,
    pub sample_peak_dbfs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    /// Peak over RMS below 150 Hz, over the passages that carry the track.
    pub crest_low_db: Option<f64>,
    /// Peak over RMS from 400 Hz to 4 kHz, over the same passages.
    pub crest_mid_db: Option<f64>,
    pub clipped_runs: u32,
    pub dropouts: u32,
    /// Where the first dropout starts, for the reason.
    pub first_dropout_ms: Option<u64>,
    /// Where a lossy encoder's low-pass cut the spectrum, if a cliff was found.
    pub spectral_cutoff_hz: Option<f64>,
}

/// Everything the verdict needs about one file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QualityInput {
    pub format: AudioFormat,
    /// `None` when no audio could be decoded at all.
    pub facts: Option<DecodedAudioFacts>,
    pub measurements: QualityMeasurements,
}

/// The flags a file earns, most serious first.
pub fn assess(input: &QualityInput) -> Vec<QualityFlag> {
    let mut flags = Vec::new();
    let Some(facts) = input.facts else {
        flags.push(QualityFlag {
            kind: QualityKind::Damaged,
            certainty: FlagCertainty::Confirmed,
            reason: "No audio in the file could be decoded".to_owned(),
        });
        return flags;
    };
    let confirmed = |kind, reason: String| QualityFlag {
        kind,
        certainty: FlagCertainty::Confirmed,
        reason,
    };
    let likely = |kind, reason: String| QualityFlag {
        kind,
        certainty: FlagCertainty::Likely,
        reason,
    };

    if facts.decode_errors > 0 {
        flags.push(confirmed(
            QualityKind::Damaged,
            format!(
                "{} {} of audio failed to decode",
                facts.decode_errors,
                plural(facts.decode_errors, "packet", "packets")
            ),
        ));
    }
    if facts.checksum_ok == Some(false) {
        flags.push(confirmed(
            QualityKind::Damaged,
            "The audio does not match the checksum stored in the file".to_owned(),
        ));
    }
    if header_length_is_exact(input.format)
        && let Some(header_ms) = facts.header_duration_ms()
    {
        let decoded_ms = facts.duration_ms();
        if header_ms.abs_diff(decoded_ms) > HEADER_LENGTH_TOLERANCE_MS {
            flags.push(confirmed(
                QualityKind::Damaged,
                format!(
                    "The file says it lasts {} but only {} decodes",
                    clock(header_ms),
                    clock(decoded_ms)
                ),
            ));
        }
    }

    let measured = input.measurements;
    if measured.clipped_runs >= CONFIRMED_CLIPPED_RUNS {
        flags.push(confirmed(
            QualityKind::Clipping,
            format!(
                "The waveform is flat at full scale in {} places",
                measured.clipped_runs
            ),
        ));
    } else if !facts.lossless
        && let Some(peak) = measured.true_peak_dbtp
        && peak > LOSSY_TRUE_PEAK_LIMIT_DBTP
    {
        flags.push(likely(
            QualityKind::Clipping,
            format!("True peak reaches {peak:+.1} dBTP, so playback clips between samples"),
        ));
    }

    if measured.dropouts > 0 {
        let at = measured
            .first_dropout_ms
            .map(|ms| format!(", the first at {}", clock(ms)))
            .unwrap_or_default();
        flags.push(likely(
            QualityKind::Dropout,
            format!(
                "The sound cuts out abruptly {} {}{at}",
                measured.dropouts,
                plural(measured.dropouts, "time", "times")
            ),
        ));
    }

    if checks_for_transcode(input.format, facts.lossless, facts.sample_rate)
        && let Some(cutoff) = measured.spectral_cutoff_hz
    {
        flags.push(likely(QualityKind::Transcode, transcode_reason(cutoff)));
    }

    flags.sort_by(|a, b| b.certainty.cmp(&a.certainty).then(a.kind.cmp(&b.kind)));
    flags
}

/// The most serious flag, which is the one the table shows.
pub fn worst(flags: &[QualityFlag]) -> Option<&QualityFlag> {
    flags
        .iter()
        .min_by(|a, b| b.certainty.cmp(&a.certainty).then(a.kind.cmp(&b.kind)))
}

/// MP3 headers often guess their length from the bitrate, so only containers that state it
/// exactly are held to it.
fn header_length_is_exact(format: AudioFormat) -> bool {
    !matches!(format, AudioFormat::Mp3)
}

/// Whether a file is lossless and fast enough to show a lossy encoder's low-pass.
pub fn checks_for_transcode(format: AudioFormat, lossless: bool, sample_rate: u32) -> bool {
    let lossless_container = matches!(
        format,
        AudioFormat::Wav | AudioFormat::Flac | AudioFormat::Aiff | AudioFormat::M4a
    );
    lossless_container && lossless && sample_rate >= TRANSCODE_MIN_SAMPLE_RATE
}

fn transcode_reason(cutoff_hz: f64) -> String {
    let khz = cutoff_hz / 1000.0;
    let source = match khz {
        k if (15.5..=16.5).contains(&k) => "a 128 kbps MP3",
        k if (18.5..=19.8).contains(&k) => "a 192 kbps MP3 or AAC",
        k if (19.8..=20.6).contains(&k) => "a 320 kbps MP3",
        _ => "a lossy file",
    };
    format!(
        "The sound stops sharply above {khz:.1} kHz, which is typical of {source} saved as lossless"
    )
}

/// Finds a lossy encoder's low-pass in a spectrum given as levels in dBFS for consecutive
/// `SPECTRUM_BAND_HZ` bands starting at `SPECTRUM_LOW_HZ`. Returns the cut frequency.
pub fn spectral_cliff(bands_db: &[f32]) -> Option<f64> {
    let steps = (CLIFF_WIDTH_HZ / SPECTRUM_BAND_HZ).round() as usize;
    let min_floor = (CLIFF_MIN_FLOOR_HZ / SPECTRUM_BAND_HZ).round() as usize;
    let floor_start = bands_db
        .iter()
        .rposition(|&level| level >= CLIFF_FLOOR_DBFS)
        .map_or(0, |last_loud| last_loud + 1);
    if floor_start == 0 || bands_db.len() - floor_start < min_floor {
        return None;
    }
    let before = bands_db[floor_start.saturating_sub(steps)..floor_start]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let after = bands_db[floor_start..(floor_start + 2).min(bands_db.len())]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    if before - after < CLIFF_DROP_DB {
        return None;
    }
    let cutoff = SPECTRUM_LOW_HZ + floor_start as f64 * SPECTRUM_BAND_HZ;
    (cutoff <= CLIFF_MAX_HZ).then_some(cutoff)
}

/// Counts runs of `MIN_CLIPPED_RUN` or more full-scale samples on each channel.
#[derive(Debug, Clone, Default)]
pub struct ClipRuns {
    run: Vec<u32>,
    runs: u32,
}

impl ClipRuns {
    pub fn new(channels: usize) -> Self {
        Self {
            run: vec![0; channels],
            runs: 0,
        }
    }

    pub fn push_frame(&mut self, frame: &[f32]) {
        for (run, &sample) in self.run.iter_mut().zip(frame) {
            if sample.abs() >= FULL_SCALE {
                *run += 1;
                if *run == MIN_CLIPPED_RUN {
                    self.runs = self.runs.saturating_add(1);
                }
            } else {
                *run = 0;
            }
        }
    }

    pub fn runs(&self) -> u32 {
        self.runs
    }
}

/// Finds short gaps of near silence cut into loud music, frame by frame.
#[derive(Debug, Clone)]
pub struct DropoutFinder {
    floor: f32,
    wall: f32,
    wall_frames: u64,
    min_frames: u64,
    max_frames: u64,
    sample_rate: u32,
    frame: u64,
    previous: Vec<f32>,
    last_loud: Option<u64>,
    gap: Option<Gap>,
    dropouts: u32,
    first_at: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct Gap {
    start: u64,
    loud_before: bool,
    cut_in: bool,
    /// Set once the gap has ended: its end frame and whether the sound jumped back in.
    ended: Option<(u64, bool)>,
}

impl DropoutFinder {
    pub fn new(sample_rate: u32, channels: usize) -> Self {
        let frames = |ms: u32| u64::from(sample_rate) * u64::from(ms) / 1000;
        Self {
            floor: db_to_amplitude(DROPOUT_FLOOR_DBFS),
            wall: db_to_amplitude(DROPOUT_WALL_DBFS),
            wall_frames: frames(DROPOUT_WALL_MS),
            min_frames: frames(DROPOUT_MIN_MS),
            max_frames: frames(DROPOUT_MAX_MS),
            sample_rate,
            frame: 0,
            previous: vec![0.0; channels],
            last_loud: None,
            gap: None,
            dropouts: 0,
            first_at: None,
        }
    }

    pub fn push_frame(&mut self, frame: &[f32]) {
        let mut peak = 0.0f32;
        let mut step = 0.0f32;
        for (previous, &sample) in self.previous.iter_mut().zip(frame) {
            peak = peak.max(sample.abs());
            step = step.max((sample - *previous).abs());
            *previous = sample;
        }
        let now = self.frame;
        self.frame += 1;
        let quiet = peak < self.floor;
        let loud = peak > self.wall;

        if let Some(mut gap) = self.gap {
            match gap.ended {
                None if quiet => {
                    if now - gap.start > self.max_frames {
                        self.gap = None;
                    }
                }
                None => {
                    gap.ended = Some((now, step > DROPOUT_STEP));
                    self.gap = Some(gap);
                    self.close_gap_if_loud(now, loud);
                }
                Some(_) => self.close_gap_if_loud(now, loud),
            }
        } else if quiet {
            let loud_before = self
                .last_loud
                .is_some_and(|at| now - at <= self.wall_frames);
            self.gap = Some(Gap {
                start: now,
                loud_before,
                cut_in: step > DROPOUT_STEP,
                ended: None,
            });
        }
        if loud {
            self.last_loud = Some(now);
        }
    }

    fn close_gap_if_loud(&mut self, now: u64, loud: bool) {
        let Some(gap) = self.gap else { return };
        let Some((end, cut_out)) = gap.ended else {
            return;
        };
        if loud {
            let length = end - gap.start;
            if gap.loud_before
                && (gap.cut_in || cut_out)
                && (self.min_frames..=self.max_frames).contains(&length)
            {
                self.dropouts = self.dropouts.saturating_add(1);
                if self.first_at.is_none() && self.sample_rate > 0 {
                    self.first_at = Some(gap.start * 1000 / u64::from(self.sample_rate));
                }
            }
            self.gap = None;
        } else if now - end > self.wall_frames {
            self.gap = None;
        }
    }

    pub fn dropouts(&self) -> u32 {
        self.dropouts
    }

    pub fn first_dropout_ms(&self) -> Option<u64> {
        self.first_at
    }
}

pub fn db_to_amplitude(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn plural(count: u32, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl fmt::Display for QualityFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.certainty.label(),
            self.kind.label().to_lowercase(),
            self.reason
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(format_lossless: bool) -> DecodedAudioFacts {
        DecodedAudioFacts {
            sample_rate: 44_100,
            channels: 2,
            frames: 44_100 * 60,
            decode_errors: 0,
            checksum_ok: None,
            header_frames: Some(44_100 * 60),
            lossless: format_lossless,
        }
    }

    fn input(format: AudioFormat, facts: DecodedAudioFacts) -> QualityInput {
        QualityInput {
            format,
            facts: Some(facts),
            measurements: QualityMeasurements::default(),
        }
    }

    #[test]
    fn a_clean_file_has_no_flags() {
        assert!(assess(&input(AudioFormat::Flac, facts(true))).is_empty());
    }

    #[test]
    fn a_file_with_no_audio_is_confirmed_damaged() {
        let flags = assess(&QualityInput {
            format: AudioFormat::Wav,
            facts: None,
            measurements: QualityMeasurements::default(),
        });
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].kind, QualityKind::Damaged);
        assert_eq!(flags[0].certainty, FlagCertainty::Confirmed);
    }

    #[test]
    fn a_short_stream_is_confirmed_damaged_except_for_mp3() {
        let mut short = facts(true);
        short.frames = 44_100 * 30;
        let flags = assess(&input(AudioFormat::Wav, short));
        assert_eq!(flags[0].kind, QualityKind::Damaged);
        assert_eq!(
            flags[0].reason,
            "The file says it lasts 1:00 but only 0:30 decodes"
        );

        let mut mp3 = short;
        mp3.lossless = false;
        assert!(assess(&input(AudioFormat::Mp3, mp3)).is_empty());
    }

    #[test]
    fn a_header_within_half_a_second_passes() {
        let mut close = facts(true);
        close.frames -= 44_100 / 4;
        assert!(assess(&input(AudioFormat::Flac, close)).is_empty());
    }

    #[test]
    fn decode_errors_and_bad_checksums_are_confirmed() {
        let mut broken = facts(true);
        broken.decode_errors = 3;
        broken.checksum_ok = Some(false);
        let flags = assess(&input(AudioFormat::Flac, broken));
        assert_eq!(flags.len(), 2);
        assert!(
            flags
                .iter()
                .all(|flag| flag.certainty == FlagCertainty::Confirmed)
        );
        assert_eq!(flags[0].reason, "3 packets of audio failed to decode");
    }

    #[test]
    fn many_clipped_runs_are_confirmed_and_a_hot_lossy_peak_is_likely() {
        let mut clipped = input(AudioFormat::Wav, facts(true));
        clipped.measurements.clipped_runs = CONFIRMED_CLIPPED_RUNS;
        let flags = assess(&clipped);
        assert_eq!(flags[0].kind, QualityKind::Clipping);
        assert_eq!(flags[0].certainty, FlagCertainty::Confirmed);

        let mut hot = input(AudioFormat::Mp3, facts(false));
        hot.measurements.true_peak_dbtp = Some(1.2);
        let flags = assess(&hot);
        assert_eq!(flags[0].certainty, FlagCertainty::Likely);
        assert_eq!(
            flags[0].reason,
            "True peak reaches +1.2 dBTP, so playback clips between samples"
        );

        let mut hot_lossless = input(AudioFormat::Wav, facts(true));
        hot_lossless.measurements.true_peak_dbtp = Some(1.2);
        assert!(assess(&hot_lossless).is_empty());
    }

    #[test]
    fn transcodes_are_judged_only_on_lossless_files() {
        let mut fake = input(AudioFormat::Flac, facts(true));
        fake.measurements.spectral_cutoff_hz = Some(16_000.0);
        let flags = assess(&fake);
        assert_eq!(flags[0].kind, QualityKind::Transcode);
        assert_eq!(
            flags[0].reason,
            "The sound stops sharply above 16.0 kHz, which is typical of a 128 kbps MP3 saved as lossless"
        );

        let mut aac = input(AudioFormat::M4a, facts(false));
        aac.measurements.spectral_cutoff_hz = Some(16_000.0);
        assert!(assess(&aac).is_empty());
    }

    #[test]
    fn worst_prefers_confirmed_flags() {
        let mut both = input(AudioFormat::Flac, facts(true));
        both.measurements.spectral_cutoff_hz = Some(19_000.0);
        both.measurements.clipped_runs = 40;
        let flags = assess(&both);
        assert_eq!(
            worst(&flags).map(|flag| flag.kind),
            Some(QualityKind::Clipping)
        );
    }

    fn bands(level_at: impl Fn(f64) -> f32) -> Vec<f32> {
        let count = ((SPECTRUM_HIGH_HZ - SPECTRUM_LOW_HZ) / SPECTRUM_BAND_HZ) as usize;
        (0..count)
            .map(|band| level_at(SPECTRUM_LOW_HZ + (band as f64 + 0.5) * SPECTRUM_BAND_HZ))
            .collect()
    }

    #[test]
    fn a_brickwall_low_pass_is_found() {
        let cut = bands(|hz| if hz < 16_000.0 { -60.0 } else { -130.0 });
        assert_eq!(spectral_cliff(&cut), Some(16_000.0));
    }

    #[test]
    fn a_gentle_roll_off_is_not_a_cliff() {
        // 12 dB an octave from 2 kHz, sinking under the floor near the top of the range.
        let gentle = bands(|hz| (-40.0 - 12.0 * (hz / 2_000.0).log2()) as f32);
        assert_eq!(spectral_cliff(&gentle), None);
    }

    #[test]
    fn a_cliff_where_every_converter_filters_proves_nothing() {
        let converter = bands(|hz| if hz < 21_000.0 { -60.0 } else { -130.0 });
        assert_eq!(spectral_cliff(&converter), None);
    }

    #[test]
    fn full_scale_runs_are_counted_once_each() {
        let mut runs = ClipRuns::new(1);
        for _ in 0..3 {
            for _ in 0..6 {
                runs.push_frame(&[1.0]);
            }
            runs.push_frame(&[0.2]);
        }
        for _ in 0..3 {
            runs.push_frame(&[-1.0]);
        }
        assert_eq!(runs.runs(), 3);
    }

    fn tone(finder: &mut DropoutFinder, frames: usize, offset: usize) {
        for n in 0..frames {
            let phase = (offset + n) as f32 * 1_000.0 / 44_100.0 * std::f32::consts::TAU;
            finder.push_frame(&[0.5 * phase.sin()]);
        }
    }

    #[test]
    fn a_cut_into_loud_music_is_a_dropout() {
        let mut finder = DropoutFinder::new(44_100, 1);
        // 1 kHz at 44.1 kHz peaks near sample 11, so the cut lands on a steep edge.
        tone(&mut finder, 44_100 + 11, 0);
        for _ in 0..(44_100 / 10) {
            finder.push_frame(&[0.0]);
        }
        tone(&mut finder, 44_100, 0);
        assert_eq!(finder.dropouts(), 1);
        assert_eq!(finder.first_dropout_ms(), Some(1_000));
    }

    #[test]
    fn a_long_silence_or_a_fade_is_not_a_dropout() {
        let mut finder = DropoutFinder::new(44_100, 1);
        tone(&mut finder, 44_100 + 11, 0);
        for _ in 0..44_100 {
            finder.push_frame(&[0.0]);
        }
        tone(&mut finder, 44_100, 0);

        for n in 0..44_100 {
            let gain = 1.0 - n as f32 / 44_100.0;
            let phase = n as f32 * 1_000.0 / 44_100.0 * std::f32::consts::TAU;
            finder.push_frame(&[0.5 * gain * phase.sin()]);
        }
        for _ in 0..(44_100 / 10) {
            finder.push_frame(&[0.0]);
        }
        assert_eq!(finder.dropouts(), 0);
    }
}
