<script lang="ts">
  import { onMount } from 'svelte';

  import {
    type Listening,
    type Preset,
    presetSettings,
    type TrackBands,
    WaveMotion,
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

  $effect(() => {
    motion.settings = presetSettings(reducedMotion.matches || paused ? 'still' : preset);
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

    if (isCachedStatic) {
      const paintStatic = () => {
        const dpr = Math.min(devicePixelRatio || 1, 2);
        const width = Math.max(1, Math.round((canvas.clientWidth || 60) * dpr));
        const height = Math.max(1, Math.round((canvas.clientHeight || 60) * dpr));
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

      const scheme = matchMedia('(prefers-color-scheme: dark)');
      const forced = matchMedia('(forced-colors: active)');
      const sized = new ResizeObserver(paintStatic);
      sized.observe(canvas);
      scheme.addEventListener('change', paintStatic);
      forced.addEventListener('change', paintStatic);
      paintStatic();

      return () => {
        sized.disconnect();
        scheme.removeEventListener('change', paintStatic);
        forced.removeEventListener('change', paintStatic);
      };
    }

    const renderer = WaveRenderer.on(canvas);
    if (!renderer) return;

    const readColours = () => {
      renderer.background = tone(backgroundToken());
      renderer.foreground = tone('--color-text');
    };
    const drawOnce = () => {
      renderer.useDensity(motion.settings.density);
      renderer.draw(motion.surface, motion.settings);
    };
    const resize = () => {
      const dpr = Math.min(devicePixelRatio || 1, 2);
      renderer.resize(
        Math.max(1, Math.round(canvas.clientWidth * dpr)),
        Math.max(1, Math.round(canvas.clientHeight * dpr)),
        dpr,
      );
      drawOnce();
    };
    const onReducedMotion = () => {
      motion.settings = presetSettings(reducedMotion.matches || paused ? 'still' : preset);
      drawOnce();
    };

    let visible = true;
    let lastFrame = performance.now();
    let request = 0;
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

    const onScheme = () => {
      readColours();
      drawOnce();
    };
    const scheme = matchMedia('(prefers-color-scheme: dark)');
    const forced = matchMedia('(forced-colors: active)');
    const seen = new IntersectionObserver(([entry]) => (visible = entry?.isIntersecting ?? true));
    const sized = new ResizeObserver(resize);
    seen.observe(canvas);
    sized.observe(canvas);
    scheme.addEventListener('change', onScheme);
    forced.addEventListener('change', onScheme);
    reducedMotion.addEventListener('change', onReducedMotion);
    readColours();
    resize();
    request = requestAnimationFrame(frame);

    return () => {
      cancelAnimationFrame(request);
      seen.disconnect();
      sized.disconnect();
      scheme.removeEventListener('change', onScheme);
      forced.removeEventListener('change', onScheme);
      reducedMotion.removeEventListener('change', onReducedMotion);
      renderer.release();
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
