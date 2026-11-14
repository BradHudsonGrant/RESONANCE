<script lang="ts">
  import X from '@lucide/svelte/icons/x';

  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import { duration, fileSize, sampleRate, tempo } from './format';

  interface Props {
    track: LibraryTrack | undefined;
    onclose: () => void;
  }

  let { track, onclose }: Props = $props();

  interface Field {
    label: string;
    value: string | null | undefined;
    numeric?: boolean;
  }

  function keyText(track: LibraryTrack): string | null {
    if (track.key) return `${track.key.camelot}  ${track.key.name}`;
    return track.key_tag;
  }

  function fields(track: LibraryTrack): Field[] {
    const audio = track.audio;
    return [
      { label: 'Album', value: track.album },
      { label: 'Album artist', value: track.album_artist },
      { label: 'Genre', value: track.genre },
      { label: 'Year', value: track.year, numeric: true },
      { label: 'Label', value: track.label },
      { label: 'Track number', value: track.track_number, numeric: true },
      { label: 'BPM', value: tempo(track.bpm), numeric: true },
      { label: 'Key', value: keyText(track), numeric: true },
      { label: 'Time', value: duration(audio?.duration_ms), numeric: true },
      { label: 'Format', value: track.format.toUpperCase() },
      {
        label: 'Sample rate',
        value: audio?.sample_rate ? sampleRate(audio.sample_rate) : null,
        numeric: true,
      },
      {
        label: 'Bit depth',
        value: audio?.bit_depth ? `${audio.bit_depth}-bit` : null,
        numeric: true,
      },
      {
        label: 'Bitrate',
        value: audio?.bitrate_kbps ? `${audio.bitrate_kbps} kbps` : null,
        numeric: true,
      },
      { label: 'Channels', value: audio?.channels?.toString(), numeric: true },
      { label: 'Size', value: fileSize(track.size_bytes), numeric: true },
      { label: 'Comment', value: track.comment },
    ].filter((field) => field.value);
  }
</script>

<aside class="inspector" aria-label="Inspector">
  <div class="close">
    <Tooltip text="Hide the inspector" shortcut="I" side="left">
      <Button variant="icon" aria-label="Hide the inspector" onclick={onclose}>
        <Icon icon={X} size="sm" />
      </Button>
    </Tooltip>
  </div>
  {#if track}
    <div class="body">
      <h2 class="title">{track.title}</h2>
      <p class={['artist', { unknown: !track.artist }]}>{track.artist ?? 'No artist tag'}</p>
      {#if !track.audio}
        <p class="warning">
          The file could not be read as audio, so its length and quality are unknown.
        </p>
      {/if}
      <dl>
        {#each fields(track) as field (field.label)}
          <dt>{field.label}</dt>
          <dd class={{ numeric: field.numeric }}>{field.value}</dd>
        {/each}
        <dt>Location</dt>
        <dd class="location">{track.location}</dd>
      </dl>
    </div>
  {:else}
    <p class="empty">Select a track to see its details here.</p>
  {/if}
</aside>

<style>
  .inspector {
    position: relative;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    width: 100%;
    height: 100%;
    min-height: 0;
    border-left: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .close {
    position: absolute;
    top: var(--space-3);
    right: var(--space-3);
  }

  .body {
    min-height: 0;
    padding: var(--space-4);
    overflow-y: auto;
  }

  .title {
    padding-right: var(--control-height-md);
    font-size: var(--text-lg);
    overflow-wrap: anywhere;
  }

  .artist {
    margin: var(--space-1) 0 0;
    color: var(--color-text-2);
  }

  .unknown {
    color: var(--color-muted);
  }

  .warning {
    margin: var(--space-3) 0 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--space-1) var(--space-4);
    margin: var(--space-4) 0 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--color-text-2);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .location {
    grid-column: 1 / -1;
    color: var(--color-text-2);
    user-select: text;
  }

  .empty {
    margin: 0;
    padding: var(--space-4) var(--control-height-md) var(--space-4) var(--space-4);
    color: var(--color-text-2);
  }
</style>
