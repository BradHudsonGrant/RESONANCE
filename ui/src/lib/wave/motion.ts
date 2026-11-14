/**
 * How the wave moves, ported from the reference page with its maths unchanged. The reference
 * estimated tempo and beats from a Web Audio analyser; here they come in as `Listening`, from the
 * grid and the engine's metering tap, so the component has no audio of its own.
 */

const TAU = Math.PI * 2;
const SILENCE = 0.03;
const IDLE_FLOW_RATE = 0.035;
const IDLE_RIPPLE_RATE = 0.03;

export type Density = 'thumb' | 'low' | 'medium' | 'high';

export const DENSITY: Record<Density, readonly [columns: number, rows: number]> = {
  thumb: [38, 16],
  low: [170, 68],
  medium: [260, 104],
  high: [360, 144],
};

export interface WaveSettings {
  drift: number;
  reactivity: number;
  smoothing: number;
  followTempo: boolean;
  followTone: boolean;
  calmBreakdowns: boolean;
  barsPerCycle: number;
  bass: number;
  mids: number;
  highs: number;
  pulse: number;
  height: number;
  pointSize: number;
  elevation: number;
  zoom: number;
  density: Density;
  frameCap: number;
}

export type Preset = 'still' | 'calm' | 'balanced' | 'lively';

const BALANCED: WaveSettings = {
  drift: 1,
  reactivity: 0.6,
  smoothing: 0.6,
  followTempo: true,
  followTone: true,
  calmBreakdowns: true,
  barsPerCycle: 32,
  bass: 1,
  mids: 1,
  highs: 1,
  pulse: 0.6,
  height: 1,
  pointSize: 1,
  elevation: 0.5,
  zoom: 1,
  density: 'medium',
  frameCap: 60,
};

const PRESETS: Record<Preset, Partial<WaveSettings>> = {
  still: { drift: 0, reactivity: 0, pulse: 0 },
  calm: { drift: 0.6, reactivity: 0.35, pulse: 0.3, smoothing: 0.8 },
  balanced: {},
  lively: { drift: 1.6, reactivity: 1, pulse: 1, smoothing: 0.4, barsPerCycle: 16 },
};

export function presetSettings(preset: Preset): WaveSettings {
  return { ...BALANCED, ...PRESETS[preset] };
}

/** What the music is doing this frame. Band levels and tone run from 0 to 1. */
export interface Listening {
  low: number;
  mid: number;
  high: number;
  /** Where the spectral energy sits, and how far it spreads. */
  tone: number;
  spread: number;
  beatSeconds: number;
  /** True on the frame a beat lands, from the grid's beat phase. */
  onBeat: boolean;
}

/** Band energy for a track's thumbnail. */
export interface TrackBands {
  low?: number;
  mid?: number;
  high?: number;
  tone?: number;
  spread?: number;
}

/** The uniforms one frame is drawn with. */
export interface Surface {
  flow: number;
  ripple: number;
  ring: number;
  ridge: number;
  width: number;
  gain: [swell: number, cross: number, ripple: number, ring: number];
}

const clamp = (value: number, min = 0, max = 1) => Math.min(max, Math.max(min, value));
const approach = (current: number, target: number, seconds: number, dt: number) =>
  target + (current - target) * Math.exp(-dt / seconds);
const follow = (current: number, target: number, attack: number, release: number, dt: number) =>
  approach(current, target, target > current ? attack : release, dt);

function seededUnit(seed: number, salt: number): number {
  let state =
    (Math.imul(Math.trunc(seed) ^ 0x9e3779b9, 0x85ebca6b) + Math.imul(salt, 0xc2b2ae35)) | 0;
  state = Math.imul(state ^ (state >>> 16), 0x7feb352d);
  state = Math.imul(state ^ (state >>> 15), 0x846ca68b);
  state = state ^ (state >>> 16);
  return (state >>> 0) / 4294967296;
}

/** Still surface from a track's seed and band energy. */
export function trackSurface(seed: number, bands?: TrackBands): Surface {
  const low = clamp(bands?.low ?? 0.25 + 0.65 * seededUnit(seed, 1));
  const mid = clamp(bands?.mid ?? 0.2 + 0.65 * seededUnit(seed, 2));
  const high = clamp(bands?.high ?? 0.15 + 0.65 * seededUnit(seed, 3));
  const tone = clamp(bands?.tone ?? 0.25 + 0.5 * seededUnit(seed, 4));
  const spread = clamp(bands?.spread ?? 0.16 + 0.42 * seededUnit(seed, 5));

  return {
    flow: seededUnit(seed, 6) * TAU,
    ripple: seededUnit(seed, 7) * TAU,
    ring: 0.22 + 0.52 * seededUnit(seed, 8),
    ridge: 0.42 + 0.36 * clamp((tone - 0.3) / 0.4),
    width: clamp(0.2 + 0.5 * spread, 0.18, 0.36),
    gain: [1 + 0.85 * low, 1 + 0.75 * mid, 0.4 + 1.8 * high, 0.35 * low],
  };
}

export class WaveMotion {
  settings: WaveSettings;
  readonly surface: Surface = {
    flow: 0,
    ripple: 0,
    ring: 1,
    ridge: 0.6,
    width: 0.26,
    gain: [1, 1, 0.4, 0],
  };
  #low = 0;
  #mid = 0;
  #high = 0;
  #levelSlow = 0;
  #lowSlow = 0;
  #lowLong = 0;
  #activity = 0;
  #calm = 0;
  #beat = 0;
  #sinceBeat = 9;
  #period = 0.5;
  #tone = 0.5;
  #spread = 0.25;

  constructor(settings: WaveSettings) {
    this.settings = settings;
  }

  /** Moves the surface on by `dt` seconds. Allocates nothing. */
  step(dt: number, listening: Listening | null) {
    this.#listen(dt, listening);
    this.#advance(dt);
  }

  #listen(dt: number, listening: Listening | null) {
    const { smoothing, calmBreakdowns } = this.settings;
    const low = listening?.low ?? 0;
    const mid = listening?.mid ?? 0;
    const high = listening?.high ?? 0;
    const level = (low + mid + high) / 3;
    if (listening && level > SILENCE) {
      this.#tone = listening.tone;
      this.#spread = listening.spread;
    }

    const attack = 0.02 + 0.1 * smoothing;
    const release = 0.12 + 0.6 * smoothing;
    this.#low = follow(this.#low, low, attack, release, dt);
    this.#mid = follow(this.#mid, mid, attack, release * 0.7, dt);
    this.#high = follow(this.#high, high, attack, release * 0.5, dt);
    this.#levelSlow = approach(this.#levelSlow, level, 3, dt);
    this.#lowSlow = approach(this.#lowSlow, low, 1.2, dt);
    this.#lowLong = approach(this.#lowLong, low, 10, dt);
    this.#activity = approach(this.#activity, level > SILENCE ? 1 : 0, 0.6, dt);

    this.#sinceBeat += dt;
    if (listening?.onBeat) {
      this.#period = listening.beatSeconds;
      this.#beat = 1;
      this.#sinceBeat = 0;
    } else {
      this.#beat = approach(this.#beat, 0, 0.25 + 0.35 * smoothing, dt);
    }

    const inBreakdown =
      calmBreakdowns && this.#activity > 0.5 && this.#lowSlow < this.#lowLong * 0.5;
    this.#calm = approach(this.#calm, inBreakdown ? 1 : 0, 1.2, dt);
  }

  #advance(dt: number) {
    const s = this.settings;
    const surface = this.surface;
    const playing = this.#activity > 0.5;
    const react = s.reactivity * this.#activity;
    const quiet = 1 - 0.6 * this.#calm;

    let flowRate = IDLE_FLOW_RATE * s.drift;
    if (s.followTempo) {
      const cycleSeconds = this.#period * 4 * s.barsPerCycle;
      flowRate += react * (TAU / cycleSeconds) * (0.6 + 0.8 * this.#beat);
    } else {
      flowRate += react * 0.08;
    }
    surface.flow += dt * flowRate;
    surface.ripple += dt * (IDLE_RIPPLE_RATE * s.drift + react * s.highs * this.#high * 1.5);

    const ringSeconds = clamp(this.#period * 1.2, 0.35, 0.9);
    surface.ring = 0.8 * Math.min(1, this.#sinceBeat / ringSeconds);

    const follows = s.followTone && playing;
    const ridge = follows ? 0.42 + 0.36 * clamp((this.#tone - 0.3) / 0.4) : 0.6;
    const width = (follows ? 0.2 + 0.5 * this.#spread : 0.26) + 0.05 * this.#calm;
    surface.ridge = approach(surface.ridge, ridge, 1.4, dt);
    surface.width = approach(surface.width, clamp(width, 0.18, 0.36), 1.4, dt);

    surface.gain[0] =
      s.height * (1 + react * quiet * (0.7 * s.bass * this.#low + 0.5 * this.#levelSlow));
    surface.gain[1] = s.height * (1 + react * quiet * s.mids * this.#mid);
    surface.gain[2] = 0.4 + react * quiet * 3 * s.highs * this.#high;
    surface.gain[3] = react * s.pulse * this.#beat;
  }
}
