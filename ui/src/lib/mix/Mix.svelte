<script lang="ts">
  import { onMount } from 'svelte';

  import Button from '../design/Button.svelte';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { MixDeck } from '../ipc/bindings/MixDeck';
  import type { MixDevice } from '../ipc/bindings/MixDevice';
  import type { MixState } from '../ipc/bindings/MixState';
  import { CoreError } from '../ipc/client';
  import { core } from '../ipc/core';
  import { duration } from '../library/format';
  import { DeckClock } from './clock';
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

  const decks: { id: MixDeck; name: string }[] = [
    { id: 'a', name: 'Deck A' },
    { id: 'b', name: 'Deck B' },
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
  let problem = $state<string | null>(null);

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
    canvas.style.color = getComputedStyle(canvas).getPropertyValue(name);
    const [r = 0, g = 0, b = 0] = (getComputedStyle(canvas).color.match(/[\d.]+/g) ?? []).map(
      Number,
    );
    canvas.style.color = '';
    return [r / 255, g / 255, b / 255];
  }

  onMount(() => {
    core.call('mix_status').then(show, failed);
    core.call('tracks', { offset: 0, limit: 1000 }).then((page) => (tracks = page.tracks), failed);
    if (!frames) return;
    const renderer = MixRenderer.on(canvas);
    if (!renderer) {
      problem = 'This browser has no WebGL2, which the Mix view draws with.';
      return;
    }

    const telemetry = emptyTelemetry();
    const clocks = [new DeckClock(), new DeckClock()] as const;
    const lanes: [LaneState, LaneState] = [
      { positionSeconds: 0, meterLeft: 0, meterRight: 0 },
      { positionSeconds: 0, meterLeft: 0, meterRight: 0 },
    ];
    const generations = [0, 0];
    const waveformGenerations = [0, 0];
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
        peak: tone('--color-text-2'),
        rms: tone('--color-text'),
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

    const scheme = matchMedia('(prefers-color-scheme: dark)');
    const seen = new IntersectionObserver(([entry]) => (visible = entry?.isIntersecting ?? true));
    const sized = new ResizeObserver(resize);
    seen.observe(canvas);
    sized.observe(canvas);
    scheme.addEventListener('change', readColours);
    readColours();
    resize();
    mixProbe.attached = true;
    request = requestAnimationFrame(frame);

    return () => {
      mixProbe.attached = false;
      cancelAnimationFrame(request);
      stopListening();
      seen.disconnect();
      sized.disconnect();
      scheme.removeEventListener('change', readColours);
      renderer.release();
    };
  });
</script>

<section class="mix" aria-label="Mix">
  <div class="decks">
    {#each decks as deck, index (deck.id)}
      <div class="deck" aria-label={deck.name}>
        <span class="name">{deck.name}</span>
        <select
          class="track"
          aria-label="Track on {deck.name}"
          value={loadedPaths[index]}
          onchange={(event) => load(index, event.currentTarget.value)}
          disabled={!frames}
        >
          <option value="" disabled>Choose a track</option>
          {#each tracks as track (track.id)}
            <option value={track.location}>
              {track.artist ? `${track.artist} - ${track.title}` : track.title}
            </option>
          {/each}
        </select>
        <Button onclick={() => toggle(index)} disabled={!loadedPaths[index]}>
          {playing[index] ? 'Stop' : 'Play'}
        </Button>
        <span class="time">
          {duration(shownSeconds[index]! * 1000)} / {duration(durations[index]! * 1000)}
        </span>
      </div>
    {/each}
  </div>

  {#if !frames}
    <p class="note">
      The Mix view runs against the development core for now. Start it with just serve and open the
      page from just ui.
    </p>
  {/if}
  <canvas bind:this={canvas} class="lanes" aria-hidden="true"></canvas>

  <footer class="status">
    {#if problem}
      <span role="alert">{problem}</span>
    {:else if device}
      <span>
        {device.name}, {device.sample_rate / 1000} kHz, {device.period_frames}-frame period ({device.requested_buffer_frames}
        asked for)
      </span>
    {:else}
      <span>Choose a track for a deck. The audio device opens with the first one.</span>
    {/if}
  </footer>
</section>

<style>
  .mix {
    display: grid;
    grid-template-rows: auto auto 1fr auto;
    height: 100%;
    min-height: 0;
  }

  .decks {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
  }

  .deck {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .name {
    font-family: var(--font-heading);
    font-weight: 500;
    white-space: nowrap;
  }

  .track {
    flex: 1;
    min-width: 0;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-control);
    background: var(--color-surface-2);
    color: var(--color-text);
    font: inherit;
  }

  .time {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .note {
    padding: var(--space-3) var(--space-4);
    color: var(--color-text-2);
  }

  .lanes {
    display: block;
    width: 100%;
    height: 100%;
    min-height: 0;
  }

  .status {
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--color-border);
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }
</style>
