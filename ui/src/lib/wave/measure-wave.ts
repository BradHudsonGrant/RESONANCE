import { type Listening, presetSettings, WaveMotion } from './motion';
import { WaveRenderer } from './renderer';

export interface WaveMeasurement {
  frames: number;
  width: number;
  height: number;
  points: string;
  medianMs: number;
  p99Ms: number;
  longestMs: number;
}

function percentile(sorted: Float64Array, fraction: number): number {
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * fraction))] ?? 0;
}

/**
 * Times the wave's own work on the UI thread for each frame: moving the surface and issuing the
 * draw, with music playing so every reactive path runs. The canvas is the size the page gives it.
 */
export async function measureWave(
  frames = 600,
  width = 960,
  height = 240,
): Promise<WaveMeasurement> {
  const canvas = document.createElement('canvas');
  canvas.style.cssText = `position:fixed;left:0;top:0;width:${width}px;height:${height}px`;
  document.body.append(canvas);
  const renderer = WaveRenderer.on(canvas);
  if (!renderer) throw new Error('This browser has no WebGL2.');
  const settings = presetSettings('lively');
  const motion = new WaveMotion(settings);
  const dpr = Math.min(devicePixelRatio || 1, 2);
  renderer.resize(Math.round(width * dpr), Math.round(height * dpr), dpr);
  renderer.useDensity(settings.density);

  const listening: Listening = {
    low: 0.6,
    mid: 0.4,
    high: 0.3,
    tone: 0.55,
    spread: 0.2,
    beatSeconds: 0.5,
    onBeat: false,
  };
  const costs = new Float64Array(frames);
  for (let frame = 0; frame < frames; frame++) {
    await new Promise(requestAnimationFrame);
    listening.onBeat = frame % 30 === 0;
    listening.low = 0.4 + 0.3 * Math.sin(frame / 40);
    const start = performance.now();
    motion.step(1 / 60, listening);
    renderer.draw(motion.surface, motion.settings);
    costs[frame] = performance.now() - start;
  }
  renderer.release();
  canvas.remove();

  const sorted = costs.slice().sort();
  return {
    frames,
    width,
    height,
    points: settings.density,
    medianMs: percentile(sorted, 0.5),
    p99Ms: percentile(sorted, 0.99),
    longestMs: sorted[sorted.length - 1] ?? 0,
  };
}
