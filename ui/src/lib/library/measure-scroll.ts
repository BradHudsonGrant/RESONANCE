/**
 * Scrolls the track table as fast as a dragged scrollbar would and records every frame, for the
 * budget in docs/architecture.md. The dev build puts it on `window`, so the same run can be
 * repeated by hand in a browser or from a test driver.
 */

export interface FrameTimes {
  frames: number;
  p50Ms: number;
  p99Ms: number;
  maxMs: number;
  /** Frames that took longer than one and a half display frames, so at least one was missed. */
  missedFrames: number;
  /** Frames in which a row in view was still waiting for its page. */
  framesWithBlankRows: number;
}

export interface ScrollMeasurement {
  rows: number;
  /** The display's frame interval, measured while nothing moved. */
  refreshMs: number;
  /** A hard fling on a touchpad: 10,000 px a second, about 330 rows, from the top down. */
  fling: FrameTimes;
  /** A dragged scrollbar: a jump to a random place every frame. */
  jumps: FrameTimes;
  /** From the last jump to every row in view being filled. */
  filledAfterJumpsMs: number;
}

function nextFrame(): Promise<number> {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function percentile(sorted: number[], share: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))] ?? 0;
}

async function run(
  grid: HTMLElement,
  frames: number,
  refreshMs: number,
  step: (frame: number, elapsedMs: number) => void,
): Promise<FrameTimes> {
  const gaps: number[] = [];
  let framesWithBlankRows = 0;
  const start = await nextFrame();
  let last = start;
  for (let frame = 0; frame < frames; frame += 1) {
    step(frame, last - start);
    const now = await nextFrame();
    if (grid.querySelector('[aria-label="Loading"]')) framesWithBlankRows += 1;
    gaps.push(now - last);
    last = now;
  }
  const sorted = [...gaps].sort((a, b) => a - b);
  return {
    frames,
    p50Ms: percentile(sorted, 0.5),
    p99Ms: percentile(sorted, 0.99),
    maxMs: sorted.at(-1) ?? 0,
    missedFrames: gaps.filter((gap) => gap > refreshMs * 1.5).length,
    framesWithBlankRows,
  };
}

export async function measureTableScroll(frames = 600): Promise<ScrollMeasurement> {
  const grid = document.querySelector<HTMLElement>('[role="grid"][aria-label="Tracks"]');
  const viewport = grid?.firstElementChild;
  if (!grid || !(viewport instanceof HTMLElement)) throw new Error('The track table is not open.');

  viewport.scrollTop = 0;
  const idle = await run(grid, 60, Infinity, () => {});
  const refreshMs = idle.p50Ms;
  const range = viewport.scrollHeight - viewport.clientHeight;

  const fling = await run(grid, frames, refreshMs, (_, elapsedMs) => {
    viewport.scrollTop = Math.min(range, (elapsedMs / 1000) * 10_000);
  });
  const jumps = await run(grid, frames, refreshMs, () => {
    viewport.scrollTop = Math.random() * range;
  });
  const stopped = performance.now();
  while (grid.querySelector('[aria-label="Loading"]')) await nextFrame();
  const filledAfterJumpsMs = performance.now() - stopped;

  return {
    rows: Number(grid.getAttribute('aria-rowcount')) - 1,
    refreshMs,
    fling,
    jumps,
    filledAfterJumpsMs,
  };
}
