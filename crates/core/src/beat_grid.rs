//! The rules that turn measured beats into a grid: which beat of the bar each one is, whether the
//! tempo is steady enough for a constant grid, and how far to trust the result.

use crate::analysis::{AnalysisProvenance, BeatGrid};

pub const BEATS_PER_BAR: usize = 4;
/// Grids less sure than this go to Prep for a check.
pub const GRID_CHECK_BELOW: f32 = 0.75;
/// A spread of bar tempos this small is a steady digital tempo, and gets a constant grid.
pub const STEADY_IQR_BPM: f64 = 0.08;
/// A smoothed bar tempo that wanders further than this is a drifting grid, as on a vinyl rip or
/// a live drummer, and keeps every measured beat.
pub const VARIABLE_DRIFT_BPM: f64 = 0.15;
/// The weights of the four kinds of evidence for bar phase.
const DOWNBEAT_WEIGHT: f64 = 0.40;
const DROP_WEIGHT: f64 = 0.30;
const NOVELTY_WEIGHT: f64 = 0.15;
const BACKBEAT_WEIGHT: f64 = 0.15;
/// A bar's low end at least this many times the bar before it is a drop: 6 dB.
const DROP_JUMP: f64 = 4.0;
/// Beats each side of the novelty measure: 8 bars, the shortest phrase in club music.
const NOVELTY_SPAN: usize = 8 * BEATS_PER_BAR;
/// Bars each side of the median that smooths bar tempos before measuring drift.
const DRIFT_SMOOTHING: usize = 2;

/// What one beat sounds like, for deciding which beat of the bar it is.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BeatEvidence {
    /// The beat tracker's downbeat logit at this beat.
    pub downbeat_logit: f32,
    /// Whether the beat sits in a bar with full drums, where the downbeat logits are reliable.
    pub full_drums: bool,
    /// Percussive energy under 150 Hz at the beat: the kick.
    pub low: f32,
    /// Percussive energy from 150 Hz to 2.5 kHz at the beat: snares and claps.
    pub mid: f32,
    /// Harmonic energy over the beat, for spotting where sections change.
    pub harmonic: f32,
}

/// The chosen bar phase: beat `i` is a downbeat when `i % 4 == phase`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarPhase {
    pub phase: usize,
    pub scores: [f64; BEATS_PER_BAR],
    /// The winning score less the runner-up, from 0 to 1.
    pub margin: f64,
}

impl BarPhase {
    /// Bar 1, beat 1: the first beat on the phase. Beats before it are the pickup, bar 0.
    pub fn first_downbeat(&self, beats: usize) -> usize {
        self.phase.min(beats.saturating_sub(1))
    }
}

/// Scores all four bar phases over the whole track at once, so a vocal or snare pickup ahead
/// of bar 1 cannot pull beat 1 onto beat 3 for the rest of the song. Each kind of evidence is
/// scaled to 0..1 across the phases before weighting, so none dominates by its units alone.
pub fn choose_bar_phase(beats: &[BeatEvidence]) -> BarPhase {
    let downbeat = normalised(downbeat_scores(beats));
    let drop = drop_scores(beats);
    let novelty = normalised(novelty_scores(beats));
    let backbeat = normalised(backbeat_scores(beats));
    let mut scores = [0.0; BEATS_PER_BAR];
    for (phase, score) in scores.iter_mut().enumerate() {
        *score = DOWNBEAT_WEIGHT * downbeat[phase]
            + DROP_WEIGHT * drop[phase]
            + NOVELTY_WEIGHT * novelty[phase]
            + BACKBEAT_WEIGHT * backbeat[phase];
    }
    let mut order = [0, 1, 2, 3];
    order.sort_by(|a, b| scores[*b].total_cmp(&scores[*a]));
    BarPhase {
        phase: order[0],
        scores,
        margin: (scores[order[0]] - scores[order[1]]).clamp(0.0, 1.0),
    }
}

fn downbeat_scores(beats: &[BeatEvidence]) -> [f64; BEATS_PER_BAR] {
    let drums = beats.iter().any(|beat| beat.full_drums);
    let mut sums = [0.0; BEATS_PER_BAR];
    let mut counts = [0usize; BEATS_PER_BAR];
    for (index, beat) in beats.iter().enumerate() {
        if beat.full_drums || !drums {
            sums[index % BEATS_PER_BAR] += f64::from(beat.downbeat_logit);
            counts[index % BEATS_PER_BAR] += 1;
        }
    }
    let mut means = [0.0; BEATS_PER_BAR];
    for phase in 0..BEATS_PER_BAR {
        if counts[phase] > 0 {
            means[phase] = sums[phase] / counts[phase] as f64;
        }
    }
    means
}

/// The first beat where the low end jumps to a full drop: a whole phase's worth of evidence,
/// since the first drop of a club track lands on bar 1 of a phrase.
fn drop_scores(beats: &[BeatEvidence]) -> [f64; BEATS_PER_BAR] {
    let mut scores = [0.0; BEATS_PER_BAR];
    // The mean low end over the 4 beats from `start`, with beats past either end as silence.
    let bar_low = |start: usize| {
        let end = (start + BEATS_PER_BAR).min(beats.len());
        beats[start.min(end)..end]
            .iter()
            .map(|beat| f64::from(beat.low))
            .sum::<f64>()
            / BEATS_PER_BAR as f64
    };
    let loudest = (0..beats.len()).map(bar_low).fold(0.0, f64::max);
    if loudest <= 0.0 {
        return scores;
    }
    for index in 0..beats.len() {
        let after = bar_low(index);
        let before = beats[index.saturating_sub(BEATS_PER_BAR)..index]
            .iter()
            .map(|beat| f64::from(beat.low))
            .sum::<f64>()
            / BEATS_PER_BAR as f64;
        let hits = f64::from(beats[index].low) >= 0.5 * after;
        if hits && after >= 0.5 * loudest && after >= DROP_JUMP * before {
            scores[index % BEATS_PER_BAR] = 1.0;
            break;
        }
    }
    scores
}

/// Sections change on downbeats, so the beats where an 8-bar window differs most from the one
/// before it vote for their phase.
fn novelty_scores(beats: &[BeatEvidence]) -> [f64; BEATS_PER_BAR] {
    let feature = |beat: &BeatEvidence| {
        [
            f64::from(beat.low).ln_1p(),
            f64::from(beat.mid).ln_1p(),
            f64::from(beat.harmonic).ln_1p(),
        ]
    };
    let features: Vec<[f64; 3]> = beats.iter().map(feature).collect();
    let mean = |range: std::ops::Range<usize>| {
        let mut sum = [0.0; 3];
        for value in &features[range] {
            for (total, part) in sum.iter_mut().zip(value) {
                *total += part;
            }
        }
        sum.map(|total| total / NOVELTY_SPAN as f64)
    };
    let novelty: Vec<f64> = (0..beats.len())
        .map(|index| {
            if index < NOVELTY_SPAN || index + NOVELTY_SPAN > beats.len() {
                return 0.0;
            }
            let before = mean(index - NOVELTY_SPAN..index);
            let after = mean(index..index + NOVELTY_SPAN);
            before
                .iter()
                .zip(after)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    let mut scores = [0.0; BEATS_PER_BAR];
    for index in 1..novelty.len().saturating_sub(1) {
        if novelty[index] > novelty[index - 1] && novelty[index] >= novelty[index + 1] {
            scores[index % BEATS_PER_BAR] += novelty[index];
        }
    }
    scores
}

/// A kick on beat 1 with snares and claps on beats 2 and 4. Four-on-the-floor kicks cannot
/// tell the phases apart, but the backbeat splits them into two pairs.
fn backbeat_scores(beats: &[BeatEvidence]) -> [f64; BEATS_PER_BAR] {
    let mut low = [0.0; BEATS_PER_BAR];
    let mut mid = [0.0; BEATS_PER_BAR];
    for (index, beat) in beats.iter().enumerate() {
        low[index % BEATS_PER_BAR] += f64::from(beat.low);
        mid[index % BEATS_PER_BAR] += f64::from(beat.mid);
    }
    let low_total: f64 = low.iter().sum();
    let mid_total: f64 = mid.iter().sum();
    let mut scores = [0.0; BEATS_PER_BAR];
    for (phase, score) in scores.iter_mut().enumerate() {
        let at = |offset: usize| (phase + offset) % BEATS_PER_BAR;
        let kick = if low_total > 0.0 {
            low[phase] / low_total
        } else {
            0.0
        };
        let snare = if mid_total > 0.0 {
            (mid[at(1)] + mid[at(3)] - mid[at(0)] - mid[at(2)]) / mid_total
        } else {
            0.0
        };
        *score = kick + snare;
    }
    scores
}

fn normalised(values: [f64; BEATS_PER_BAR]) -> [f64; BEATS_PER_BAR] {
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if high - low <= 1e-9 || (high - low).is_nan() {
        return [0.0; BEATS_PER_BAR];
    }
    values.map(|value| (value - low) / (high - low))
}

/// How the beats keep time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoFit {
    /// To 0.01 BPM: the fitted tempo for a steady grid, the median bar tempo otherwise.
    pub bpm: f64,
    /// The interquartile range of the bar tempos.
    pub spread_bpm: f64,
    /// The beats keep a constant tempo, and `period_s` and `offset_s` place every beat.
    pub steady: bool,
    /// The tempo drifts, so the grid keeps each measured beat.
    pub variable: bool,
    /// The least-squares line through the beats: beat `i` at `offset_s + i * period_s`.
    pub period_s: f64,
    pub offset_s: f64,
}

/// Fits the beats' tempo. Bar tempos come from successive downbeats, or from single beats when
/// the track is too short for three bars. The line runs through the beats marked in nchored,
/// those placed on a kick, so a pickup or fill placed less exactly does not tilt it. With fewer
/// than 8 anchors it runs through every beat.
pub fn fit_tempo(beats_s: &[f64], first_downbeat: usize, anchored: &[bool]) -> Option<TempoFit> {
    if beats_s.len() < 2 {
        return None;
    }
    let downbeats: Vec<f64> = beats_s
        .iter()
        .skip(first_downbeat)
        .step_by(BEATS_PER_BAR)
        .copied()
        .collect();
    let mut local: Vec<f64> = if downbeats.len() >= 4 {
        downbeats
            .windows(2)
            .map(|pair| 60.0 * BEATS_PER_BAR as f64 / (pair[1] - pair[0]))
            .collect()
    } else {
        beats_s
            .windows(2)
            .map(|pair| 60.0 / (pair[1] - pair[0]))
            .collect()
    };
    local.retain(|bpm| bpm.is_finite() && *bpm > 0.0);
    if local.is_empty() {
        return None;
    }
    let smoothed = median_smoothed(&local, DRIFT_SMOOTHING);
    let mut sorted = local.clone();
    sorted.sort_by(f64::total_cmp);
    let spread_bpm = quantile(&sorted, 0.75) - quantile(&sorted, 0.25);
    let drift = smoothed.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - smoothed.iter().copied().fold(f64::INFINITY, f64::min);
    let (period_s, offset_s) = least_squares(beats_s, anchored);
    let steady = spread_bpm <= STEADY_IQR_BPM;
    let bpm = if steady {
        60.0 / period_s
    } else {
        quantile(&sorted, 0.5)
    };
    Some(TempoFit {
        bpm: (bpm * 100.0).round() / 100.0,
        spread_bpm,
        steady,
        variable: !steady && drift > VARIABLE_DRIFT_BPM,
        period_s,
        offset_s,
    })
}

fn least_squares(beats_s: &[f64], anchored: &[bool]) -> (f64, f64) {
    let anchors = anchored.iter().filter(|anchor| **anchor).count();
    let points: Vec<(f64, f64)> = beats_s
        .iter()
        .enumerate()
        .filter(|(index, _)| anchors < 8 || anchored.get(*index).copied().unwrap_or(false))
        .map(|(index, time)| (index as f64, *time))
        .collect();
    let count = points.len() as f64;
    let mean_index = points.iter().map(|(index, _)| index).sum::<f64>() / count;
    let mean_time = points.iter().map(|(_, time)| time).sum::<f64>() / count;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (index, time) in &points {
        let di = index - mean_index;
        covariance += di * (time - mean_time);
        variance += di * di;
    }
    let period = covariance / variance;
    (period, mean_time - period * mean_index)
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    let position = q * (sorted.len() - 1) as f64;
    let below = position.floor() as usize;
    let above = position.ceil() as usize;
    let fraction = position - below as f64;
    sorted[below] + (sorted[above] - sorted[below]) * fraction
}

fn median_smoothed(values: &[f64], half: usize) -> Vec<f64> {
    (0..values.len())
        .map(|index| {
            let start = index.saturating_sub(half);
            let end = (index + half + 1).min(values.len());
            let mut window = values[start..end].to_vec();
            window.sort_by(f64::total_cmp);
            quantile(&window, 0.5)
        })
        .collect()
}

/// What the grid's confidence is made of, each from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridEvidence {
    /// The beat tracker's mean beat probability at the chosen beats.
    pub beat_strength: f64,
    /// The share of beats that snapped to a clear transient.
    pub snapped_share: f64,
    /// The bar phase's winning margin.
    pub phase_margin: f64,
    /// The bar tempos' spread.
    pub spread_bpm: f64,
}

/// How sure the grid is, from 0 to 1. A steady tempo with every beat on a kick and a clear bar
/// phase scores near 1; a grid the tracker hesitated over, or whose bars could start on two
/// different beats, falls below `GRID_CHECK_BELOW`.
pub fn grid_confidence(evidence: GridEvidence) -> f32 {
    let steadiness = (1.0 - evidence.spread_bpm / 2.0).clamp(0.0, 1.0);
    let confidence = 0.35 * evidence.beat_strength.clamp(0.0, 1.0)
        + 0.25 * evidence.snapped_share.clamp(0.0, 1.0)
        + 0.25 * evidence.phase_margin.clamp(0.0, 1.0)
        + 0.15 * steadiness;
    confidence as f32
}

/// Whether Prep should ask the user to check a track's grid. A grid the user confirmed never
/// needs it.
pub fn needs_grid_check(provenance: AnalysisProvenance) -> bool {
    match provenance {
        AnalysisProvenance::Estimated { confidence } => confidence < GRID_CHECK_BELOW,
        AnalysisProvenance::ConfirmedByUser => false,
    }
}

impl BeatGrid {
    /// Whether the grid follows a drifting tempo rather than a constant one.
    pub fn is_variable(&self) -> bool {
        fit_tempo(&self.beats_s, self.first_downbeat as usize, &[]).is_some_and(|fit| fit.variable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steady(bpm: f64, count: usize) -> Vec<f64> {
        (0..count).map(|i| 0.25 + i as f64 * 60.0 / bpm).collect()
    }

    #[test]
    fn a_steady_tempo_fits_to_a_hundredth_of_a_bpm() {
        let beats = steady(124.0, 400);
        let fit = fit_tempo(&beats, 0, &[]).expect("fit");
        assert!(fit.steady && !fit.variable);
        assert_eq!(fit.bpm, 124.0);
        assert!((fit.offset_s - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_drifting_tempo_is_variable() {
        // 125 to 126.5 BPM over 600 beats.
        let mut beats = vec![0.0];
        for i in 1..600 {
            let bpm = 125.0 + 1.5 * i as f64 / 600.0;
            beats.push(beats[i - 1] + 60.0 / bpm);
        }
        let fit = fit_tempo(&beats, 0, &[]).expect("fit");
        assert!(!fit.steady && fit.variable);
        let grid = BeatGrid {
            bpm: fit.bpm,
            beats_s: beats,
            first_downbeat: 0,
        };
        assert!(grid.is_variable());
    }

    fn bar_of(phase: usize, index: usize) -> usize {
        (index + BEATS_PER_BAR - phase) % BEATS_PER_BAR
    }

    #[test]
    fn a_drop_and_the_backbeat_outvote_a_downbeat_logit_on_the_pickup() {
        // A 2-beat snare pickup, then a kick on every beat and a clap on 2 and 4.
        let phase = 2;
        let beats: Vec<BeatEvidence> = (0..160)
            .map(|index| {
                let in_bar = bar_of(phase, index);
                let drums = index >= phase;
                BeatEvidence {
                    downbeat_logit: if index == 0 { 4.0 } else { -2.0 },
                    full_drums: drums,
                    low: if drums { 1.0 } else { 0.0 },
                    mid: if !drums || in_bar % 2 == 1 { 0.8 } else { 0.1 },
                    harmonic: 0.2,
                }
            })
            .collect();
        let choice = choose_bar_phase(&beats);
        assert_eq!(choice.phase, phase);
        assert_eq!(choice.first_downbeat(beats.len()), 2);
    }

    #[test]
    fn grids_under_the_threshold_go_to_prep_unless_the_user_confirmed_them() {
        assert!(needs_grid_check(AnalysisProvenance::Estimated {
            confidence: 0.6
        }));
        assert!(!needs_grid_check(AnalysisProvenance::Estimated {
            confidence: 0.9
        }));
        assert!(!needs_grid_check(AnalysisProvenance::ConfirmedByUser));
    }
}
