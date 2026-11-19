<script lang="ts">
  import Button from '../design/Button.svelte';
  import Card from '../design/Card.svelte';
  import DataTag from '../design/DataTag.svelte';
  import FocusBrackets from '../design/FocusBrackets.svelte';
  import Icon from '../design/Icon.svelte';
  import ScrubBar from '../design/ScrubBar.svelte';
  import Thumb from '../design/Thumb.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import { audioSummary, duration, tempo, trackSeed } from '../library/format';
  import { keyDetail } from '../library/key-notation.svelte';

  interface Props {
    deck: 'A' | 'B';
    variant?: 'dock' | 'full';
    tracks?: LibraryTrack[];
    loadedPath?: string;
    playing?: boolean;
    shownSeconds?: number;
    durationSeconds?: number;
    canLoad?: boolean;
    onload?: (path: string) => void;
    ontoggle?: () => void;
  }

  let {
    deck,
    variant = 'dock',
    tracks = [],
    loadedPath = '',
    playing = false,
    shownSeconds = 0,
    durationSeconds = 0,
    canLoad = false,
    onload,
    ontoggle,
  }: Props = $props();

  let focused = $state(false);

  const loadedTrack = $derived(
    loadedPath ? tracks.find((item) => item.location === loadedPath) : undefined,
  );

  const fallbackTitle = $derived.by(() => {
    if (loadedTrack) return loadedTrack.title;
    if (!loadedPath) return `Deck ${deck} is empty`;
    const parts = loadedPath.split(/[\\/]/);
    return parts[parts.length - 1] || loadedPath;
  });

  const tempoTag = $derived(
    loadedTrack && loadedTrack.bpm !== null ? `${tempo(loadedTrack.bpm)} BPM` : null,
  );

  const keyTag = $derived.by(() => {
    if (loadedTrack?.key) return keyDetail(loadedTrack.key);
    return loadedTrack?.key_tag ?? null;
  });

  const formatLine = $derived(
    loadedTrack ? audioSummary(loadedTrack.format, loadedTrack.audio) : null,
  );

  const progressPercent = $derived(
    durationSeconds > 0 ? Math.min(100, Math.max(0, (shownSeconds / durationSeconds) * 100)) : 0,
  );

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== ' ' || !loadedPath) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest('button, select, input')) return;
    event.preventDefault();
    ontoggle?.();
  }
</script>

{#if variant === 'dock'}
  <div class="deck dock">
    <Card variant="surface" radius="lg">
      <div class="pod">
        <span class="badge" aria-hidden="true">{deck}</span>
        <span class="art" aria-hidden="true"></span>
        <div class="text">
          <span class="dock-title">Deck {deck} is empty</span>
          <span class="note">Decks arrive in M7</span>
        </div>
        <Tooltip text="Deck {deck} actions arrive in M7" side="left">
          <Button
            variant="icon"
            size="sm"
            aria-label="Deck {deck} actions"
            aria-disabled="true"
            tabindex={-1}
          >
            <Icon name="more" size="xs" />
          </Button>
        </Tooltip>
      </div>
    </Card>
  </div>
{:else}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="deck full"
    role="region"
    aria-label="Deck {deck}"
    tabindex="0"
    onfocusin={() => (focused = true)}
    onfocusout={(event) => {
      const next = event.relatedTarget as Node | null;
      if (!next || !event.currentTarget.contains(next)) focused = false;
    }}
    {onkeydown}
  >
    <FocusBrackets active={focused}>
      <Card variant="surface" radius="lg">
        <div class="full-body">
          <div class="head">
            <span class="badge" aria-hidden="true">{deck}</span>
            {#if loadedPath}
              <Thumb size="md" seed={trackSeed(loadedPath)} alt="{fallbackTitle} artwork" />
            {:else}
              <span class="art" aria-hidden="true"></span>
            {/if}
            <div class="identity">
              <span class={['full-title', { empty: !loadedPath }]}>{fallbackTitle}</span>
              <span class="artist">
                {#if loadedTrack}
                  {loadedTrack.artist ?? 'No artist tag'}
                {:else if loadedPath}
                  Loaded from path
                {:else}
                  Select a track to load
                {/if}
              </span>
            </div>
            <div class="select-wrap">
              <select
                class="track-select"
                aria-label="Track on Deck {deck}"
                value={loadedPath}
                onchange={(event) => onload?.(event.currentTarget.value)}
                disabled={!canLoad}
              >
                <option value="" disabled>Load track</option>
                {#each tracks as track (track.id)}
                  <option value={track.location}>
                    {track.artist ? `${track.artist} - ${track.title}` : track.title}
                  </option>
                {/each}
              </select>
              <span class="select-label" aria-hidden="true">
                {loadedPath ? 'Switch' : 'Load track'}
                <Icon name="chevron-down" size="xs" />
              </span>
            </div>
          </div>

          <div class="readout">
            <div class="tags">
              {#if tempoTag}
                <DataTag variant="pill" label={tempoTag} />
              {/if}
              {#if keyTag}
                <DataTag variant="pill" label={keyTag} />
              {/if}
              {#if formatLine}
                <span class="format numeric">{formatLine}</span>
              {:else if !loadedPath}
                <span class="format muted numeric">No track loaded</span>
              {/if}
            </div>
            <span class="time numeric">
              {duration(shownSeconds * 1000)} / {duration(durationSeconds * 1000)}
            </span>
          </div>

          <div class="transport">
            <Button
              variant={playing ? 'primary' : 'default'}
              size="lg"
              onclick={ontoggle}
              disabled={!loadedPath}
            >
              {playing ? 'Stop' : 'Play'}
            </Button>
            <Tooltip text="Cue points arrive in M7">
              <Button size="lg" aria-disabled="true" tabindex={-1}>Cue</Button>
            </Tooltip>
            <Tooltip text="Beat sync arrives in M7">
              <Button size="lg" aria-disabled="true" tabindex={-1}>Sync</Button>
            </Tooltip>
            <Tooltip text="Keylock arrives in M7">
              <Button size="lg" aria-disabled="true" tabindex={-1}>Keylock</Button>
            </Tooltip>
          </div>

          <ScrubBar
            size="md"
            stretch
            value={progressPercent}
            max={100}
            disabled
            hideLabel
            label="Deck {deck} position"
          />
        </div>
      </Card>
    </FocusBrackets>
  </div>
{/if}

<style>
  .deck {
    min-width: 0;
  }

  .dock :global(.card) {
    height: var(--dock-card-height);
  }

  .full :global(.brackets) {
    display: block;
    width: 100%;
    padding: var(--space-1);
  }

  .pod,
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .badge {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: var(--control-height-md);
    height: var(--control-height-md);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }

  .art {
    flex-shrink: 0;
    width: var(--thumb-md);
    height: var(--thumb-md);
    border: var(--border-width) dashed var(--color-border);
    border-radius: var(--radius-md);
  }

  .text,
  .identity {
    display: grid;
    flex: 1;
    gap: var(--space-0-5);
    min-width: 0;
  }

  .dock-title,
  .note,
  .full-title,
  .artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dock-title {
    color: var(--color-text-2);
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }

  .note {
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .full-body {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .full-title {
    color: var(--color-text);
    font-family: var(--font-display);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-tight);
  }

  .full-title.empty {
    color: var(--color-text-2);
  }

  .artist {
    color: var(--color-text-2);
    font-size: var(--text-base);
  }

  .full:focus-visible {
    outline: none;
  }

  .select-wrap {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
    align-self: flex-start;
  }

  .select-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    height: var(--control-height);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    white-space: nowrap;
    pointer-events: none;
  }

  .track-select {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
  }

  .select-wrap:hover:has(.track-select:not(:disabled)) .select-label {
    background: var(--color-raised);
  }

  .select-wrap:has(.track-select:focus-visible) .select-label {
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: var(--focus-ring-offset);
  }

  .select-wrap:has(.track-select:disabled) .select-label {
    border-color: var(--color-border);
    background: transparent;
    color: var(--color-muted);
  }

  .track-select:disabled {
    cursor: default;
  }

  .readout {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-height: var(--control-height-xs);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .format {
    color: var(--color-text-2);
    font-size: var(--text-xs);
    line-height: var(--leading-mono);
  }

  .format.muted {
    color: var(--color-muted);
  }

  .time {
    margin-left: auto;
    color: var(--color-text);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .transport {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }
</style>
