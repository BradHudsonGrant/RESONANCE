<script lang="ts">
  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Thumb from '../design/Thumb.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { TrackArtwork } from '../ipc/bindings/TrackArtwork';
  import type { TrackQuality } from '../ipc/bindings/TrackQuality';
  import CollectionMenu from './CollectionMenu.svelte';
  import { audioSummary, duration, fileSize, level, tempo, trackSeed } from './format';
  import { keyDetail } from './key-notation.svelte';
  import QualityFlags from './QualityFlags.svelte';

  interface Props {
    track: LibraryTrack | undefined;
    inSheet?: boolean;
    previewTrackId?: number | null;
    previewPlaying?: boolean;
    collections?: LibraryCollection[];
    activeCollection?: LibraryCollection | null;
    onclose: () => void;
    onpreview?: (track: LibraryTrack) => void;
    onrelink?: (track: LibraryTrack) => void;
    onopeninprep?: (track: LibraryTrack) => void;
    onidentifyonline?: (track: LibraryTrack) => void;
    identifying?: boolean;
    ontogglefavourite?: (track: LibraryTrack) => void;
    onaddtocollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onremovefromcollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    oncreatecollectionandadd?: (name: string, trackIds: number[]) => Promise<void> | void;
    /** The selected track's picture state, with any release picture waiting for a decision. */
    artwork?: TrackArtwork | null;
    artworkError?: string | null;
    onpickartwork?: (trackId: number, file: File) => void;
    onacceptartwork?: (trackId: number) => void;
    onskipartwork?: (trackId: number) => void;
    /** The selected track's quality check. */
    quality?: TrackQuality | null;
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
    onrelink,
    onopeninprep,
    onidentifyonline,
    identifying = false,
    ontogglefavourite,
    onaddtocollection,
    onremovefromcollection,
    oncreatecollectionandadd,
    artwork = null,
    artworkError = null,
    onpickartwork,
    onacceptartwork,
    onskipartwork,
    quality = null,
  }: Props = $props();

  const checked = $derived(quality && track && quality.track_id === track.id ? quality : null);

  function qualityRow(item: TrackQuality): DetailCell[] {
    return [
      { label: 'Loudness', value: level(item.integrated_lufs, 'LUFS') || null, numeric: true },
      { label: 'Range', value: level(item.loudness_range_lu, 'LU') || null, numeric: true },
      {
        label: 'True peak',
        value: level(item.true_peak_dbtp, 'dBTP', true) || null,
        numeric: true,
      },
    ];
  }

  const proposed = $derived(
    artwork && track && artwork.track_id === track.id ? artwork.proposed : null,
  );

  let picker = $state<HTMLInputElement | null>(null);
  let dropping = $state(false);

  function picked(file: File | undefined) {
    if (file && track) onpickartwork?.(track.id, file);
  }

  function carriesFiles(event: DragEvent): boolean {
    return Boolean(event.dataTransfer?.types.includes('Files'));
  }

  const isMissing = $derived(Boolean(track && track.missing_since != null));
  const isPreviewTrack = $derived(Boolean(track && previewTrackId === track.id && !isMissing));
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
    if (item.key) return keyDetail(item.key);
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
        <Tooltip text="Choose or drop a cover" side="below">
          <button
            type="button"
            class={['cover', { dropping }]}
            aria-label="Choose a cover for {track.title}"
            disabled={!onpickartwork}
            onclick={() => picker?.click()}
            ondragenter={(event) => {
              if (carriesFiles(event)) dropping = true;
            }}
            ondragover={(event) => {
              if (!carriesFiles(event)) return;
              event.preventDefault();
              if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
            }}
            ondragleave={() => (dropping = false)}
            ondrop={(event) => {
              event.preventDefault();
              dropping = false;
              picked(event.dataTransfer?.files[0]);
            }}
          >
            <Thumb
              size="lg"
              seed={trackSeed(track.location)}
              src={track.artwork ? artworkUrl(track.artwork) : null}
              alt="{track.title} artwork"
            />
          </button>
        </Tooltip>
        <input
          bind:this={picker}
          class="picker"
          type="file"
          accept="image/jpeg,image/png"
          tabindex="-1"
          aria-hidden="true"
          onchange={(event) => {
            picked(event.currentTarget.files?.[0]);
            event.currentTarget.value = '';
          }}
        />
        <div class="identity">
          <div class="hero-top">
            <h2 class="title">{track.title}</h2>
            <div class="tools">
              <Tooltip
                text={isMissing
                  ? 'File is missing — relink to play'
                  : isPreviewPlaying
                    ? 'Pause preview'
                    : 'Play preview'}
                side="left"
              >
                <Button
                  variant="icon"
                  active={isPreviewTrack}
                  disabled={isMissing}
                  aria-label={isPreviewPlaying ? `Pause ${track.title}` : `Play ${track.title}`}
                  aria-pressed={isPreviewPlaying}
                  onclick={() => track && !isMissing && onpreview?.(track)}
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

      {#if !isMissing && (onopeninprep || onidentifyonline)}
        <div class="prep-actions">
          {#if onopeninprep}
            <Button size="sm" onclick={() => track && onopeninprep(track)}>
              <Icon name="prep" size="xs" />
              Open in Prep
            </Button>
          {/if}
          {#if onidentifyonline}
            <Button
              size="sm"
              disabled={identifying}
              onclick={() => track && onidentifyonline(track)}
            >
              <Icon name="search" size="xs" />
              {identifying ? 'Looking up…' : 'Look up online'}
            </Button>
          {/if}
        </div>
      {/if}

      {#if isMissing}
        <div class="missing-banner" role="status">
          <div class="missing-copy">
            <div class="missing-head">
              <Icon name="missing" size="sm" />
              <span class="missing-title">Missing file</span>
            </div>
            <p class="missing-note">
              This track stays in your library and collections, but the file is no longer at its
              recorded path.
            </p>
          </div>
          {#if onrelink}
            <Button size="sm" variant="primary" onclick={() => track && onrelink(track)}>
              <Icon name="relink" size="xs" />
              Relink…
            </Button>
          {/if}
        </div>
      {/if}

      {#if artworkError}
        <p class="warning" role="alert">{artworkError}</p>
      {/if}

      {#if proposed}
        <section class="proposed" aria-label="Proposed cover">
          <Thumb
            size="md"
            seed={trackSeed(track.location)}
            src={artworkUrl(proposed.artwork)}
            alt="Proposed cover"
          />
          <div class="proposed-copy">
            <MonoLabel text="Cover" size="xs" tone="muted" />
            <p class="proposed-note">Release cover from {proposed.provider}</p>
            <div class="proposed-actions">
              <Button
                size="sm"
                variant="primary"
                onclick={() => track && onacceptartwork?.(track.id)}
              >
                Use cover
              </Button>
              <Button size="sm" onclick={() => track && onskipartwork?.(track.id)}>Skip</Button>
            </div>
          </div>
        </section>
      {/if}

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

      {#if checked?.checked}
        <ModularDivider />

        <section class="quality" aria-label="Quality">
          <MonoLabel text="Quality" size="xs" tone="muted" />
          <div class="detail-row quality-levels">
            {#each qualityRow(checked) as cell (cell.label)}
              <div class="cell">
                <MonoLabel text={cell.label} size="xs" tone="muted" />
                <span class={['value', { numeric: cell.numeric, missing: !cell.value }]}>
                  {cell.value ?? '—'}
                </span>
              </div>
            {/each}
          </div>
          {#if checked.flags.length > 0}
            <QualityFlags flags={checked.flags} />
          {:else}
            <p class="quality-clean">No problems found</p>
          {/if}
        </section>
      {/if}

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
  onRelink={onrelink}
  onOpenInPrep={menuShowFavourite && onopeninprep
    ? (tracks) => tracks[0] && onopeninprep(tracks[0])
    : undefined}
  onIdentifyOnline={menuShowFavourite && onidentifyonline
    ? (tracks) => tracks[0] && onidentifyonline(tracks[0])
    : undefined}
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

  .cover {
    display: inline-flex;
    padding: 0;
    border: 0;
    border-radius: var(--radius-lg);
    background: none;
    cursor: pointer;
  }

  .cover:disabled {
    cursor: default;
  }

  .cover.dropping {
    outline: var(--border-width) dashed var(--color-border-control);
    outline-offset: var(--space-0-5);
  }

  .picker {
    display: none;
  }

  .proposed {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
  }

  .proposed-copy {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .proposed-note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .proposed-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  @media (forced-colors: active) {
    .cover.dropping {
      outline-color: Highlight;
    }
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

  .prep-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .warning {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .missing-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-3);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface-2);
  }

  .missing-copy {
    display: grid;
    flex: 1 1 12rem;
    gap: var(--space-1);
    min-width: 0;
  }

  .missing-head {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .missing-note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-xs);
    line-height: var(--leading-normal);
  }

  @media (forced-colors: active) {
    .missing-banner {
      border-color: CanvasText;
    }
  }

  .details,
  .quality,
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

  .detail-row.quality-levels {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .quality-clean {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
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
