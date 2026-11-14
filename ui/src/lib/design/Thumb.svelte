<script lang="ts">
  import { onMount } from 'svelte';

  import type { TrackBands } from '../wave/motion';
  import { drawWaveThumb, type Rgb } from '../wave/renderer';

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

  const pixelDimension = $derived(size === 'sm' ? 36 : size === 'lg' ? 96 : 60);

  function tone(element: HTMLElement, name: string): Rgb {
    element.style.forcedColorAdjust = 'none';
    element.style.color = `var(${name})`;
    const [r = 0, g = 0, b = 0] = (getComputedStyle(element).color.match(/[\d.]+/g) ?? []).map(
      Number,
    );
    element.style.color = '';
    element.style.forcedColorAdjust = '';
    return [r / 255, g / 255, b / 255];
  }

  function paint() {
    if (!canvas || !frame) return;
    const dpr = Math.min(devicePixelRatio || 1, 2);
    const dim = Math.max(1, Math.round((frame.clientWidth || pixelDimension) * dpr));
    lastCacheKey = drawWaveThumb(canvas, {
      seed,
      bands,
      width: dim,
      height: dim,
      dpr,
      background: tone(canvas, '--color-surface-2'),
      foreground: tone(canvas, '--color-text'),
    });
  }

  $effect(() => {
    if (!mounted || src || !canvas) return;
    void seed;
    void bands;
    void size;
    paint();
  });

  onMount(() => {
    mounted = true;
    const scheme = matchMedia('(prefers-color-scheme: dark)');
    const forced = matchMedia('(forced-colors: active)');
    scheme.addEventListener('change', paint);
    forced.addEventListener('change', paint);
    paint();
    return () => {
      scheme.removeEventListener('change', paint);
      forced.removeEventListener('change', paint);
    };
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
>
  {#if src}
    <img class="image" {src} {alt} />
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
