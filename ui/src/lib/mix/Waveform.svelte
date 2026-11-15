<script lang="ts">
  import { onMount } from 'svelte';

  import { onThemeChange } from '../design/theme';

  interface Props {
    mode: 'inline' | 'crossfade' | 'curve';
    peaks: Float32Array | readonly number[];
    secondaryPeaks?: Float32Array | readonly number[] | null;
    durationSec: number;
    progress?: number;
    mixPoint?: number;
    playing?: boolean;
    label?: string;
  }

  let {
    mode,
    peaks,
    secondaryPeaks = null,
    durationSec,
    progress = 0,
    mixPoint = 0.6,
    playing = false,
    label,
  }: Props = $props();

  let stage: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let mounted = $state(false);

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

  function formatTimestamp(seconds: number): string {
    const whole = Math.max(0, Math.round(seconds));
    const mins = Math.floor(whole / 60);
    const secs = whole % 60;
    return `${mins}:${String(secs).padStart(2, '0')}`;
  }

  const curveTimestamps = $derived(
    [0, 0.25, 0.5, 0.75, 1].map((fraction) => formatTimestamp(durationSec * fraction)),
  );

  function resolveColor(token: string): string {
    canvas.style.forcedColorAdjust = 'none';
    canvas.style.color = `var(${token})`;
    const computed = getComputedStyle(canvas).color;
    canvas.style.color = '';
    canvas.style.forcedColorAdjust = '';
    return computed;
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
    if (peaks.length === 0) return;

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

    if (mode === 'crossfade') {
      for (let i = 0; i < count; i++) {
        const x = i * stepPx;
        const t = i / Math.max(1, count - 1);
        const envA = 1 / (1 + Math.exp((t - mixPoint) * 10));
        const envB = 1 - envA;
        const ampA = samplePeak(peaks, i, count) * envA;
        const ampB = samplePeak(secondaryPeaks, i, count) * envB;

        const hA = Math.max(barPx, Math.round(ampA * (midY - 2 * barPx)));
        const hB = ampB > 0 ? Math.max(barPx, Math.round(ampB * (midY - 2 * barPx))) : 0;

        ctx.fillStyle = t <= progress ? textColor : text2Color;
        ctx.fillRect(x, midY - hA, barPx, hA);
        if (hB > 0) {
          ctx.fillStyle = text2Color;
          ctx.fillRect(x, midY, barPx, hB);
        }
      }

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

    for (let i = 0; i < count; i++) {
      const x = i * stepPx;
      const t = i / Math.max(1, count - 1);
      const amp = samplePeak(peaks, i, count);
      const halfH = Math.max(barPx, Math.round(amp * (midY - barPx)));
      ctx.fillStyle = t <= progress ? textColor : text2Color;
      ctx.fillRect(x, midY - halfH, barPx, halfH * 2);
    }
  }

  $effect(() => {
    if (!mounted) return;
    void mode;
    void peaks;
    void secondaryPeaks;
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
    const stopThemes = onThemeChange(paint);
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
