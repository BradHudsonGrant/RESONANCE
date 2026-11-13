<script lang="ts">
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';

  // Playback arrives with the preview deck. Until then the bar says what is selected and that
  // nothing is playing, and has no controls that would do nothing.
  let { track }: { track: LibraryTrack | undefined } = $props();
</script>

<footer class="player" aria-label="Player">
  <span class="state">Nothing playing</span>
  {#if track}
    <span class="selected">
      <span class="title">{track.title}</span>
      {#if track.artist}<span class="artist">{track.artist}</span>{/if}
    </span>
  {/if}
</footer>

<style>
  .player {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    height: 48px;
    padding: 0 var(--space-4);
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
    font-size: var(--text-sm);
  }

  .state {
    color: var(--color-muted);
  }

  .selected {
    display: flex;
    gap: var(--space-2);
    min-width: 0;
    white-space: nowrap;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .artist {
    color: var(--color-text-2);
  }
</style>
