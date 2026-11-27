<script lang="ts">
  import { onMount } from 'svelte';

  import { onThemeChange } from '../design/theme';
  import { BIN_BYTES } from './frames';

  interface Props {
    mode: 'inline' | 'crossfade' | 'curve';
    /** The energy curve, one level from 0 to 1 a bar. Only `curve` reads it. */
    peaks?: Float32Array | readonly number[];
    /**
     * A waveform overview for `inline` and `crossfade`: four bytes a column, the peak and the low,
     * mid and high band levels, 255 at full scale.
     */
    columns?: Uint8Array | null;
    /** Deck B's overview in `crossfade`. */
    secondaryColumns?: Uint8Array | null;
    durationSec: number;
    progress?: number;
    mixPoint?: number;
    playing?: boolean;
    label?: string;
  }

  let {
    mode,
    peaks = [],
    columns = null,
    secondaryColumns = null,
    durationSec,
    progress = 0,
    mixPoint = 0.6,
    playing = false,
    label,
  }: Props = $props();

  let stage: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let mounted = $state(false);

  /**
   * How bright each layer of a bar is drawn, outside in: the peak, then the high, mid and low band
   * levels. The lower the band, the brighter, so kicks and bass read as the solid core of a bar
   * and hats as a faint fringe, with no colour.
   */
  const LAYER_ALPHA = [0.3, 0.5, 0.72, 1] as const;
  const LAYERS = LAYER_ALPHA.length;

  function samplePeak(
    source: Float32Array | readonly number[] | null,
    index: number,
    total: number,
  ): number {
    if (!source || source.length === 0) return 0;
    const at = Math.min(
      source.length - 1,
      Math.floor((index / Math.max(1, total)) * source.length),
    );
    return Math.min(1, Math.max(0.05, Math.abs(source[at] ?? 0)));
  }

  /**
   * The loudest of each layer over the columns under each of `bars` bars, from 0 to 1, four
   * levels a bar in layer order. Bytes are peak, low, mid, high; layers go peak, high, mid, low.
   */
  function sampleLayers(source: Uint8Array | null, bars: number): Float32Array {
    const levels = new Float32Array(bars * LAYERS);
    const count = source ? Math.floor(source.length / BIN_BYTES) : 0;
    if (!source || count === 0) return levels;
    for (let bar = 0; bar < bars; bar++) {
      const from = Math.floor((bar * count) / bars);
      const to = Math.min(count, Math.max(from + 1, Math.floor(((bar + 1) * count) / bars)));
      const out = bar * LAYERS;
      for (let column = from; column < to; column++) {
        const at = column * BIN_BYTES;
        levels[out] = Math.max(levels[out]!, source[at]! / 255);
        levels[out + 1] = Math.max(levels[out + 1]!, source[at + 3]! / 255);
        levels[out + 2] = Math.max(levels[out + 2]!, source[at + 2]! / 255);
        levels[out + 3] = Math.max(levels[out + 3]!, source[at + 1]! / 255);
      }
    }
    return levels;
  }

  function formatTimestamp(seconds: number): string {
    const whole = Math.max(0, Math.round(seconds));
    const mins = Math.floor(whole / 60);
    const secs = whole % 60;
    return `${mins}:${String(secs).padStart(2, '0')}`;
  }

  const curveTimestamps = $derived(
    [0, 0.25, 0.5, 0.75, 1].map((fraction) => formatTimestamp(durationSec * fraction)),
  );

  // Rows repaint as the table scrolls, so each waveform reads its colours once per theme.
  let colours: Record<string, string> = {};

  function resolveColor(token: string): string {
    const known = colours[token];
    if (known) return known;
    canvas.style.forcedColorAdjust = 'none';
    canvas.style.color = `var(${token})`;
    const computed = getComputedStyle(canvas).color;
    canvas.style.color = '';
    canvas.style.forcedColorAdjust = '';
    colours[token] = computed;
    return computed;
  }

  /**
   * Draws bars `from` to `to` in one tone, a layer at a time so the canvas changes state eight
   * times a paint rather than once a bar. Heights are whole device pixels, out from `y`.
   */
  function drawBars(
    ctx: CanvasRenderingContext2D,
    levels: Float32Array,
    from: number,
    to: number,
    stepPx: number,
    y: number,
    reach: (bar: number) => number,
    barPx: number,
    direction: 'both' | 'up' | 'down',
  ): void {
    for (let layer = 0; layer < LAYERS; layer++) {
      ctx.globalAlpha = LAYER_ALPHA[layer]!;
      for (let bar = from; bar < to; bar++) {
        const level = levels[bar * LAYERS + layer]!;
        const span = reach(bar);
        if (level === 0 || span <= 0) continue;
        const h = Math.max(barPx, Math.round(level * span));
        const x = bar * stepPx;
        if (direction === 'both') ctx.fillRect(x, y - h, barPx, h * 2);
        else if (direction === 'up') ctx.fillRect(x, y - h, barPx, h);
        else ctx.fillRect(x, y, barPx, h);
      }
    }
    ctx.globalAlpha = 1;
  }

  function paint() {
    if (!canvas || !stage) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = Math.max(1, devicePixelRatio || 1);
    const defaultWidth = mode === 'inline' ? 140 : mode === 'curve' ? 385 : 420;
    const defaultHeight = mode === 'inline' ? 17 : mode === 'curve' ? 67 : 101;
    const cssWidth = stage.clientWidth || defaultWidth;
    const cssHeight = stage.clientHeight || defaultHeight;
    const width = Math.max(1, Math.round(cssWidth * dpr));
    const height = Math.max(1, Math.round(cssHeight * dpr));

    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }

    ctx.clearRect(0, 0, width, height);
    const empty = mode === 'curve' ? peaks.length === 0 : !columns && !secondaryColumns;
    if (empty) return;

    const textColor = resolveColor('--color-text');
    const text2Color = resolveColor('--color-text-2');
    const borderColor = resolveColor('--color-border');
    const accentColor = resolveColor('--color-accent');

    const barPx = Math.max(1, Math.round(dpr));
    const gapPx = Math.max(1, Math.round(dpr));
    const stepPx = barPx + gapPx;
    const count = Math.max(1, Math.floor(width / stepPx));

    if (mode === 'curve') {
      const baselineY = height - barPx;
      ctx.fillStyle = borderColor;
      ctx.fillRect(0, baselineY, width, barPx);

      ctx.fillStyle = textColor;
      for (let i = 0; i < count; i++) {
        const amp = samplePeak(peaks, i, count);
        const barH = Math.max(barPx, Math.round(amp * (height - 4 * barPx)));
        const x = i * stepPx;
        ctx.fillRect(x, baselineY - barH, barPx, barH);
      }
      return;
    }

    const midY = Math.floor(height / 2);
    // Bars up to and including this one have played.
    const playedTo = Math.min(count, Math.floor(progress * Math.max(1, count - 1)) + 1);

    if (mode === 'crossfade') {
      const reach = midY - 2 * barPx;
      const envA = (bar: number) =>
        1 / (1 + Math.exp((bar / Math.max(1, count - 1) - mixPoint) * 10));
      const levelsA = sampleLayers(columns, count);
      const levelsB = sampleLayers(secondaryColumns, count);
      const reachA = (bar: number) => reach * envA(bar);
      ctx.fillStyle = textColor;
      drawBars(ctx, levelsA, 0, playedTo, stepPx, midY, reachA, barPx, 'up');
      ctx.fillStyle = text2Color;
      drawBars(ctx, levelsA, playedTo, count, stepPx, midY, reachA, barPx, 'up');
      drawBars(
        ctx,
        levelsB,
        0,
        count,
        stepPx,
        midY,
        (bar) => reach * (1 - envA(bar)),
        barPx,
        'down',
      );

      const strokePx = Math.max(1, Math.round(1.5 * dpr));
      ctx.lineWidth = strokePx;
      ctx.strokeStyle = textColor;
      ctx.beginPath();
      for (let i = 0; i < count; i++) {
        const x = i * stepPx;
        const t = i / Math.max(1, count - 1);
        const envA = 1 / (1 + Math.exp((t - mixPoint) * 10));
        const y = height - strokePx - envA * (height - 6 * barPx);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      ctx.strokeStyle = text2Color;
      ctx.beginPath();
      for (let i = 0; i < count; i++) {
        const x = i * stepPx;
        const t = i / Math.max(1, count - 1);
        const envB = 1 - 1 / (1 + Math.exp((t - mixPoint) * 10));
        const y = height - strokePx - envB * (height - 6 * barPx);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();

      const playX = Math.round(Math.min(1, Math.max(0, mixPoint)) * (width - barPx));
      const capPx = Math.max(3, Math.round(5 * dpr));
      ctx.fillStyle = playing ? accentColor : textColor;
      ctx.fillRect(playX, 0, barPx, height);
      ctx.fillRect(Math.round(playX - (capPx - barPx) / 2), 0, capPx, capPx);
      return;
    }

    const levels = sampleLayers(columns, count);
    const reach = () => midY - barPx;
    ctx.fillStyle = textColor;
    drawBars(ctx, levels, 0, playedTo, stepPx, midY, reach, barPx, 'both');
    ctx.fillStyle = text2Color;
    drawBars(ctx, levels, playedTo, count, stepPx, midY, reach, barPx, 'both');
  }

  $effect(() => {
    if (!mounted) return;
    void mode;
    void peaks;
    void columns;
    void secondaryColumns;
    void durationSec;
    void progress;
    void mixPoint;
    void playing;
    paint();
  });

  onMount(() => {
    mounted = true;
    const sized = new ResizeObserver(paint);
    sized.observe(stage);
    const stopThemes = onThemeChange(() => {
      colours = {};
      paint();
    });
    paint();
    return () => {
      sized.disconnect();
      stopThemes();
    };
  });
</script>

<div class={['waveform', mode]} role="img" aria-label={label ?? `${mode} waveform`}>
  <div bind:this={stage} class="stage">
    <canvas bind:this={canvas} class="canvas" aria-hidden="true"></canvas>
  </div>
  {#if mode === 'curve'}
    <div class="timestamps numeric" aria-hidden="true">
      {#each curveTimestamps as stamp, index (`${index}-${stamp}`)}
        <span>{stamp}</span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .waveform {
    display: grid;
    gap: var(--space-1-5);
    min-width: 0;
  }

  .stage {
    position: relative;
    overflow: hidden;
    width: 100%;
  }

  .inline {
    width: var(--waveform-inline-width);
  }

  .inline .stage {
    height: var(--waveform-inline-height);
  }

  .crossfade {
    width: 100%;
  }

  .crossfade .stage {
    height: var(--dock-waveform-height);
  }

  .curve {
    width: 100%;
    max-width: var(--waveform-curve-width);
  }

  .curve .stage {
    height: var(--waveform-curve-height);
  }

  .canvas {
    position: absolute;
    inset: 0;
    display: block;
    width: 100%;
    height: 100%;
  }

  .timestamps {
    display: flex;
    justify-content: space-between;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: var(--leading-mono);
  }
</style>
