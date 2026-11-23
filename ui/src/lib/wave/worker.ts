import { WaveMotion, type WaveSettings } from './motion';
import { type Rgb, WaveRenderer } from './renderer';

export type WaveWorkerMessage =
  | {
      kind: 'init';
      canvas: OffscreenCanvas;
      settings: WaveSettings;
      background: Rgb;
      foreground: Rgb;
      width: number;
      height: number;
      dpr: number;
      paused: boolean;
    }
  | { kind: 'resize'; width: number; height: number; dpr: number }
  | { kind: 'theme'; background: Rgb; foreground: Rgb }
  | { kind: 'settings'; settings: WaveSettings; paused: boolean }
  | { kind: 'visible'; visible: boolean };

let renderer: WaveRenderer | null = null;
let motion: WaveMotion | null = null;
let visible = true;
let paused = false;
let lastFrame = 0;
let frameHandle = 0;

function schedule(callback: (now: number) => void): number {
  if (typeof requestAnimationFrame === 'function') {
    return requestAnimationFrame(callback);
  }
  return self.setTimeout(() => callback(performance.now()), 16);
}

function drawOnce() {
  if (!renderer || !motion) return;
  renderer.useDensity(motion.settings.density);
  renderer.draw(motion.surface, motion.settings);
}

function frame(now: number) {
  frameHandle = schedule(frame);
  if (!visible || paused || !motion || motion.settings.drift === 0) {
    lastFrame = now;
    return;
  }
  const elapsed = now - lastFrame;
  const cap = motion.settings.frameCap;
  if (cap && elapsed < 1000 / cap - 1) return;
  lastFrame = now;
  motion.step(Math.min(elapsed / 1000, 0.1), null);
  drawOnce();
}

self.addEventListener('message', (event: MessageEvent<WaveWorkerMessage>) => {
  const message = event.data;
  switch (message.kind) {
    case 'init': {
      motion = new WaveMotion(message.settings);
      paused = message.paused;
      renderer = WaveRenderer.on(message.canvas);
      if (!renderer) return;
      renderer.background = message.background;
      renderer.foreground = message.foreground;
      renderer.resize(message.width, message.height, message.dpr);
      drawOnce();
      lastFrame = performance.now();
      if (!frameHandle) frameHandle = schedule(frame);
      break;
    }
    case 'resize': {
      if (!renderer) return;
      renderer.resize(message.width, message.height, message.dpr);
      drawOnce();
      break;
    }
    case 'theme': {
      if (!renderer) return;
      renderer.background = message.background;
      renderer.foreground = message.foreground;
      drawOnce();
      break;
    }
    case 'settings': {
      paused = message.paused;
      if (!motion) return;
      motion.settings = message.settings;
      drawOnce();
      break;
    }
    case 'visible': {
      visible = message.visible;
      break;
    }
  }
});
