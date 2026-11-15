<script lang="ts">
  import Button from '../design/Button.svelte';
  import Card from '../design/Card.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import ScrubBar from '../design/ScrubBar.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import { duration } from '../library/format';

  interface Props {
    trackId?: number | null;
    title?: string | null;
    artist?: string | null;
    playing?: boolean;
    positionMs?: number;
    durationMs?: number;
    volume?: number;
    muted?: boolean;
    canPlay?: boolean;
    error?: string | null;
    ontoggleplay?: () => void;
    onseek?: (positionMs: number) => void;
    onvolume?: (volume: number) => void;
    ontogglemute?: () => void;
  }

  let {
    trackId = null,
    title = null,
    artist = null,
    playing = false,
    positionMs = 0,
    durationMs = 0,
    volume = 1,
    muted = false,
    canPlay = false,
    error = null,
    ontoggleplay,
    onseek,
    onvolume,
    ontogglemute,
  }: Props = $props();

  const hasTrack = $derived(trackId !== null);
  const playEnabled = $derived(hasTrack || canPlay);
  const elapsedText = $derived(hasTrack ? duration(positionMs) : '0:00');
  const totalText = $derived(hasTrack && durationMs > 0 ? duration(durationMs) : '0:00');
  const scrubMax = $derived(hasTrack && durationMs > 0 ? durationMs : 100);
  const scrubStep = $derived(scrubMax >= 2000 ? 1000 : 10);
  const scrubValue = $derived(
    hasTrack && durationMs > 0 ? Math.min(Math.max(0, positionMs), durationMs) : 0,
  );
  const isMuted = $derived(muted || volume <= 0);
  const volumePercent = $derived(Math.round((isMuted ? 0 : volume) * 100));
</script>

<footer class="player" aria-label="Player">
  <Card variant="surface" radius="xl">
    <div class="row">
      <div class="transport">
        <Tooltip
          text={!playEnabled
            ? 'Select a track to preview'
            : playing
              ? 'Pause preview'
              : 'Play preview'}
          shortcut={playEnabled ? 'Space' : undefined}
        >
          <Button
            variant="circle"
            size="lg"
            aria-label={playing ? 'Pause' : 'Play'}
            aria-disabled={!playEnabled}
            aria-pressed={playing}
            onclick={() => {
              if (playEnabled) ontoggleplay?.();
            }}
          >
            <Icon name={playing ? 'pause' : 'play'} size="sm" filled />
          </Button>
        </Tooltip>

        {#if hasTrack}
          <div class="track">
            <span class="track-title" title={title ?? undefined}>{title ?? ''}</span>
            {#if error}
              <span class="player-error" role="alert" title={error}>{error}</span>
            {:else if artist}
              <span class="track-artist" title={artist}>{artist}</span>
            {/if}
          </div>
        {:else if error}
          <span class="player-error standalone" role="alert" title={error}>{error}</span>
        {:else}
          <span class="state">Nothing playing</span>
        {/if}
      </div>

      <div class="scrub">
        <span class={['time elapsed numeric', { unloaded: !hasTrack }]}>{elapsedText}</span>
        <ScrubBar
          size="md"
          stretch
          label="Scrub"
          hideLabel
          value={scrubValue}
          min={0}
          max={scrubMax}
          step={scrubStep}
          disabled={!hasTrack || durationMs <= 0}
          onInput={(value) => onseek?.(value)}
        />
        <span class={['time total numeric', { unloaded: !hasTrack }]}>{totalText}</span>
      </div>

      <div class="mixer">
        <div class="preview-volume">
          <Tooltip text={isMuted ? 'Unmute preview' : 'Mute preview'}>
            <Button
              variant="icon"
              active={isMuted}
              aria-label={isMuted ? 'Unmute' : 'Mute'}
              aria-pressed={isMuted}
              onclick={() => ontogglemute?.()}
            >
              <Icon name={isMuted ? 'volume-mute' : 'volume'} size="sm" />
            </Button>
          </Tooltip>
          <Tooltip text={`Preview volume · ${volumePercent}%`}>
            <ScrubBar
              size="sm"
              label="Volume"
              hideLabel
              min={0}
              max={100}
              step={1}
              value={volumePercent}
              onInput={(pct) => onvolume?.(pct / 100)}
            />
          </Tooltip>
        </div>

        <div class="deck-mixer">
          <ModularDivider orientation="vertical" />

          <!-- The decks' mixer arrives in M7, so those controls stay unavailable. -->
          <Tooltip text="Deck volume arrives in M7">
            <ScrubBar size="sm" label="A Vol" disabled />
          </Tooltip>
          <Tooltip text="Deck volume arrives in M7">
            <ScrubBar size="sm" label="B Vol" disabled />
          </Tooltip>
          <span class="label m7-label">Curve</span>
          <Tooltip text="Crossfader curves arrive in M7">
            <Button
              variant="select"
              size="sm"
              aria-label="Curve"
              aria-disabled="true"
              tabindex={-1}
            >
              —
            </Button>
          </Tooltip>
          <span class="label m7-label">EQ</span>
          <Tooltip text="EQ arrives in M7">
            <Button variant="select" size="sm" aria-label="EQ" aria-disabled="true" tabindex={-1}>
              —
            </Button>
          </Tooltip>
          <ModularDivider orientation="vertical" />
          <Tooltip text="Headphone cue arrives in M7" side="left">
            <Button variant="icon" aria-label="Headphone cue" aria-disabled="true" tabindex={-1}>
              <Icon name="headphones" size="md" />
            </Button>
          </Tooltip>
        </div>
      </div>
    </div>
  </Card>
</footer>

<style>
  .player {
    container: player / inline-size;
    padding: 0 var(--player-inset-x) var(--player-inset-bottom);
  }

  .player :global(.card) {
    height: var(--player-height);
    padding: 0 var(--space-4);
  }

  .player :global(.content) {
    height: 100%;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(var(--player-scrub-min-width), 1fr) auto;
    align-items: center;
    gap: var(--space-6);
    height: 100%;
  }

  .transport,
  .mixer,
  .deck-mixer,
  .preview-volume {
    display: flex;
    align-items: center;
  }

  .transport {
    gap: var(--space-3);
    min-width: 0;
  }

  .mixer,
  .deck-mixer {
    gap: var(--space-3-5);
  }

  .preview-volume {
    gap: var(--space-1-5);
  }

  .track {
    display: flex;
    flex-direction: column;
    min-width: 0;
    max-width: clamp(110px, 16cqi, 240px);
  }

  .track-title,
  .track-artist,
  .player-error {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: var(--leading-compact);
  }

  .track-title {
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .track-artist {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .player-error {
    color: var(--color-accent);
    font-size: var(--text-xs);
  }

  .player-error.standalone {
    max-width: clamp(120px, 20cqi, 280px);
    margin-left: var(--space-1);
    font-size: var(--text-sm);
  }

  .state {
    margin-left: var(--space-1);
    color: var(--color-muted);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .scrub {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .time {
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    white-space: nowrap;
  }

  .time.elapsed {
    color: var(--color-text);
  }

  .time.total {
    color: var(--color-text-2);
  }

  .time.unloaded {
    color: var(--color-muted);
  }

  .label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  /* Below a 1360 px window the volume sliders shorten. Container queries cannot read tokens. */
  @container player (width < 1320px) {
    .mixer,
    .deck-mixer {
      --scrub-width-sm: var(--scrub-width-compact);
      gap: var(--space-2-5);
    }

    .row {
      gap: var(--space-4);
    }
  }

  @container player (width < 1120px) {
    .m7-label {
      display: none;
    }

    .mixer,
    .deck-mixer {
      gap: var(--space-2);
    }

    .row {
      gap: var(--space-3);
    }
  }

  @container player (width < 960px) {
    .deck-mixer {
      display: none;
    }
  }
</style>
