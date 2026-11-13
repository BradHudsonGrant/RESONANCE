<script lang="ts">
  import { onMount } from 'svelte';

  import { type Listening, type Preset, presetSettings, WaveMotion } from './motion';
  import { WaveRenderer } from './renderer';

  interface Props {
    preset?: Preset;
    /** What the music is doing, read once a frame. Without it the surface drifts on its own. */
    listening?: () => Listening | null;
  }

  let { preset = 'balanced', listening }: Props = $props();

  let canvas: HTMLCanvasElement;
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const motion = new WaveMotion(presetSettings('balanced'));

  $effect(() => {
    motion.settings = presetSettings(reducedMotion.matches ? 'still' : preset);
  });

  /** The rgb() a colour token resolves to on this element, as 0 to 1 channels. */
  function tone(name: string): [number, number, number] {
    const probe = getComputedStyle(canvas).getPropertyValue(name);
    canvas.style.color = probe;
    const [r = 0, g = 0, b = 0] = (getComputedStyle(canvas).color.match(/[\d.]+/g) ?? []).map(
      Number,
    );
    canvas.style.color = '';
    return [r / 255, g / 255, b / 255];
  }

  onMount(() => {
    const renderer = WaveRenderer.on(canvas);
    if (!renderer) return;

    const readColours = () => {
      renderer.background = tone('--color-background');
      renderer.foreground = tone('--color-text');
    };
    const resize = () => {
      const dpr = Math.min(devicePixelRatio || 1, 2);
      renderer.resize(
        Math.max(1, Math.round(canvas.clientWidth * dpr)),
        Math.max(1, Math.round(canvas.clientHeight * dpr)),
        dpr,
      );
    };
    const onReducedMotion = () => {
      motion.settings = presetSettings(reducedMotion.matches ? 'still' : preset);
    };

    let visible = true;
    let lastFrame = performance.now();
    let request = 0;
    const frame = (now: number) => {
      request = requestAnimationFrame(frame);
      if (!visible || document.hidden) {
        lastFrame = now;
        return;
      }
      const elapsed = now - lastFrame;
      const cap = motion.settings.frameCap;
      if (cap && elapsed < 1000 / cap - 1) return;
      lastFrame = now;
      motion.step(Math.min(elapsed / 1000, 0.1), listening?.() ?? null);
      renderer.useDensity(motion.settings.density);
      renderer.draw(motion.surface, motion.settings);
    };

    const scheme = matchMedia('(prefers-color-scheme: dark)');
    const seen = new IntersectionObserver(([entry]) => (visible = entry?.isIntersecting ?? true));
    const sized = new ResizeObserver(resize);
    seen.observe(canvas);
    sized.observe(canvas);
    scheme.addEventListener('change', readColours);
    reducedMotion.addEventListener('change', onReducedMotion);
    readColours();
    resize();
    request = requestAnimationFrame(frame);

    return () => {
      cancelAnimationFrame(request);
      seen.disconnect();
      sized.disconnect();
      scheme.removeEventListener('change', readColours);
      reducedMotion.removeEventListener('change', onReducedMotion);
      renderer.release();
    };
  });
</script>

<canvas bind:this={canvas} class="wave" aria-hidden="true"></canvas>

<style>
  .wave {
    display: block;
    width: 100%;
    height: 100%;
  }
</style>
