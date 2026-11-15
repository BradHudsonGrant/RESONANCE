import { describe, expect, it } from 'vitest';

import {
  countSince,
  emptyTelemetry,
  frameKind,
  readTelemetry,
  readWaveform,
  TELEMETRY,
  WAVEFORM,
} from './frames';

/** The same reading as the Rust test in crates/api/src/mix_frame.rs, laid out by hand. */
function telemetryFrame(): ArrayBuffer {
  const frame = new ArrayBuffer(144);
  const view = new DataView(frame);
  view.setUint8(0, TELEMETRY);
  [48_000, 256, 480, 1056, 470, 490, 51_000, 1000, 2, 300, 5, 1, 0].forEach((value, field) =>
    view.setUint32(4 + field * 4, value, true),
  );
  view.setFloat64(56, 61.25, true);
  view.setFloat64(64, 300.5, true);
  view.setFloat32(72, 0.75, true);
  view.setFloat32(76, 0.5, true);
  view.setUint32(80, 3, true);
  view.setUint8(84, 1);
  view.setFloat64(96, 200, true);
  view.setFloat32(108, 0.25, true);
  view.setUint32(112, 4, true);
  view.setFloat64(120, 42.5, true);
  view.setFloat64(128, 180.0, true);
  view.setUint32(136, 7, true);
  view.setUint8(140, 1);
  return frame;
}

describe('readTelemetry', () => {
  it('reads every field from its offset', () => {
    const telemetry = emptyTelemetry();

    readTelemetry(telemetryFrame(), telemetry);

    expect(telemetry.engine).toEqual({
      sampleRate: 48_000,
      requestedBufferFrames: 256,
      periodFrames: 480,
      bufferFrames: 1056,
      windowMinFrames: 470,
      windowMaxFrames: 490,
      windowMaxCallbackNs: 51_000,
      callbacks: 1000,
      underruns: 2,
      missingFrames: 300,
      lateCallbacks: 5,
      starvedCallbacks: 1,
      streamErrors: 0,
    });
    expect(telemetry.decks).toEqual([
      {
        generation: 3,
        playing: true,
        positionSeconds: 61.25,
        durationSeconds: 300.5,
        meterLeft: 0.75,
        meterRight: 0.5,
      },
      {
        generation: 4,
        playing: false,
        positionSeconds: 0,
        durationSeconds: 200,
        meterLeft: 0,
        meterRight: 0.25,
      },
    ]);
    expect(telemetry.preview).toEqual({
      generation: 7,
      playing: true,
      positionSeconds: 42.5,
      durationSeconds: 180.0,
    });
  });

  it('reuses the objects it is given', () => {
    const telemetry = emptyTelemetry();
    const deck = telemetry.decks[0];
    const preview = telemetry.preview;

    readTelemetry(telemetryFrame(), telemetry);

    expect(telemetry.decks[0]).toBe(deck);
    expect(telemetry.preview).toBe(preview);
  });

  it('refuses a frame that is cut short', () => {
    expect(() => readTelemetry(new ArrayBuffer(60), emptyTelemetry())).toThrow('cut short');
  });
});

describe('readWaveform', () => {
  it('reads the deck, generation and bins after the header', () => {
    const frame = new Uint8Array(20);
    const view = new DataView(frame.buffer);
    view.setUint8(0, WAVEFORM);
    view.setUint8(1, 1);
    view.setUint32(4, 7, true);
    view.setUint32(8, 300, true);
    view.setUint32(12, 2, true);
    frame.set([10, 5, 255, 90], 16);

    const waveform = readWaveform(frame.buffer);

    expect(frameKind(frame.buffer)).toBe(WAVEFORM);
    expect(waveform.deck).toBe(1);
    expect(waveform.generation).toBe(7);
    expect(waveform.binsPerSecond).toBe(300);
    expect([...waveform.bins]).toEqual([10, 5, 255, 90]);
  });
});

describe('countSince', () => {
  it('counts across the wrap at 2^32', () => {
    expect(countSince(5, 2)).toBe(3);
    expect(countSince(1, 2 ** 32 - 2)).toBe(3);
  });
});
