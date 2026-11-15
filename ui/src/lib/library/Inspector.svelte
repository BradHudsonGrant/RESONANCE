<script lang="ts">
  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Thumb from '../design/Thumb.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import CollectionMenu from './CollectionMenu.svelte';
  import { audioSummary, duration, fileSize, tempo, trackSeed } from './format';

  interface Props {
    track: LibraryTrack | undefined;
    inSheet?: boolean;
    previewTrackId?: number | null;
    previewPlaying?: boolean;
    collections?: LibraryCollection[];
    activeCollection?: LibraryCollection | null;
    onclose: () => void;
    onpreview?: (track: LibraryTrack) => void;
    ontogglefavourite?: (track: LibraryTrack) => void;
    onaddtocollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onremovefromcollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    oncreatecollectionandadd?: (name: string, trackIds: number[]) => Promise<void> | void;
  }

  let {
    track,
    inSheet = false,
    previewTrackId = null,
    previewPlaying = false,
    collections = [],
    activeCollection = null,
    onclose,
    onpreview,
    ontogglefavourite,
    onaddtocollection,
    onremovefromcollection,
    oncreatecollectionandadd,
  }: Props = $props();

  const isPreviewTrack = $derived(Boolean(track && previewTrackId === track.id));
  const isPreviewPlaying = $derived(isPreviewTrack && previewPlaying);

  let menuOpen = $state(false);
  let menuAnchor = $state<HTMLElement | null>(null);
  let menuShowFavourite = $state(false);

  function openMenu(anchor: HTMLElement, includeFavourite: boolean) {
    menuAnchor = anchor;
    menuShowFavourite = includeFavourite;
    menuOpen = true;
  }

  function closeMenu() {
    const anchor = menuAnchor;
    menuOpen = false;
    if (anchor?.isConnected) anchor.focus();
  }

  interface DetailCell {
    label: string;
    value: string | null | undefined;
    numeric?: boolean;
  }

  function keyText(item: LibraryTrack): string | null {
    if (item.key) return `${item.key.camelot} · ${item.key.name}`;
    return item.key_tag;
  }

  function primaryRow(item: LibraryTrack): DetailCell[] {
    return [
      { label: 'BPM', value: tempo(item.bpm) || null, numeric: true },
      { label: 'Key', value: keyText(item), numeric: true },
      { label: 'Time', value: duration(item.audio?.duration_ms) || null, numeric: true },
      { label: 'Release', value: item.year, numeric: true },
    ];
  }

  function secondaryRow(item: LibraryTrack): DetailCell[] {
    return [
      { label: 'Genre', value: item.genre },
      { label: 'Album', value: item.album },
      { label: 'Label', value: item.label },
    ];
  }
</script>

<aside class={['inspector', { 'in-sheet': inSheet }]} aria-label="Inspector">
  {#if track}
    <div class="body">
      <div class="hero">
        <Thumb size="lg" seed={trackSeed(track.location)} alt="{track.title} artwork" />
        <div class="identity">
          <div class="hero-top">
            <h2 class="title">{track.title}</h2>
            <div class="tools">
              <Tooltip text={isPreviewPlaying ? 'Pause preview' : 'Play preview'} side="left">
                <Button
                  variant="icon"
                  active={isPreviewTrack}
                  aria-label={isPreviewPlaying ? `Pause ${track.title}` : `Play ${track.title}`}
                  aria-pressed={isPreviewPlaying}
                  onclick={() => track && onpreview?.(track)}
                >
                  <Icon name={isPreviewPlaying ? 'pause' : 'play'} size="sm" filled />
                </Button>
              </Tooltip>
              <Tooltip
                text={track.favourite ? 'Remove from favourites' : 'Add to favourites'}
                side="left"
              >
                <Button
                  variant="icon"
                  aria-label="Favourite track"
                  aria-pressed={track.favourite}
                  onclick={() => track && ontogglefavourite?.(track)}
                >
                  <Icon name="star" size="sm" filled={track.favourite} />
                </Button>
              </Tooltip>
              <Tooltip text="Track actions" side="left">
                <Button
                  variant="icon"
                  aria-label="More actions"
                  aria-haspopup="menu"
                  aria-expanded={menuOpen && menuShowFavourite}
                  onclick={(event) => openMenu(event.currentTarget, true)}
                >
                  <Icon name="more" size="sm" />
                </Button>
              </Tooltip>
              <Tooltip text="Hide the inspector" shortcut="I" side="left">
                <Button variant="icon" aria-label="Hide the inspector" onclick={onclose}>
                  <Icon name="close" size="sm" />
                </Button>
              </Tooltip>
            </div>
          </div>
          <p class={['artist', { unknown: !track.artist }]}>{track.artist ?? 'No artist tag'}</p>
          <p class="format numeric">{audioSummary(track.format, track.audio)}</p>
        </div>
      </div>

      {#if !track.audio}
        <p class="warning">
          The file could not be read as audio, so its length and quality are unknown.
        </p>
      {/if}

      <ModularDivider />

      <div class="details">
        <div class="detail-row primary">
          {#each primaryRow(track) as cell (cell.label)}
            <div class="cell">
              <MonoLabel text={cell.label} size="xs" tone="muted" />
              <span class={['value', { numeric: cell.numeric, missing: !cell.value }]}>
                {cell.value ?? '—'}
              </span>
            </div>
          {/each}
        </div>

        <div class="detail-row secondary">
          {#each secondaryRow(track) as cell (cell.label)}
            <div class="cell">
              <MonoLabel text={cell.label} size="xs" tone="muted" />
              <span class={['value', { numeric: cell.numeric, missing: !cell.value }]}>
                {cell.value ?? '—'}
              </span>
            </div>
          {/each}
        </div>
      </div>

      <ModularDivider />

      <div class="collections-section">
        <div class="collections-head">
          <MonoLabel text="Collections" size="xs" tone="muted" />
          <Tooltip text="Add to collection" side="left">
            <Button
              variant="circle"
              size="sm"
              aria-label="Add to collection"
              aria-haspopup="menu"
              aria-expanded={menuOpen && !menuShowFavourite}
              onclick={(event) => openMenu(event.currentTarget, false)}
            >
              <Icon name="plus" size="xs" />
            </Button>
          </Tooltip>
        </div>
        {#if track.collections.length > 0}
          <div class="collection-pills">
            {#each track.collections as col (col.id)}
              <DataTag
                variant="removable"
                label={col.name}
                onRemove={() => track && onremovefromcollection?.(col.id, [track.id])}
              />
            {/each}
          </div>
        {:else}
          <p class="collection-empty">None</p>
        {/if}
      </div>

      <ModularDivider />

      <div class="file-facts">
        <div class="detail-row compact">
          <div class="cell">
            <MonoLabel text="Format" size="xs" tone="muted" />
            <span class="value numeric">{track.format.toUpperCase()}</span>
          </div>
          <div class="cell">
            <MonoLabel text="Size" size="xs" tone="muted" />
            <span class="value numeric">{fileSize(track.size_bytes)}</span>
          </div>
          {#if track.album_artist}
            <div class="cell">
              <MonoLabel text="Album artist" size="xs" tone="muted" />
              <span class="value">{track.album_artist}</span>
            </div>
          {/if}
          {#if track.track_number}
            <div class="cell">
              <MonoLabel text="Track" size="xs" tone="muted" />
              <span class="value numeric">{track.track_number}</span>
            </div>
          {/if}
        </div>

        {#if track.comment}
          <div class="cell">
            <MonoLabel text="Comment" size="xs" tone="muted" />
            <p class="comment">{track.comment}</p>
          </div>
        {/if}

        <div class="cell">
          <MonoLabel text="Location" size="xs" tone="muted" />
          <p class="location numeric">{track.location}</p>
        </div>
      </div>
    </div>
  {:else}
    <div class="empty-wrap">
      <div class="empty-head">
        <MonoLabel text="Inspector" size="xs" tone="muted" />
        <Tooltip text="Hide the inspector" shortcut="I" side="left">
          <Button variant="icon" aria-label="Hide the inspector" onclick={onclose}>
            <Icon name="close" size="sm" />
          </Button>
        </Tooltip>
      </div>
      <p class="empty">
        Select a track in the table to read its tags, audio format and file location.
      </p>
    </div>
  {/if}
</aside>

<CollectionMenu
  open={menuOpen && track != null}
  anchorEl={menuAnchor}
  label={menuShowFavourite ? 'Track actions' : 'Add to collection'}
  {collections}
  selectedTracks={track ? [track] : []}
  {activeCollection}
  showFavourite={menuShowFavourite}
  onToggleFavourite={ontogglefavourite}
  onAdd={onaddtocollection}
  onRemove={onremovefromcollection}
  onCreateAndAdd={oncreatecollectionandadd}
  onClose={closeMenu}
/>

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

  .inspector.in-sheet {
    border-left: 0;
    background: transparent;
  }

  .body {
    display: grid;
    align-content: start;
    gap: var(--space-4);
    min-height: 0;
    padding: var(--space-5);
    overflow-y: auto;
  }

  .inspector.in-sheet .body {
    padding: 0;
  }

  .hero {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: start;
    gap: var(--space-4);
  }

  .identity {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    min-height: var(--thumb-lg);
  }

  .hero-top {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-tight);
    overflow-wrap: anywhere;
  }

  .tools {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: var(--space-0-5);
    margin: calc(-1 * var(--space-1)) calc(-1 * var(--space-1-5)) 0 0;
  }

  .artist {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-base);
    overflow-wrap: anywhere;
  }

  .unknown {
    color: var(--color-muted);
  }

  .format {
    margin: auto 0 0;
    padding-bottom: var(--space-0-5);
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-mono);
  }

  .warning {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .details,
  .collections-section,
  .file-facts {
    display: grid;
    gap: var(--space-3-5);
  }

  .collections-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .collection-pills {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1-5);
  }

  .collection-empty {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .detail-row {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-3);
  }

  .detail-row.secondary {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1.2fr);
  }

  .detail-row.compact {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }

  .cell {
    display: grid;
    align-content: start;
    gap: var(--space-1);
    min-width: 0;
  }

  .value {
    color: var(--color-text);
    font-size: var(--text-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .missing {
    color: var(--color-muted);
  }

  .comment {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .location {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: var(--leading-normal);
    word-break: break-all;
    user-select: text;
  }

  .empty-wrap {
    display: grid;
    align-content: start;
    gap: var(--space-2);
    padding: var(--space-5);
  }

  .inspector.in-sheet .empty-wrap {
    padding: 0;
  }

  .empty-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .empty {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-md);
  }
</style>
