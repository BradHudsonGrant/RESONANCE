import type { DeckReading } from './frames';

/** A reading further than this from where the playhead has got to moves it there at once. */
const SNAP_SECONDS = 0.05;

/** The share of a small difference taken up at each reading. Smooths the arrival jitter. */
const EASE = 0.1;

/**
 * A deck's playhead between readings from the core. The audio clock is the only clock: each
 * reading says where the deck is, and in between the playhead moves on with page time while the
 * deck plays. Small differences are eased out over a few readings so the waveform never jumps.
 */
export class DeckClock {
  #base = 0;
  #at = 0;
  #playing = false;
  #generation = 0;
  /** The last reading's distance from where the playhead had got to, in seconds. */
  lastError = 0;

  /** `now` is when the reading arrived, on the clock `performance.now()` reads, in milliseconds. */
  update(reading: DeckReading, now: number): void {
    const predicted = this.at(now);
    const error = reading.positionSeconds - predicted;
    const sameRun = reading.generation === this.#generation && reading.playing === this.#playing;
    this.#base =
      sameRun && Math.abs(error) <= SNAP_SECONDS
        ? predicted + error * EASE
        : reading.positionSeconds;
    this.#at = now;
    this.#playing = reading.playing;
    this.#generation = reading.generation;
    this.lastError = sameRun ? error : 0;
  }

  at(now: number): number {
    return this.#playing ? this.#base + (now - this.#at) / 1000 : this.#base;
  }

  get playing(): boolean {
    return this.#playing;
  }
}
