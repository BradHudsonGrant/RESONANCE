<script lang="ts">
  import { onMount } from 'svelte';

  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import { onThemeChange } from '../design/theme';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { MixDeck } from '../ipc/bindings/MixDeck';
  import type { MixDevice } from '../ipc/bindings/MixDevice';
  import type { MixState } from '../ipc/bindings/MixState';
  import { CoreError } from '../ipc/client';
  import { core } from '../ipc/core';
  import { DeckClock } from './clock';
  import Deck from './Deck.svelte';
  import {
    emptyTelemetry,
    frameKind,
    readTelemetry,
    readWaveform,
    TELEMETRY,
    WAVEFORM,
  } from './frames';
  import { mixProbe } from './probe';
  import { type LaneState, MixRenderer } from './renderer';

  interface Props {
    onplayingchange?: (playing: boolean) => void;
  }

  let { onplayingchange }: Props = $props();

  const decks: { id: MixDeck; letter: 'A' | 'B' }[] = [
    { id: 'a', letter: 'A' },
    { id: 'b', letter: 'B' },
  ];

  // The installed app has no binary channel yet, so the view needs the development core.
  const frames = 'onFrame' in core ? core : null;

  let canvas: HTMLCanvasElement;
  let tracks = $state<LibraryTrack[]>([]);
  let loadedPaths = $state<string[]>(['', '']);
  let playing = $state([false, false]);
  let shownSeconds = $state([0, 0]);
  let durations = $state([0, 0]);
  let device = $state<MixDevice | null>(null);
  let maxCallbackMs = $state<string | null>(null);
  let problem = $state<string | null>(null);

  $effect(() => {
    onplayingchange?.(Boolean(playing[0] || playing[1]));
  });

  function show(state: MixState) {
    device = state.device;
    state.decks.forEach((deck, index) => {
      loadedPaths[index] = deck.path ?? '';
      durations[index] = deck.duration_seconds;
    });
  }

  function failed(error: unknown) {
    problem = error instanceof CoreError ? error.message : String(error);
  }

  async function load(index: number, path: string) {
    problem = null;
    try {
      show(await core.call('mix_load', { deck: decks[index]!.id, path }));
    } catch (error) {
      failed(error);
    }
  }

  async function toggle(index: number) {
    problem = null;
    try {
      show(await core.call('mix_play', { deck: decks[index]!.id, playing: !playing[index] }));
    } catch (error) {
      failed(error);
    }
  }

  /** The rgb() a colour token resolves to on the canvas, as 0 to 1 channels. */
  function tone(name: string): [number, number, number] {
    canvas.style.forcedColorAdjust = 'none';
    canvas.style.color = `var(${name})`;
    const [r = 0, g = 0, b = 0] = (getComputedStyle(canvas).color.match(/[\d.]+/g) ?? []).map(
      Number,
    );
    canvas.style.color = '';
    canvas.style.forcedColorAdjust = '';
    return [r / 255, g / 255, b / 255];
  }

  onMount(() => {
    core.call('mix_status').then(show, failed);
    core.call('tracks', { offset: 0, limit: 1000 }).then((page) => (tracks = page.tracks), failed);
    if (!frames) {
      return () => onplayingchange?.(false);
    }
    const renderer = MixRenderer.on(canvas);
    if (!renderer) {
      problem = 'This browser has no WebGL2, which the Mix view draws with.';
      return () => onplayingchange?.(false);
    }

    const telemetry = emptyTelemetry();
    const clocks = [new DeckClock(), new DeckClock()] as const;
    const lanes: [LaneState, LaneState] = [
      { positionSeconds: 0, meterLeft: 0, meterRight: 0 },
      { positionSeconds: 0, meterLeft: 0, meterRight: 0 },
    ];
    const generations = [0, 0];
    const waveformGenerations = [0, 0];
    let peakCallbackNs = 0;
    let lastCallbackStamp = -1;
    let changed = true;

    const stopListening = frames.onFrame((frame, arrivedAt) => {
      const kind = frameKind(frame);
      if (kind === WAVEFORM) {
        const waveform = readWaveform(frame);
        renderer.setWaveform(waveform);
        waveformGenerations[waveform.deck] = waveform.generation;
        changed = true;
      } else if (kind === TELEMETRY) {
        readTelemetry(frame, telemetry);
        if (telemetry.decks[0].playing || telemetry.decks[1].playing) {
          peakCallbackNs = Math.max(peakCallbackNs, telemetry.engine.windowMaxCallbackNs);
        }
        const stamp = Math.floor(arrivedAt / 1000);
        if (peakCallbackNs >= 10_000 && stamp !== lastCallbackStamp) {
          lastCallbackStamp = stamp;
          const formatted = (peakCallbackNs / 1_000_000).toFixed(2);
          if (formatted !== maxCallbackMs) maxCallbackMs = formatted;
        }
        for (let index = 0; index < 2; index++) {
          const deck = telemetry.decks[index]!;
          clocks[index]!.update(deck, arrivedAt);
          lanes[index]!.meterLeft = deck.meterLeft;
          lanes[index]!.meterRight = deck.meterRight;
          if (deck.generation !== generations[index]) {
            generations[index] = deck.generation;
            // A short track's waveform can arrive just before the first reading that names it.
            if (waveformGenerations[index] !== deck.generation)
              renderer.clearWaveform(index === 0 ? 0 : 1);
          }
          if (deck.playing !== playing[index]) playing[index] = deck.playing;
          if (deck.durationSeconds !== durations[index]) durations[index] = deck.durationSeconds;
        }
        mixProbe.reading(telemetry, clocks[0].lastError, clocks[1].lastError);
        changed = true;
      }
    });

    const readColours = () => {
      renderer.palette = {
        background: tone('--color-background'),
        divider: tone('--color-border'),
        upcoming: tone('--color-text-2'),
        played: tone('--color-text'),
        playhead: tone('--color-accent'),
        meterTrack: tone('--color-surface-2'),
        meter: tone('--color-text-2'),
        meterHot: tone('--color-text'),
      };
      changed = true;
    };
    const resize = () => {
      const dpr = devicePixelRatio || 1;
      renderer.resize(
        Math.max(1, Math.round(canvas.clientWidth * dpr)),
        Math.max(1, Math.round(canvas.clientHeight * dpr)),
        dpr,
      );
      changed = true;
    };

    let visible = true;
    let request = 0;
    const frame = (now: number) => {
      request = requestAnimationFrame(frame);
      if (!visible || document.hidden) return;
      if (!changed && !clocks[0].playing && !clocks[1].playing) return;
      const started = performance.now();
      for (let index = 0; index < 2; index++) {
        const position = clocks[index]!.at(now);
        lanes[index]!.positionSeconds = position;
        const whole = Math.floor(position);
        if (whole !== shownSeconds[index]) shownSeconds[index] = whole;
      }
      renderer.draw(lanes[0], lanes[1]);
      changed = false;
      mixProbe.frame(now, performance.now() - started);
    };

    const seen = new IntersectionObserver(([entry]) => (visible = entry?.isIntersecting ?? true));
    const sized = new ResizeObserver(resize);
    seen.observe(canvas);
    sized.observe(canvas);
    const stopThemes = onThemeChange(readColours);
    readColours();
    resize();
    mixProbe.attached = true;
    request = requestAnimationFrame(frame);

    return () => {
      mixProbe.attached = false;
      onplayingchange?.(false);
      cancelAnimationFrame(request);
      stopListening();
      seen.disconnect();
      sized.disconnect();
      stopThemes();
      renderer.release();
    };
  });
</script>

<section class="mix" aria-label="Mix">
  <div class="header">
    <div class="header-left">
      <h1 class="heading">Mix</h1>
      <Tooltip text="Audio device selection arrives in M7">
        <Button variant="select" size="sm" aria-label="Audio device" aria-disabled="true">
          {device ? device.name : 'Default output'}
        </Button>
      </Tooltip>
    </div>

    <div class="header-right">
      {#if device}
        <DataTag
          variant="rect"
          label="{(device.sample_rate / 1000).toFixed(
            1,
          )} kHz · {device.period_frames} frames{maxCallbackMs ? ` · ${maxCallbackMs} ms max` : ''}"
        />
      {:else}
        <span class="idle-note">Audio opens when a track loads</span>
      {/if}
      <Tooltip text="Session recording arrives in M7" side="left">
        <Button size="sm" aria-label="Record session" aria-disabled="true" tabindex={-1}>
          <span class="rec-dot" aria-hidden="true"></span>Rec
        </Button>
      </Tooltip>
    </div>
  </div>

  <div class="body">
    {#if !frames}
      <p class="note">
        The Mix view runs against the development core for now. Start it with just serve and open
        the page from just ui.
      </p>
    {/if}

    {#if problem}
      <p class="problem" role="alert">{problem}</p>
    {/if}

    <div class="waveform-card">
      <canvas bind:this={canvas} class="lanes" aria-hidden="true"></canvas>
    </div>

    <div class="decks">
      {#each decks as deck, index (deck.id)}
        <Deck
          deck={deck.letter}
          variant="full"
          {tracks}
          loadedPath={loadedPaths[index]}
          playing={playing[index]}
          shownSeconds={shownSeconds[index]}
          durationSeconds={durations[index]}
          canLoad={Boolean(frames)}
          onload={(path) => load(index, path)}
          ontoggle={() => toggle(index)}
        />
      {/each}
    </div>
  </div>
</section>

<style>
  .mix {
    display: grid;
    grid-template-rows: var(--control-height-lg) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: 0 var(--space-5);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .header-left,
  .header-right {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .heading {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-xl);
    font-weight: var(--weight-medium);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-tight);
  }

  .idle-note {
    color: var(--color-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .rec-dot {
    width: var(--provenance-dot-size);
    height: var(--provenance-dot-size);
    border-radius: var(--radius-pill);
    background: var(--color-muted);
  }

  .body {
    display: grid;
    align-content: start;
    gap: var(--space-4);
    min-height: 0;
    padding: var(--space-4) var(--space-5);
    overflow-y: auto;
  }

  .note,
  .problem {
    margin: 0;
    padding: var(--space-2-5) var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .problem {
    border-color: var(--color-border-control);
    color: var(--color-text);
  }

  .waveform-card {
    overflow: hidden;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-background);
  }

  .lanes {
    display: block;
    width: 100%;
    height: calc(2 * var(--mix-lane-height) + var(--border-width));
  }

  .decks {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: var(--space-4);
  }
</style>
