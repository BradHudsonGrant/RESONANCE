import { describe, expect, it } from 'vitest';

import { DeckClock } from './clock';
import type { DeckReading } from './frames';

function reading(positionSeconds: number, playing = true, generation = 1): DeckReading {
  return {
    generation,
    playing,
    positionSeconds,
    durationSeconds: 300,
    meterLeft: 0,
    meterRight: 0,
  };
}

describe('DeckClock', () => {
  it('moves on with page time while the deck plays', () => {
    const clock = new DeckClock();

    clock.update(reading(10), 1000);

    expect(clock.at(1500)).toBeCloseTo(10.5);
  });

  it('stays put while the deck is stopped', () => {
    const clock = new DeckClock();

    clock.update(reading(10, false), 1000);

    expect(clock.at(5000)).toBe(10);
  });

  it('eases out a small difference instead of jumping', () => {
    const clock = new DeckClock();
    clock.update(reading(10), 1000);

    clock.update(reading(10.026), 1016);

    expect(clock.at(1016)).toBeCloseTo(10.016 + 0.01 * 0.1, 6);
    expect(clock.lastError).toBeCloseTo(0.01, 6);
  });

  it('jumps to a reading that is far off, such as after a seek', () => {
    const clock = new DeckClock();
    clock.update(reading(10), 1000);

    clock.update(reading(42), 1016);

    expect(clock.at(1016)).toBe(42);
  });

  it('starts again from the reading when a new track is loaded', () => {
    const clock = new DeckClock();
    clock.update(reading(120), 1000);

    clock.update(reading(0.01, true, 2), 1016);

    expect(clock.at(1016)).toBe(0.01);
    expect(clock.lastError).toBe(0);
  });
});
