/**
 * Reads the core's binary frames. The layouts are set in `crates/api/src/mix_frame.rs` and
 * `crates/api/src/overviews.rs`, and the offsets here have to match them. Numbers are
 * little-endian.
 */

export const TELEMETRY = 1;
export const WAVEFORM = 2;
export const OVERVIEWS = 3;
/** Bytes in one waveform bin or overview column: peak, then the low, mid and high band levels. */
export const BIN_BYTES = 4;
const TELEMETRY_LENGTH = 144;
const DECKS_AT = 56;
const DECK_LENGTH = 32;
const PREVIEW_AT = 120;
const WAVEFORM_HEADER_LENGTH = 16;
const OVERVIEWS_HEADER_LENGTH = 12;

export interface DeckReading {
  /** Zero while the deck is empty. Matches the waveform for the same load. */
  generation: number;
  playing: boolean;
  /** The position being heard when the core took the reading. */
  positionSeconds: number;
  durationSeconds: number;
  /** From 0 at the meter's floor to 1 at full scale. */
  meterLeft: number;
  meterRight: number;
}

/** Counts from the audio thread. Totals wrap at 2^32; `window` values cover one reading. */
export interface EngineReading {
  sampleRate: number;
  requestedBufferFrames: number;
  periodFrames: number;
  bufferFrames: number;
  windowMinFrames: number;
  windowMaxFrames: number;
  windowMaxCallbackNs: number;
  callbacks: number;
  underruns: number;
  missingFrames: number;
  lateCallbacks: number;
  starvedCallbacks: number;
  streamErrors: number;
}

export interface PreviewReading {
  /** Zero while nothing is loaded on the preview channel. */
  generation: number;
  playing: boolean;
  positionSeconds: number;
  durationSeconds: number;
}

export interface Telemetry {
  decks: [DeckReading, DeckReading];
  preview: PreviewReading;
  engine: EngineReading;
}

export interface WaveformFrame {
  deck: 0 | 1;
  generation: number;
  binsPerSecond: number;
  /** Four bytes for each bin, peak then the low, mid and high band RMS, 255 at full scale. */
  bins: Uint8Array;
}

function emptyDeck(): DeckReading {
  return {
    generation: 0,
    playing: false,
    positionSeconds: 0,
    durationSeconds: 0,
    meterLeft: 0,
    meterRight: 0,
  };
}

function emptyPreview(): PreviewReading {
  return {
    generation: 0,
    playing: false,
    positionSeconds: 0,
    durationSeconds: 0,
  };
}

export function emptyTelemetry(): Telemetry {
  return {
    decks: [emptyDeck(), emptyDeck()],
    preview: emptyPreview(),
    engine: {
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
    },
  };
}

export function frameKind(frame: ArrayBuffer): number {
  return frame.byteLength > 0 ? new DataView(frame).getUint8(0) : 0;
}

/** Writes a telemetry frame into `into`, reusing its objects. */
export function readTelemetry(frame: ArrayBuffer, into: Telemetry): void {
  if (frame.byteLength < TELEMETRY_LENGTH) throw new Error('A telemetry frame was cut short.');
  const view = new DataView(frame);
  const u32 = (at: number) => view.getUint32(at, true);
  const engine = into.engine;
  engine.sampleRate = u32(4);
  engine.requestedBufferFrames = u32(8);
  engine.periodFrames = u32(12);
  engine.bufferFrames = u32(16);
  engine.windowMinFrames = u32(20);
  engine.windowMaxFrames = u32(24);
  engine.windowMaxCallbackNs = u32(28);
  engine.callbacks = u32(32);
  engine.underruns = u32(36);
  engine.missingFrames = u32(40);
  engine.lateCallbacks = u32(44);
  engine.starvedCallbacks = u32(48);
  engine.streamErrors = u32(52);
  for (let index = 0; index < 2; index++) {
    const at = DECKS_AT + index * DECK_LENGTH;
    const deck = into.decks[index]!;
    deck.positionSeconds = view.getFloat64(at, true);
    deck.durationSeconds = view.getFloat64(at + 8, true);
    deck.meterLeft = view.getFloat32(at + 16, true);
    deck.meterRight = view.getFloat32(at + 20, true);
    deck.generation = view.getUint32(at + 24, true);
    deck.playing = view.getUint8(at + 28) === 1;
  }
  const preview = into.preview;
  preview.positionSeconds = view.getFloat64(PREVIEW_AT, true);
  preview.durationSeconds = view.getFloat64(PREVIEW_AT + 8, true);
  preview.generation = view.getUint32(PREVIEW_AT + 16, true);
  preview.playing = view.getUint8(PREVIEW_AT + 20) === 1;
}

export function readWaveform(frame: ArrayBuffer): WaveformFrame {
  const view = new DataView(frame);
  const count = view.getUint32(12, true);
  if (frame.byteLength < WAVEFORM_HEADER_LENGTH + BIN_BYTES * count)
    throw new Error('A waveform frame was cut short.');
  return {
    deck: view.getUint8(1) === 1 ? 1 : 0,
    generation: view.getUint32(4, true),
    binsPerSecond: view.getUint32(8, true),
    bins: new Uint8Array(frame, WAVEFORM_HEADER_LENGTH, BIN_BYTES * count),
  };
}

/**
 * Reads the overviews of the tracks a request named, in the order it named them. A track
 * without an overview yet reads as null.
 */
export function readOverviews(body: ArrayBuffer): (Uint8Array | null)[] {
  const view = new DataView(body);
  if (body.byteLength < OVERVIEWS_HEADER_LENGTH || view.getUint8(0) !== OVERVIEWS)
    throw new Error('An overview body has the wrong header.');
  const columns = view.getUint32(4, true);
  const tracks = view.getUint32(8, true);
  const entry = 1 + BIN_BYTES * columns;
  if (body.byteLength < OVERVIEWS_HEADER_LENGTH + tracks * entry)
    throw new Error('An overview body was cut short.');
  const overviews: (Uint8Array | null)[] = [];
  for (let index = 0; index < tracks; index++) {
    const at = OVERVIEWS_HEADER_LENGTH + index * entry;
    overviews.push(
      view.getUint8(at) === 1 ? new Uint8Array(body, at + 1, BIN_BYTES * columns) : null,
    );
  }
  return overviews;
}

/** The difference between two readings of a count that wraps at 2^32. */
export function countSince(later: number, earlier: number): number {
  return (later - earlier + 2 ** 32) % 2 ** 32;
}
