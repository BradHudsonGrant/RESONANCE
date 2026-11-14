import type { EngineReading, Telemetry } from './frames';

/**
 * What the Mix view records while a measurement runs: each frame's interval and the view's own
 * time on the UI thread, and every reading from the core. The arrays are sized when the run
 * starts, so recording allocates nothing, and outside a run each call returns at once.
 */
export class MixProbe {
  #active = false;
  #last = 0;
  #capacity = 0;
  /** True while a Mix view is open and drawing. */
  attached = false;
  playing: [boolean, boolean] = [false, false];
  frames = 0;
  gaps = new Float64Array(0);
  costs = new Float64Array(0);
  readings = 0;
  /** How far each reading was from where the playhead had got to, in milliseconds. */
  clockErrors = new Float64Array(0);
  clockErrorCount = 0;
  first: EngineReading | null = null;
  readonly latest: EngineReading = {
    sampleRate: 0,
    requestedBufferFrames: 0,
    periodFrames: 0,
    bufferFrames: 0,
    windowMinFrames: 0,
    windowMaxFrames: 0,
    windowMaxCallbackNs: 0,
    callbacks: 0,
    underruns: 0,
    missingFrames: 0,
    lateCallbacks: 0,
    starvedCallbacks: 0,
    streamErrors: 0,
  };
  longestCallbackNs = 0;
  fewestCallbackFrames = Infinity;
  mostCallbackFrames = 0;

  start(frames: number): void {
    this.#capacity = frames;
    this.frames = 0;
    this.gaps = new Float64Array(frames);
    this.costs = new Float64Array(frames);
    this.readings = 0;
    // Readings come at about 60 a second, two decks each, against frames at up to 240 a second.
    this.clockErrors = new Float64Array(frames * 2);
    this.clockErrorCount = 0;
    this.first = null;
    this.longestCallbackNs = 0;
    this.fewestCallbackFrames = Infinity;
    this.mostCallbackFrames = 0;
    this.#last = 0;
    this.#active = true;
  }

  stop(): void {
    this.#active = false;
  }

  get done(): boolean {
    return this.frames >= this.#capacity;
  }

  frame(now: number, costMs: number): void {
    if (!this.#active || this.frames >= this.#capacity) return;
    if (this.#last > 0) {
      this.gaps[this.frames] = now - this.#last;
      this.costs[this.frames] = costMs;
      this.frames += 1;
    }
    this.#last = now;
  }

  reading(telemetry: Telemetry, errorA: number, errorB: number): void {
    this.playing[0] = telemetry.decks[0].playing;
    this.playing[1] = telemetry.decks[1].playing;
    if (!this.#active || this.frames >= this.#capacity) return;
    const engine = telemetry.engine;
    this.first ??= { ...engine };
    Object.assign(this.latest, engine);
    this.readings += 1;
    this.longestCallbackNs = Math.max(this.longestCallbackNs, engine.windowMaxCallbackNs);
    if (engine.windowMinFrames > 0)
      this.fewestCallbackFrames = Math.min(this.fewestCallbackFrames, engine.windowMinFrames);
    this.mostCallbackFrames = Math.max(this.mostCallbackFrames, engine.windowMaxFrames);
    if (telemetry.decks[0].playing) this.#clockError(errorA);
    if (telemetry.decks[1].playing) this.#clockError(errorB);
  }

  #clockError(seconds: number): void {
    if (this.clockErrorCount < this.clockErrors.length)
      this.clockErrors[this.clockErrorCount++] = Math.abs(seconds) * 1000;
  }
}

export const mixProbe = new MixProbe();
