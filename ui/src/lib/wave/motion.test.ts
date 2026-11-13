import { describe, expect, it } from 'vitest';

import { type Listening, presetSettings, WaveMotion } from './motion';

const music: Listening = {
  low: 0.6,
  mid: 0.4,
  high: 0.3,
  tone: 0.55,
  spread: 0.2,
  beatSeconds: 0.5,
  onBeat: false,
};

function run(motion: WaveMotion, seconds: number, listening: (t: number) => Listening | null) {
  const dt = 1 / 60;
  for (let t = 0; t < seconds; t += dt) motion.step(dt, listening(t));
}

describe('WaveMotion', () => {
  it('drifts on its own when nothing is playing', () => {
    const motion = new WaveMotion(presetSettings('balanced'));

    run(motion, 10, () => null);

    expect(motion.surface.flow).toBeCloseTo(0.35, 2);
    expect(motion.surface.gain).toEqual([1, 1, 0.4, 0]);
  });

  it('holds still on the Still preset', () => {
    const motion = new WaveMotion(presetSettings('still'));

    run(motion, 10, (t) => ({ ...music, onBeat: t % 0.5 < 1 / 60 }));

    expect(motion.surface.flow).toBe(0);
    expect(motion.surface.ripple).toBe(0);
    expect(motion.surface.gain[3]).toBe(0);
  });

  it('flows faster with music, and faster still on Lively', () => {
    const balanced = new WaveMotion(presetSettings('balanced'));
    const lively = new WaveMotion(presetSettings('lively'));
    const idle = new WaveMotion(presetSettings('balanced'));

    run(balanced, 10, () => music);
    run(lively, 10, () => music);
    run(idle, 10, () => null);

    expect(balanced.surface.flow).toBeGreaterThan(idle.surface.flow);
    expect(lively.surface.flow).toBeGreaterThan(balanced.surface.flow);
  });

  it('sends a ring out from the crest on each beat', () => {
    const motion = new WaveMotion(presetSettings('balanced'));
    run(motion, 4, () => music);

    motion.step(1 / 60, { ...music, onBeat: true });
    const justAfter = motion.surface.ring;
    run(motion, 0.3, () => music);

    expect(justAfter).toBeLessThan(0.05);
    expect(motion.surface.ring).toBeGreaterThan(justAfter);
    expect(motion.surface.gain[3]).toBeGreaterThan(0);
  });

  it('settles in a breakdown, when the low end drops out and the music carries on', () => {
    const motion = new WaveMotion(presetSettings('balanced'));
    run(motion, 20, () => music);
    const swellBefore = motion.surface.gain[0];

    run(motion, 6, () => ({ ...music, low: 0.02 }));

    expect(motion.surface.gain[0]).toBeLessThan(swellBefore);
    expect(motion.surface.width).toBeGreaterThan(0.26);
  });

  it('moves the ridge towards where the tone sits, smoothly', () => {
    const motion = new WaveMotion(presetSettings('balanced'));

    motion.step(1 / 60, { ...music, tone: 0.7 });
    const firstStep = motion.surface.ridge;
    run(motion, 10, () => ({ ...music, tone: 0.7 }));

    expect(Math.abs(firstStep - 0.6)).toBeLessThan(0.01);
    expect(motion.surface.ridge).toBeCloseTo(0.78, 2);
  });
});
