//! Beat grids on made-up club tracks whose kick onsets are known to the sample.

use std::f64::consts::TAU;
use std::path::PathBuf;

use resonance_analysis::beats::{
    BeatAudio, BeatEstimate, BeatTracker, CoarseBeats, MEL_FRAMES_PER_SECOND, estimate_grid,
};
use resonance_analysis::runtime::{ExecutionTarget, ModelSession};

const RATE: f64 = 44_100.0;

/// A repeatable noise source, so every run builds the same track.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// A track with known beats: a kick with a click on every beat from `first_kick`, a clap on beats 2 and 4,
/// an open hat on each off-beat, a bass note between kicks and a quiet pad throughout. Beats
/// before `first_kick` carry a clap and a vocal-like tone only, as a pickup does.
struct Track {
    mono: Vec<f32>,
    beats_s: Vec<f64>,
    kicks: Vec<usize>,
}

fn track(beats_s: Vec<f64>, first_kick: usize) -> Track {
    let end = beats_s[beats_s.len() - 1] + 1.0;
    let mut mono = vec![0.0f64; (end * RATE) as usize];
    let mut noise = Noise(7);
    let mut kicks = Vec::new();
    let add = |mono: &mut Vec<f64>, at: usize, len: usize, sample: &mut dyn FnMut(f64) -> f64| {
        for n in 0..len {
            if let Some(slot) = mono.get_mut(at + n) {
                *slot += sample(n as f64 / RATE);
            }
        }
    };
    for (index, beat) in beats_s.iter().enumerate() {
        let at = (beat * RATE).round() as usize;
        let next = beats_s.get(index + 1).copied().unwrap_or(beat + 0.47);
        let gap = next - beat;
        let in_bar = (index + 4 - first_kick % 4) % 4;
        if index >= first_kick {
            kicks.push(at);
            // A kick sweeping from 150 Hz to 50 Hz, starting on an upward zero-crossing.
            add(&mut mono, at, (0.25 * RATE) as usize, &mut |t| {
                let phase = TAU * (50.0 * t + 100.0 * 0.03 * (1.0 - (-t / 0.03).exp()));
                0.8 * (-t / 0.07).exp() * phase.sin()
            });
            // The beater's click: 2 ms of a 3 kHz tone, which also starts upwards.
            add(&mut mono, at, (0.002 * RATE) as usize, &mut |t| {
                0.5 * (-t / 0.0007).exp() * (TAU * 3_000.0 * t).sin()
            });
            // A bass note on the off-beat.
            let bass_at = ((beat + gap / 2.0) * RATE).round() as usize;
            add(&mut mono, bass_at, (gap / 2.5 * RATE) as usize, &mut |t| {
                let fade = (t / 0.005).min(1.0);
                0.2 * fade * (TAU * 55.0 * t).sin()
            });
            let hat_at = bass_at;
            add(&mut mono, hat_at, (0.05 * RATE) as usize, &mut |t| {
                0.08 * (-t / 0.015).exp() * noise.next()
            });
        } else {
            add(&mut mono, at, (gap * 0.8 * RATE) as usize, &mut |t| {
                0.2 * (TAU * 330.0 * t).sin() * (1.0 - t / (gap * 0.8))
            });
        }
        if in_bar % 2 == 1 || index < first_kick {
            add(&mut mono, at, (0.12 * RATE) as usize, &mut |t| {
                0.25 * (-t / 0.04).exp() * noise.next()
            });
        }
    }
    for (n, slot) in mono.iter_mut().enumerate() {
        let t = n as f64 / RATE;
        *slot += 0.04 * ((TAU * 220.0 * t).sin() + (TAU * 277.18 * t).sin());
    }
    Track {
        mono: mono.into_iter().map(|value| value as f32).collect(),
        beats_s,
        kicks,
    }
}

fn steady(bpm: f64, beats: usize) -> Vec<f64> {
    (0..beats).map(|i| 0.5 + i as f64 * 60.0 / bpm).collect()
}

/// Coarse beats as the model reports them: on its 20 ms frames, with downbeat logits that
/// favour `downbeat_phase`.
fn coarse(beats_s: &[f64], downbeat_logit: impl Fn(usize) -> f32) -> CoarseBeats {
    CoarseBeats {
        beats_s: beats_s
            .iter()
            .map(|beat| (beat * MEL_FRAMES_PER_SECOND).round() / MEL_FRAMES_PER_SECOND)
            .collect(),
        strength: vec![0.9; beats_s.len()],
        downbeat_logit: (0..beats_s.len()).map(downbeat_logit).collect(),
    }
}

/// Every kick has a marker under 1 ms from its onset, on an upward zero-crossing unless the mix
/// has none within 0.5 ms of the kick, and bar 1 beat 1 is the first kick. Returns the worst
/// error in ms.
fn assert_on_kicks(track: &Track, estimate: &BeatEstimate) -> f64 {
    let beats = &estimate.grid.beats_s;
    let rises_at = |n: usize| track.mono[n - 1] <= 0.0 && track.mono[n] > 0.0;
    let mut worst: f64 = 0.0;
    for (index, kick) in track.kicks.iter().enumerate() {
        let kick_s = *kick as f64 / RATE;
        let marker = beats
            .iter()
            .copied()
            .min_by(|a, b| (a - kick_s).abs().total_cmp(&(b - kick_s).abs()))
            .expect("markers");
        let error_ms = (marker - kick_s).abs() * 1_000.0;
        worst = worst.max(error_ms);
        assert!(
            error_ms < 1.0,
            "kick {index} is {error_ms:.3} ms from its marker"
        );
        let sample = (marker * RATE).round() as usize;
        // In a mix the kick can start on top of another sound, leaving no crossing to find.
        let none_to_find = !(kick - 22..=kick + 22).any(rises_at);
        assert!(
            rises_at(sample) || none_to_find,
            "kick {index} is not on a zero-crossing"
        );
    }
    let bar_one = beats[estimate.grid.first_downbeat as usize];
    assert!(
        (bar_one - track.kicks[0] as f64 / RATE).abs() < 0.001,
        "bar 1 beat 1 at {bar_one} s is not the first kick"
    );
    worst
}
fn analyse(track: &Track, coarse: &CoarseBeats) -> BeatEstimate {
    let audio = BeatAudio::from_mono(track.mono.clone()).expect("audio");
    estimate_grid(&audio, coarse).expect("a grid")
}

fn on_phase(phase: usize) -> impl Fn(usize) -> f32 {
    move |index| if index % 4 == phase { 3.0 } else { -3.0 }
}

#[test]
fn steady_tempos_land_on_every_kick_with_a_constant_grid() {
    for bpm in [124.0, 128.0, 174.0] {
        let track = track(steady(bpm, 128), 0);
        let estimate = analyse(&track, &coarse(&track.beats_s, on_phase(0)));
        let worst = assert_on_kicks(&track, &estimate);
        assert_eq!(estimate.grid.bpm, bpm);
        assert!(!estimate.variable);
        assert_eq!(estimate.grid.first_downbeat, 0);
        assert!(
            estimate.confidence >= 0.75,
            "confidence {}",
            estimate.confidence
        );
        println!(
            "{bpm} BPM: worst {worst:.3} ms, confidence {:.2}",
            estimate.confidence
        );
    }
}

#[test]
fn a_two_beat_pickup_leaves_bar_one_on_the_first_kick() {
    let track = track(steady(128.0, 130), 2);
    // The tracker latches onto the pickup's opening clap and is unsure everywhere else.
    let estimate = analyse(
        &track,
        &coarse(&track.beats_s, |index| if index == 0 { 3.0 } else { -1.0 }),
    );
    assert_on_kicks(&track, &estimate);
    assert_eq!(estimate.grid.first_downbeat, 2);
}

/// 160 beats speeding up evenly from 125 to 126.5 BPM, as a vinyl rip might.
fn drifting() -> Vec<f64> {
    let mut beats = vec![0.5];
    let count = 160;
    for i in 1..count {
        let bpm = 125.0 + 1.5 * i as f64 / count as f64;
        beats.push(beats[i - 1] + 60.0 / bpm);
    }
    beats
}

#[test]
fn a_drifting_tempo_keeps_its_beats_and_is_variable() {
    let track = track(drifting(), 0);
    let estimate = analyse(&track, &coarse(&track.beats_s, on_phase(0)));
    assert_on_kicks(&track, &estimate);
    assert!(estimate.variable);
    assert!(estimate.grid.is_variable());
}

/// The whole pipeline with the published model. Its weights are not in the repository, so
/// this runs only when `RESONANCE_BEAT_THIS_ONNX` names a copy.
#[test]
#[ignore = "needs the Beat This! ONNX file"]
fn the_model_finds_the_grid_of_a_made_up_track() {
    let weights = PathBuf::from(std::env::var("RESONANCE_BEAT_THIS_ONNX").expect("weights path"));
    let cache = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("beats-trt-cache");
    let session =
        ModelSession::open_on(&weights, ExecutionTarget::Cpu, &cache).expect("model session");
    let mut tracker = BeatTracker::new(session);
    for (bpm, first_kick) in [(124.0, 0), (128.0, 2), (174.0, 0)] {
        let track = track(steady(bpm, 128 + first_kick), first_kick);
        let audio = BeatAudio::from_mono(track.mono.clone()).expect("audio");
        let estimate = tracker.track(&audio).expect("tracking").expect("a grid");
        println!(
            "{bpm} BPM, pickup {first_kick}: {} beats at {} BPM, first downbeat {}, confidence {:.2}",
            estimate.grid.beats_s.len(),
            estimate.grid.bpm,
            estimate.grid.first_downbeat,
            estimate.confidence
        );
        let worst = assert_on_kicks(&track, &estimate);
        println!("worst {worst:.3} ms");
        assert_eq!(estimate.grid.bpm, bpm);
        assert!(!estimate.variable);
    }
    let track = track(drifting(), 0);
    let audio = BeatAudio::from_mono(track.mono.clone()).expect("audio");
    let estimate = tracker.track(&audio).expect("tracking").expect("a grid");
    let worst = assert_on_kicks(&track, &estimate);
    println!(
        "125 to 126.5 BPM: variable {}, confidence {:.2}, worst {worst:.3} ms",
        estimate.variable, estimate.confidence
    );
    assert!(estimate.variable);
}
