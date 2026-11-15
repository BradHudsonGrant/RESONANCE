<script lang="ts">
  import { onMount } from 'svelte';

  import { onThemeChange } from '../design/theme';
  import {
    type Listening,
    type Preset,
    presetSettings,
    type TrackBands,
    WaveMotion,
    type WaveSettings,
  } from './motion';
  import { drawWaveThumb, type Rgb, WaveRenderer } from './renderer';

  interface Props {
    variant?: 'ambient' | 'sidebar' | 'card' | 'thumb';
    seed?: number;
    bands?: TrackBands;
    paused?: boolean;
    preset?: Preset;
    /** What the music is doing, read once a frame. Without it the surface drifts on its own. */
    listening?: () => Listening | null;
  }

  let {
    variant = 'ambient',
    seed = 1,
    bands,
    paused = false,
    preset = 'balanced',
    listening,
  }: Props = $props();

  let canvas: HTMLCanvasElement;
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const motion = new WaveMotion(presetSettings('balanced'));

  /** The sidebar's field is wide and short, so the camera sits lower and closer to fill it. */
  function settings(): WaveSettings {
    const base = presetSettings(reducedMotion.matches || paused ? 'still' : preset);
    if (variant !== 'sidebar') return base;
    return { ...base, density: 'low', pointSize: 0.35, zoom: 2.3, elevation: 0.3 };
  }

  $effect(() => {
    motion.settings = settings();
  });

  /** The rgb() a colour token resolves to on this element, as 0 to 1 channels. */
  function tone(name: string): Rgb {
    canvas.style.forcedColorAdjust = 'none';
    canvas.style.color = `var(${name})`;
    const [r = 0, g = 0, b = 0] = (getComputedStyle(canvas).color.match(/[\d.]+/g) ?? []).map(
      Number,
    );
    canvas.style.color = '';
    canvas.style.forcedColorAdjust = '';
    return [r / 255, g / 255, b / 255];
  }

  function backgroundToken(): string {
    if (variant === 'sidebar' || variant === 'card') return '--color-surface';
    if (variant === 'thumb') return '--color-surface-2';
    return '--color-background';
  }

  onMount(() => {
    const isCachedStatic = variant === 'thumb' || variant === 'card';
    let cssWidth = 0;
    let cssHeight = 0;

    if (isCachedStatic) {
      const paintStatic = () => {
        const dpr = Math.min(devicePixelRatio || 1, 2);
        const width = Math.max(1, Math.round((cssWidth || canvas.clientWidth || 60) * dpr));
        const height = Math.max(1, Math.round((cssHeight || canvas.clientHeight || 60) * dpr));
        drawWaveThumb(canvas, {
          seed,
          bands,
          width,
          height,
          dpr,
          background: tone(backgroundToken()),
          foreground: tone('--color-text'),
        });
      };

      const sized = new ResizeObserver(([entry]) => {
        if (entry) {
          cssWidth = entry.contentRect.width;
          cssHeight = entry.contentRect.height;
        }
        paintStatic();
      });
      sized.observe(canvas);
      const stopThemes = onThemeChange(paintStatic);

      return () => {
        sized.disconnect();
        stopThemes();
      };
    }

    let disposed = false;
    let renderer: WaveRenderer | null = null;
    let visible = true;
    let lastFrame = performance.now();
    let request = 0;

    const readColours = () => {
      if (!renderer) return;
      renderer.background = tone(backgroundToken());
      renderer.foreground = tone('--color-text');
    };
    const drawOnce = () => {
      if (!renderer) return;
      renderer.useDensity(motion.settings.density);
      renderer.draw(motion.surface, motion.settings);
    };
    const resize = () => {
      if (!renderer) return;
      const dpr = Math.min(devicePixelRatio || 1, 2);
      const width = cssWidth || canvas.clientWidth || 1;
      const height = cssHeight || canvas.clientHeight || 1;
      renderer.resize(
        Math.max(1, Math.round(width * dpr)),
        Math.max(1, Math.round(height * dpr)),
        dpr,
      );
      drawOnce();
    };
    const onReducedMotion = () => {
      motion.settings = settings();
      drawOnce();
    };

    const frame = (now: number) => {
      request = requestAnimationFrame(frame);
      if (!visible || document.hidden || reducedMotion.matches || paused) {
        lastFrame = now;
        return;
      }
      const elapsed = now - lastFrame;
      const cap = motion.settings.frameCap;
      if (cap && elapsed < 1000 / cap - 1) return;
      lastFrame = now;
      motion.step(Math.min(elapsed / 1000, 0.1), listening?.() ?? null);
      drawOnce();
    };

    const seen = new IntersectionObserver(([entry]) => (visible = entry?.isIntersecting ?? true));
    const sized = new ResizeObserver(([entry]) => {
      if (entry) {
        cssWidth = entry.contentRect.width;
        cssHeight = entry.contentRect.height;
      }
      resize();
    });
    seen.observe(canvas);
    sized.observe(canvas);
    const stopThemes = onThemeChange(() => {
      readColours();
      drawOnce();
    });
    reducedMotion.addEventListener('change', onReducedMotion);

    void WaveRenderer.prepare(canvas, () => disposed).then((ready) => {
      if (!ready || disposed) {
        ready?.release();
        return;
      }
      renderer = ready;
      readColours();
      resize();
      lastFrame = performance.now();
      request = requestAnimationFrame(frame);
    });

    return () => {
      disposed = true;
      cancelAnimationFrame(request);
      seen.disconnect();
      sized.disconnect();
      stopThemes();
      reducedMotion.removeEventListener('change', onReducedMotion);
      renderer?.release();
    };
  });
</script>

<canvas
  bind:this={canvas}
  class={['wave', variant]}
  data-variant={variant}
  data-seed={seed}
  aria-hidden="true"
></canvas>

<style>
  .wave {
    display: block;
    width: 100%;
    height: 100%;
  }

  .sidebar {
    height: var(--sidebar-wave-height);
    mask-image: linear-gradient(to bottom, transparent, black 28%);
  }
</style>
