<script lang="ts">
  import { onMount } from 'svelte';

  import type { TrackBands } from '../wave/motion';
  import { drawWaveThumb, type Rgb } from '../wave/renderer';
  import { onThemeChange } from './theme';

  interface Props {
    size?: 'sm' | 'md' | 'lg';
    src?: string | null;
    alt?: string;
    seed: number;
    bands?: TrackBands;
  }

  let { size = 'md', src = null, alt = '', seed, bands }: Props = $props();

  let frame: HTMLSpanElement;
  let canvas: HTMLCanvasElement | undefined = $state();
  let mounted = $state(false);
  let lastCacheKey = $state('');
  // The source that failed to load. The dot wave stands in until the source changes.
  let failedSrc = $state<string | null>(null);
  const showImage = $derived(Boolean(src) && src !== failedSrc);

  const pixelDimension = $derived(size === 'sm' ? 36 : size === 'lg' ? 96 : 60);

  function parseRgb(value: string): Rgb {
    const [r = 0, g = 0, b = 0] = (value.match(/[\d.]+/g) ?? []).map(Number);
    return [r / 255, g / 255, b / 255];
  }

  function paint() {
    if (!canvas || !frame) return;
    const dpr = Math.min(devicePixelRatio || 1, 2);
    const dim = Math.max(1, Math.round(pixelDimension * dpr));
    const computed = getComputedStyle(frame);
    lastCacheKey = drawWaveThumb(canvas, {
      seed,
      bands,
      width: dim,
      height: dim,
      dpr,
      background: parseRgb(computed.backgroundColor),
      foreground: parseRgb(computed.color),
    });
  }

  $effect(() => {
    if (!mounted || showImage || !canvas) return;
    void seed;
    void bands;
    void size;
    paint();
  });

  onMount(() => {
    mounted = true;
    return onThemeChange(paint);
  });
</script>

<span
  bind:this={frame}
  class={['thumb', `size-${size}`]}
  role={alt ? 'img' : undefined}
  aria-label={alt || undefined}
  aria-hidden={alt ? undefined : 'true'}
  data-seed={seed}
  data-cache-key={lastCacheKey || undefined}
  data-artwork={showImage ? 'image' : 'wave'}
>
  {#if showImage}
    <img class="image" {src} alt="" onerror={() => (failedSrc = src)} />
  {:else}
    <canvas bind:this={canvas} class="canvas" aria-hidden="true"></canvas>
  {/if}
</span>

<style>
  .thumb {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    overflow: hidden;
    width: var(--thumb-md);
    height: var(--thumb-md);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-2);
    color: var(--color-text);
    forced-color-adjust: none;
  }

  .size-sm {
    width: var(--thumb-sm);
    height: var(--thumb-sm);
    border-radius: var(--radius-sm);
  }

  .size-lg {
    width: var(--thumb-lg);
    height: var(--thumb-lg);
    border-radius: var(--radius-lg);
  }

  .image,
  .canvas {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
