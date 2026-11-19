/**
 * Times auto-preview from a key press in the track table to the first frame of the new track
 * reaching the output, for the budget in docs/architecture.md. The dev build puts it on
 * `window`. Auto-preview must be on.
 *
 * Each press moves one row and waits until the core's telemetry shows the new load playing.
 * The frame reports the position being heard, so the first frame went out `position - start`
 * before the reading arrived, and that is taken off. The reading's trip over the socket is
 * counted, so the result is an upper bound.
 *
 * The first pass moves down over `swaps` rows to read the files into the system cache. The
 * measured pass moves back up over the same rows.
 */

import { core } from '../ipc/core';
import { emptyTelemetry, frameKind, readTelemetry, TELEMETRY } from '../mix/frames';

export interface AutoPreviewMeasurement {
  swaps: number;
  p50Ms: number;
  p95Ms: number;
  maxMs: number;
  /** Every swap in the measured pass, in order. */
  swapsMs: number[];
}

const SWAP_TIMEOUT_MS = 3_000;
const PROVEN_HEARD_SECONDS = 0.05;

function percentile(sorted: number[], share: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))] ?? 0;
}

export async function measureAutoPreview(swaps = 20): Promise<AutoPreviewMeasurement> {
  const grid = document.querySelector<HTMLElement>('[role="grid"][aria-label="Tracks"]');
  if (!grid) throw new Error('The track table is not on the page.');
  if (!('onFrame' in core)) throw new Error('This core sends no telemetry.');

  // The core's answer to each focus holds where that load starts, before any of it has played.
  const startsAt = new Map<number, number>();
  const call = core.call;
  core.call = (async (...args: Parameters<typeof call>) => {
    const result = await call.apply(core, args);
    if (args[0] === 'preview_focus') {
      const state = result as { generation: number; position_ms: number };
      if (!startsAt.has(state.generation)) {
        startsAt.set(state.generation, state.position_ms / 1000);
      }
    }
    return result;
  }) as typeof call;

  const telemetry = emptyTelemetry();
  let waiter: ((at: number) => void) | null = null;
  let lastGeneration = 0;
  const stop = core.onFrame((frame) => {
    if (frameKind(frame) !== TELEMETRY) return;
    const at = performance.now();
    readTelemetry(frame, telemetry);
    const preview = telemetry.preview;
    if (preview.generation === lastGeneration || !preview.playing) return;
    const startSeconds = startsAt.get(preview.generation);
    if (startSeconds === undefined) {
      if (waiter === null) lastGeneration = preview.generation;
      return;
    }
    const heardSeconds = preview.positionSeconds - startSeconds;
    // Between callbacks the position runs on from the last one even before the new track has
    // samples, by at most one device period. Past this the frames are known to have played.
    if (heardSeconds < PROVEN_HEARD_SECONDS) return;
    lastGeneration = preview.generation;
    waiter?.(at - heardSeconds * 1000);
  });

  async function swap(key: 'ArrowDown' | 'ArrowUp'): Promise<number> {
    const heard = new Promise<number>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('No new track was heard.')), SWAP_TIMEOUT_MS);
      waiter = (at) => {
        clearTimeout(timer);
        resolve(at);
      };
    });
    const pressed = performance.now();
    grid?.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    const at = await heard;
    waiter = null;
    // Let the swap settle so the next press starts from a playing track, as a DJ listening would.
    await new Promise((resolve) => setTimeout(resolve, 250));
    return at - pressed;
  }

  try {
    grid.focus();
    // A few readings first, so the track already playing is not taken for the first swap.
    await new Promise((resolve) => setTimeout(resolve, 100));
    for (let step = 0; step < swaps; step += 1) await swap('ArrowDown');
    const swapsMs: number[] = [];
    for (let step = 0; step < swaps; step += 1) swapsMs.push(await swap('ArrowUp'));
    const sorted = [...swapsMs].sort((a, b) => a - b);
    return {
      swaps,
      p50Ms: percentile(sorted, 0.5),
      p95Ms: percentile(sorted, 0.95),
      maxMs: sorted[sorted.length - 1] ?? 0,
      swapsMs,
    };
  } finally {
    stop();
    core.call = call;
  }
}
