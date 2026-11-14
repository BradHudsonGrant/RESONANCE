import { countSince } from './frames';
import { mixProbe } from './probe';

/**
 * Measures the open Mix view while both decks play, for the budget in docs/architecture.md. The
 * dev build puts it on `window`. Each frame's length is the time between two
 * `requestAnimationFrame` callbacks, as in `measureTableScroll`; the view's own cost is its
 * UI-thread time for the frame. The audio counts come from the engine's counters over the run.
 */

export interface MixMeasurement {
  frames: number;
  seconds: number;
  /** The median frame interval, which is the display's refresh while frames are held. */
  refreshMs: number;
  p50Ms: number;
  p99Ms: number;
  maxMs: number;
  /** Frames longer than one and a half display frames, so at least one was missed. */
  missedFrames: number;
  framesOver8Ms: number;
  viewP50Ms: number;
  viewP99Ms: number;
  viewMaxMs: number;
  audio: {
    sampleRate: number;
    requestedBufferFrames: number;
    periodFrames: number;
    bufferFrames: number;
    fewestCallbackFrames: number;
    mostCallbackFrames: number;
    callbacks: number;
    underruns: number;
    missingFrames: number;
    lateCallbacks: number;
    starvedCallbacks: number;
    streamErrors: number;
    longestCallbackMs: number;
  };
  playhead: {
    readings: number;
    /** How far a reading from the core was from where the playhead had got to. */
    p50ErrorMs: number;
    p99ErrorMs: number;
    maxErrorMs: number;
  };
}

function percentile(sorted: Float64Array, share: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))] ?? 0;
}

function nextFrame(): Promise<number> {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

export async function measureMix(frames = 1200): Promise<MixMeasurement> {
  if (!mixProbe.attached) throw new Error('Open the Mix view first.');
  if (!mixProbe.playing[0] || !mixProbe.playing[1]) throw new Error('Play both decks first.');
  const started = performance.now();
  mixProbe.start(frames);
  while (!mixProbe.done) {
    if (!mixProbe.attached) throw new Error('The Mix view closed during the measurement.');
    await nextFrame();
  }
  mixProbe.stop();
  const seconds = (performance.now() - started) / 1000;

  const gaps = mixProbe.gaps.slice(0, mixProbe.frames).sort();
  const costs = mixProbe.costs.slice(0, mixProbe.frames).sort();
  const errors = mixProbe.clockErrors.slice(0, mixProbe.clockErrorCount).sort();
  const refreshMs = percentile(gaps, 0.5);
  const first = mixProbe.first ?? mixProbe.latest;
  const last = mixProbe.latest;
  return {
    frames: mixProbe.frames,
    seconds,
    refreshMs,
    p50Ms: refreshMs,
    p99Ms: percentile(gaps, 0.99),
    maxMs: gaps.at(-1) ?? 0,
    missedFrames: gaps.filter((gap) => gap > refreshMs * 1.5).length,
    framesOver8Ms: gaps.filter((gap) => gap > 8).length,
    viewP50Ms: percentile(costs, 0.5),
    viewP99Ms: percentile(costs, 0.99),
    viewMaxMs: costs.at(-1) ?? 0,
    audio: {
      sampleRate: last.sampleRate,
      requestedBufferFrames: last.requestedBufferFrames,
      periodFrames: last.periodFrames,
      bufferFrames: last.bufferFrames,
      fewestCallbackFrames: Number.isFinite(mixProbe.fewestCallbackFrames)
        ? mixProbe.fewestCallbackFrames
        : 0,
      mostCallbackFrames: mixProbe.mostCallbackFrames,
      callbacks: countSince(last.callbacks, first.callbacks),
      underruns: countSince(last.underruns, first.underruns),
      missingFrames: countSince(last.missingFrames, first.missingFrames),
      lateCallbacks: countSince(last.lateCallbacks, first.lateCallbacks),
      starvedCallbacks: countSince(last.starvedCallbacks, first.starvedCallbacks),
      streamErrors: countSince(last.streamErrors, first.streamErrors),
      longestCallbackMs: mixProbe.longestCallbackNs / 1e6,
    },
    playhead: {
      readings: mixProbe.readings,
      p50ErrorMs: percentile(errors, 0.5),
      p99ErrorMs: percentile(errors, 0.99),
      maxErrorMs: errors.at(-1) ?? 0,
    },
  };
}
