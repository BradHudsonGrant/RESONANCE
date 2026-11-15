<script lang="ts">
  import { untrack } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';

  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import TableCell from '../design/TableCell.svelte';
  import TableRow from '../design/TableRow.svelte';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { SortDirection } from '../ipc/bindings/SortDirection';
  import type { TrackSortField } from '../ipc/bindings/TrackSortField';
  import CollectionMenu from './CollectionMenu.svelte';
  import { columnWidths, compactColumnWidths, dataColumns, tableMinWidth } from './columns';
  import type { TrackPages } from './track-pages.svelte';

  interface Props {
    pages: TrackPages;
    /** The place in library order of the selected track. */
    selected?: number | null;
    previewTrackId?: number | null;
    previewPlaying?: boolean;
    collections?: LibraryCollection[];
    activeCollection?: LibraryCollection | null;
    /** Enter or a double click on a row. */
    onopen?: () => void;
    onpreview?: (track: LibraryTrack) => void;
    ontogglefavourite?: (track: LibraryTrack) => void;
    onsort?: (sort: TrackSortField, direction: SortDirection) => void;
    onaddtocollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onremovefromcollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    oncreatecollectionandadd?: (name: string, trackIds: number[]) => Promise<void> | void;
  }

  const columnSortField: Record<string, TrackSortField> = {
    title: 'title',
    artist: 'artist',
    bpm: 'bpm',
    key: 'key',
    time: 'duration',
  };

  let {
    pages,
    selected = $bindable(null),
    previewTrackId = null,
    previewPlaying = false,
    collections = [],
    activeCollection = null,
    onopen,
    onpreview,
    ontogglefavourite,
    onsort,
    onaddtocollection,
    onremovefromcollection,
    oncreatecollectionandadd,
  }: Props = $props();

  // Rows drawn beyond the edges of the view, so a fast scroll does not open a gap.
  const OVERSCAN = 8;
  const INITIAL_ROWS = 8;

  const id = $props.id();
  const checkedIds = new SvelteSet<number>();
  const checkedFallback = new SvelteMap<number, LibraryTrack>();
  let gridEl: HTMLDivElement | undefined = $state();
  let viewport: HTMLDivElement | undefined = $state();
  let head: HTMLDivElement | undefined = $state();
  let rowHeight = $state(33);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let headHeight = $state(0);
  let overscan = $state(0);

  let menuOpen = $state(false);
  let menuAnchor = $state<HTMLElement | null>(null);
  let menuTracks = $state<LibraryTrack[]>([]);
  let menuShowFavourite = $state(false);
  let menuLabel = $state('Track actions');

  const allLoadedChecked = $derived(pages.allLoadedChecked(checkedIds));
  const someChecked = $derived(checkedIds.size > 0);
  const resolvedMenuTracks = $derived(
    menuTracks.map((track) => pages.trackById(track.id) ?? track),
  );

  // The header sticks to the top of the scrolling area, so the rows show below it.
  const rowsHeight = $derived(Math.max(0, viewportHeight - headHeight));
  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const shown = $derived(
    Math.max(
      0,
      Math.min(
        pages.total - first,
        viewportHeight > 0 ? Math.ceil(rowsHeight / rowHeight) + 2 * overscan : INITIAL_ROWS,
      ),
    ),
  );
  const rowsInView = $derived(Math.max(1, Math.floor(rowsHeight / rowHeight)));

  // Reading sizes inside ResizeObserver waits until Blink has laid out the table, so mounting it
  // forces no synchronous style or layout pass.
  $effect(() => {
    if (!viewport || !head) return;
    let rowHeightRead = false;
    const sized = new ResizeObserver((entries) => {
      for (const entry of entries) {
        if (entry.target === viewport) viewportHeight = entry.contentRect.height;
        else if (entry.target === head) {
          headHeight = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;
        }
      }
      if (!rowHeightRead && viewport) {
        rowHeightRead = true;
        const height = parseFloat(getComputedStyle(viewport).getPropertyValue('--row-height'));
        if (height > 0) rowHeight = height;
      }
    });
    sized.observe(viewport);
    sized.observe(head);
    return () => sized.disconnect();
  });

  $effect(() => pages.want(first, first + shown));

  // The cold-start budget runs from launch to this mark, and the off-screen overscan rows fill
  // in the next task once the visible slice is ready.
  let filledOnce = false;
  let overscanTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    if (filledOnce || viewportHeight === 0 || shown === 0 || !pages.track(first)) return;
    filledOnce = true;
    performance.mark('tracks-filled');
    overscanTimer = setTimeout(() => {
      overscan = OVERSCAN;
    });
  });
  $effect(() => () => clearTimeout(overscanTimer));

  $effect(() => {
    if (selected !== null) reveal(selected);
  });

  let lastScrollReset = untrack(() => pages.scrollResetCount);
  $effect(() => {
    const resetCount = pages.scrollResetCount;
    if (resetCount !== lastScrollReset) {
      lastScrollReset = resetCount;
      scrollTop = 0;
      if (viewport) viewport.scrollTop = 0;
    }
  });

  let lastCollectionId = untrack(() => pages.query.collection_id ?? null);
  $effect(() => {
    const nextCollection = pages.query.collection_id ?? null;
    if (nextCollection !== lastCollectionId) {
      lastCollectionId = nextCollection;
      clearChecked();
      menuOpen = false;
    }
  });

  const activeSort = $derived<TrackSortField>(pages.query.sort ?? 'index');
  const activeDirection = $derived<SortDirection>(pages.query.direction ?? 'asc');

  function headerSort(field: TrackSortField): 'ascending' | 'descending' | 'none' {
    if (field === 'index') {
      return activeSort === 'index' && activeDirection === 'desc' ? 'descending' : 'none';
    }
    if (activeSort !== field) return 'none';
    return activeDirection === 'desc' ? 'descending' : 'ascending';
  }

  function cycleSort(field: TrackSortField) {
    if (field === 'index') {
      if (activeSort === 'index' && activeDirection === 'asc') {
        onsort?.('index', 'desc');
      } else {
        onsort?.('index', 'asc');
      }
      return;
    }
    if (activeSort !== field) {
      onsort?.(field, 'asc');
    } else if (activeDirection === 'asc') {
      onsort?.(field, 'desc');
    } else {
      onsort?.('index', 'asc');
    }
  }

  function reveal(index: number) {
    if (!viewport) return;
    const top = index * rowHeight;
    const visible = viewport.clientHeight - headHeight;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + rowHeight > viewport.scrollTop + visible)
      viewport.scrollTop = top + rowHeight - visible;
  }

  function select(index: number) {
    if (pages.total > 0) selected = Math.min(Math.max(index, 0), pages.total - 1);
  }

  function toggleChecked(track: LibraryTrack) {
    if (checkedIds.has(track.id)) {
      checkedIds.delete(track.id);
      checkedFallback.delete(track.id);
    } else {
      checkedIds.add(track.id);
      checkedFallback.set(track.id, track);
    }
  }

  function clearChecked() {
    checkedIds.clear();
    checkedFallback.clear();
  }

  function resolveCheckedTracks(): LibraryTrack[] {
    const out: LibraryTrack[] = [];
    for (const trackId of checkedIds) {
      const live = pages.trackById(trackId) ?? checkedFallback.get(trackId);
      if (live) out.push(live);
    }
    return out;
  }

  function toggleHeaderChecked() {
    if (checkedIds.size > 0) {
      clearChecked();
      return;
    }
    for (const track of pages.loadedTracks()) {
      checkedIds.add(track.id);
      checkedFallback.set(track.id, track);
    }
  }

  function openMenuForTracks(
    tracks: LibraryTrack[],
    anchor: HTMLElement | null,
    options: { showFavourite: boolean; label: string },
  ) {
    if (tracks.length === 0) return;
    menuTracks = tracks;
    menuAnchor = anchor;
    menuShowFavourite = options.showFavourite;
    menuLabel = options.label;
    menuOpen = true;
  }

  function closeMenu() {
    const anchor = menuAnchor;
    menuOpen = false;
    if (anchor?.isConnected && anchor !== gridEl && anchor.tabIndex >= 0) {
      anchor.focus();
    } else {
      gridEl?.focus();
    }
  }

  async function removeTracksFromCollection(collectionId: number, trackIds: number[]) {
    await onremovefromcollection?.(collectionId, trackIds);
    if (activeCollection?.id === collectionId) {
      for (const trackId of trackIds) {
        checkedIds.delete(trackId);
        checkedFallback.delete(trackId);
      }
    }
  }

  function targetTracksForAction(fallbackTrack?: LibraryTrack): LibraryTrack[] {
    if (fallbackTrack) {
      if (checkedIds.size > 1 && checkedIds.has(fallbackTrack.id)) {
        return resolveCheckedTracks();
      }
      return [fallbackTrack];
    }
    if (checkedIds.size > 0) return resolveCheckedTracks();
    if (selected !== null) {
      const track = pages.track(selected);
      if (track) return [track];
    }
    return [];
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (menuOpen) return;
    const at = selected ?? -1;
    const moves: Record<string, number> = {
      ArrowDown: at + 1,
      ArrowUp: at - 1,
      PageDown: at + rowsInView,
      PageUp: at - rowsInView,
      Home: 0,
      End: pages.total - 1,
    };
    const to = moves[event.key];
    if (to !== undefined) {
      event.preventDefault();
      if (event.shiftKey && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
        const start = selected ?? 0;
        const clampedTo = pages.total > 0 ? Math.min(Math.max(to, 0), pages.total - 1) : 0;
        for (let idx = Math.min(start, clampedTo); idx <= Math.max(start, clampedTo); idx += 1) {
          const item = pages.track(idx);
          if (item) {
            checkedIds.add(item.id);
            checkedFallback.set(item.id, item);
          }
        }
      }
      select(to);
    } else if (event.key === 'Enter' && selected !== null) {
      event.preventDefault();
      onopen?.();
    } else if (event.key.toLowerCase() === 'x' && selected !== null) {
      const track = pages.track(selected);
      if (track) {
        event.preventDefault();
        toggleChecked(track);
      }
    } else if (event.key.toLowerCase() === 'f' && selected !== null) {
      const track = pages.track(selected);
      if (track && ontogglefavourite) {
        event.preventDefault();
        ontogglefavourite(track);
      }
    } else if (event.key.toLowerCase() === 'c') {
      const targets = targetTracksForAction();
      if (targets.length > 0) {
        event.preventDefault();
        const anchor =
          (selected !== null ? document.getElementById(`${id}-row-${selected}`) : null) ??
          gridEl ??
          null;
        openMenuForTracks(targets, anchor, {
          showFavourite: false,
          label: 'Add to collection',
        });
      }
    } else if ((event.key === 'Delete' || event.key === 'Backspace') && activeCollection != null) {
      const targets = targetTracksForAction();
      if (targets.length > 0) {
        event.preventDefault();
        void removeTracksFromCollection(
          activeCollection.id,
          targets.map((track) => track.id),
        );
      }
    } else if (event.key === 'Escape' && checkedIds.size > 0) {
      event.preventDefault();
      clearChecked();
    }
  }
</script>

{#snippet headerButton(label: string, field: TrackSortField, numeric = false)}
  {@const sortState = headerSort(field)}
  <button
    type="button"
    class={['sort-btn', { numeric, sorted: sortState !== 'none' }]}
    onclick={() => cycleSort(field)}
  >
    {#if sortState !== 'none'}
      <span class="sort-arrow">
        <Icon name={sortState === 'ascending' ? 'chevron-up' : 'chevron-down'} size={11} />
      </span>
    {/if}
    <span>{label}</span>
  </button>
{/snippet}

<div
  bind:this={gridEl}
  class={['table', { 'has-selection': someChecked }]}
  role="grid"
  tabindex="0"
  aria-label="Tracks"
  aria-rowcount={pages.total + 1}
  aria-activedescendant={selected !== null && selected >= first && selected < first + shown
    ? `${id}-row-${selected}`
    : undefined}
  style:--columns={columnWidths}
  style:--columns-compact={compactColumnWidths}
  style:--table-width={tableMinWidth}
  {onkeydown}
>
  {#if someChecked}
    <div class="selection-bar" role="region" aria-label="Selected tracks">
      <span class="selection-count numeric">{checkedIds.size} selected</span>
      <div class="selection-actions">
        <Button
          size="sm"
          onclick={(event) =>
            openMenuForTracks(resolveCheckedTracks(), event.currentTarget, {
              showFavourite: false,
              label: 'Add selected tracks to collection',
            })}
        >
          Add to collection
        </Button>
        {#if activeCollection}
          <Button
            size="sm"
            onclick={() =>
              void removeTracksFromCollection(activeCollection.id, Array.from(checkedIds))}
          >
            Remove from {activeCollection.name}
          </Button>
        {/if}
        <Button variant="quiet" size="sm" onclick={clearChecked}>Clear</Button>
      </div>
    </div>
  {/if}
  <div
    class="viewport"
    bind:this={viewport}
    onscroll={() => (scrollTop = viewport?.scrollTop ?? 0)}
  >
    <div class="head" role="rowgroup" bind:this={head}>
      <TableRow header aria-rowindex={1} style="grid-template-columns: var(--active-columns)">
        <TableCell header compact>
          <button
            type="button"
            class={['tick', 'header-tick', { filled: someChecked }]}
            role="checkbox"
            aria-label={someChecked ? 'Clear ticked tracks' : 'Tick all loaded tracks'}
            aria-checked={allLoadedChecked ? 'true' : someChecked ? 'mixed' : 'false'}
            tabindex={-1}
            onclick={toggleHeaderChecked}
          >
            {#if allLoadedChecked}
              <Icon name="check" size="xs" />
            {:else if someChecked}
              <span class="tick-dash" aria-hidden="true"></span>
            {/if}
          </button>
        </TableCell>
        <TableCell header compact><span class="visually-hidden">Status</span></TableCell>
        <TableCell header numeric ariaSort={headerSort('index')}>
          {@render headerButton('#', 'index', true)}
        </TableCell>
        {#each dataColumns as column (column.key)}
          {@const field = columnSortField[column.key]}
          <TableCell
            header
            numeric={column.numeric}
            ariaSort={field ? headerSort(field) : undefined}
          >
            {#if field}
              {@render headerButton(column.label, field, column.numeric)}
            {:else}
              {column.label}
            {/if}
          </TableCell>
        {/each}
        <TableCell header compact>
          <span class="visually-hidden">Actions</span>
        </TableCell>
      </TableRow>
    </div>
    <div class="body" role="rowgroup" style:height="{pages.total * rowHeight}px">
      <div class="rows" style:transform="translateY({first * rowHeight}px)">
        {#each { length: shown }, offset}
          {@const index = first + offset}
          {@const track = pages.track(index)}
          {@const isSelected = index === selected}
          {@const isChecked = track ? checkedIds.has(track.id) : false}
          {@const isPreviewTrack = Boolean(track && previewTrackId === track.id)}
          {@const isPreviewPlaying = isPreviewTrack && previewPlaying}
          <TableRow
            id="{id}-row-{index}"
            aria-rowindex={index + 2}
            selected={isSelected}
            style="grid-template-columns: var(--active-columns)"
            onclick={() => select(index)}
            ondblclick={() => {
              select(index);
              if (track && onpreview) onpreview(track);
              else onopen?.();
            }}
          >
            <!-- The cells stay while a row waits for its page, so a jump changes text and does
                 not rebuild them. -->
            <TableCell compact>
              <button
                type="button"
                class={['tick', { filled: isSelected || isChecked }]}
                role="checkbox"
                aria-label={track ? `Tick ${track.title}` : 'Tick row'}
                aria-checked={isChecked}
                tabindex={-1}
                disabled={!track}
                onclick={(event) => {
                  event.stopPropagation();
                  if (track) toggleChecked(track);
                }}
                ondblclick={(event) => event.stopPropagation()}
              >
                {#if isSelected || isChecked}
                  <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true">
                    <path
                      d="M2.2 6.2 4.8 8.8 9.8 3.4"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.6"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    />
                  </svg>
                {/if}
              </button>
            </TableCell>
            <TableCell compact>
              <button
                type="button"
                class={[
                  'status-cell',
                  {
                    shown: Boolean(track) && (isSelected || isPreviewTrack),
                    playing: isPreviewPlaying,
                  },
                ]}
                aria-label={track
                  ? isPreviewPlaying
                    ? `Pause ${track.title}`
                    : `Play ${track.title}`
                  : 'Play track'}
                aria-pressed={isPreviewPlaying}
                disabled={!track}
                tabindex={-1}
                onclick={(event) => {
                  event.stopPropagation();
                  if (track) {
                    select(index);
                    onpreview?.(track);
                  }
                }}
                ondblclick={(event) => event.stopPropagation()}
              >
                <span class="status-icon" aria-hidden="true">
                  <Icon name={isPreviewPlaying ? 'pause' : 'play'} size={13} filled />
                </span>
              </button>
            </TableCell>
            <TableCell numeric size="sm" tone="muted">
              {track ? index + 1 : ''}
            </TableCell>
            {#each dataColumns as column (column.key)}
              <TableCell
                numeric={column.numeric}
                mono={column.mono}
                tone={column.tone}
                weight={column.weight}
              >
                {track ? column.text(track) : ''}
              </TableCell>
            {/each}
            <TableCell compact>
              <div class="row-actions">
                <button
                  type="button"
                  class={['row-star', { fav: Boolean(track?.favourite) }]}
                  aria-label={track ? `Favourite ${track.title}` : 'Favourite track'}
                  aria-pressed={Boolean(track?.favourite)}
                  disabled={!track}
                  tabindex={-1}
                  onclick={(event) => {
                    event.stopPropagation();
                    if (track) ontogglefavourite?.(track);
                  }}
                  ondblclick={(event) => event.stopPropagation()}
                >
                  <Icon name="star" size="xs" filled={Boolean(track?.favourite)} />
                </button>
                <button
                  type="button"
                  class="more"
                  aria-label={track ? `Actions for ${track.title}` : 'Track actions'}
                  aria-haspopup="menu"
                  disabled={!track}
                  tabindex={-1}
                  onclick={(event) => {
                    event.stopPropagation();
                    if (track) {
                      const targets = targetTracksForAction(track);
                      openMenuForTracks(targets, event.currentTarget, {
                        showFavourite: targets.length === 1,
                        label: `Actions for ${track.title}`,
                      });
                    }
                  }}
                  ondblclick={(event) => event.stopPropagation()}
                >
                  <Icon name="more" size="xs" />
                </button>
              </div>
            </TableCell>
            {#if !track}
              <span class="pending" aria-label="Loading"></span>
            {/if}
          </TableRow>
        {/each}
      </div>
    </div>
  </div>
</div>

<CollectionMenu
  open={menuOpen && resolvedMenuTracks.length > 0}
  anchorEl={menuAnchor}
  label={menuLabel}
  {collections}
  selectedTracks={resolvedMenuTracks}
  {activeCollection}
  showFavourite={menuShowFavourite}
  onToggleFavourite={ontogglefavourite}
  onAdd={onaddtocollection}
  onRemove={removeTracksFromCollection}
  onCreateAndAdd={oncreatecollectionandadd}
  onClose={closeMenu}
/>

<style>
  .table {
    --active-columns: var(--columns);
    container: track-table / inline-size;
    display: grid;
    grid-template-rows: minmax(0, 1fr);
    height: 100%;
    min-height: 0;
    padding: 0 var(--space-5);
  }

  .table.has-selection {
    grid-template-rows: auto minmax(0, 1fr);
  }

  .selection-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-1-5) var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .selection-count {
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .selection-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .table:focus-visible {
    outline: none;
  }

  .table:focus-visible:not(:has([aria-selected='true'])) .viewport {
    border-radius: var(--radius-sm);
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: var(--focus-ring-offset);
  }

  .table:focus-visible :global(.row.selected) {
    outline-width: var(--focus-ring-width);
    outline-offset: var(--focus-ring-offset);
  }

  .viewport {
    height: 100%;
    overflow: auto;
    overscroll-behavior: contain;
  }

  .head,
  .body {
    min-width: var(--table-width);
  }

  .head {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--color-background);
  }

  /* What changes as the table scrolls is laid out and painted inside here alone. */
  .body {
    overflow: hidden;
    contain: strict;
  }

  .rows {
    will-change: transform;
  }

  .rows > :global([role='row']) {
    contain: layout paint;
  }

  .tick {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    padding: 0;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-background);
    cursor: pointer;
  }

  .tick.filled {
    border-color: var(--color-text);
    background: var(--color-text);
  }

  .header-tick {
    border-color: var(--color-border-control);
    cursor: pointer;
  }

  .tick-dash {
    width: var(--space-2);
    height: var(--selection-bar-width);
    background: var(--color-background);
  }

  .tick:disabled {
    cursor: default;
    opacity: 0.4;
  }

  .status-cell {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-xs);
    height: var(--control-height-xs);
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .status-cell:disabled {
    cursor: default;
  }

  .status-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--color-text);
    opacity: 0;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .status-cell.shown .status-icon,
  :global(.row:hover) .status-cell:not(:disabled) .status-icon,
  .status-cell:focus-visible:not(:disabled) .status-icon {
    opacity: 1;
  }

  :global(.row:hover) .status-cell:not(.shown, :disabled) .status-icon {
    color: var(--color-text-2);
  }

  .status-cell:hover:not(:disabled),
  .status-cell:focus-visible:not(:disabled) {
    background: var(--color-surface-2);
  }

  .status-cell:hover:not(:disabled) .status-icon {
    color: var(--color-text);
  }

  .sort-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    width: calc(100% + var(--space-4));
    height: var(--table-header-height);
    margin: 0 calc(var(--space-2) * -1);
    padding: 0 var(--space-2);
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .sort-btn:not(.numeric) {
    flex-direction: row-reverse;
    justify-content: flex-end;
  }

  .sort-btn.numeric {
    justify-content: flex-end;
  }

  .sort-btn:hover,
  .sort-btn.sorted {
    color: var(--color-text);
  }

  .sort-arrow {
    display: inline-flex;
    flex: 0 0 auto;
  }

  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: var(--space-0-5);
  }

  .row-star,
  .more {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-keybadge);
    height: var(--control-height-keybadge);
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out),
      opacity var(--duration-fast) var(--ease-out);
  }

  .row-star {
    opacity: 0;
  }

  .row-star.fav {
    color: var(--color-text);
    opacity: 1;
  }

  :global(.row:hover) .row-star:not(:disabled),
  .row-star:focus-visible:not(:disabled) {
    opacity: 1;
  }

  .row-star:hover:not(:disabled),
  .row-star:focus-visible:not(:disabled),
  .more:hover:not(:disabled),
  .more:focus-visible:not(:disabled) {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .row-star:disabled {
    cursor: default;
    opacity: 0;
  }

  .more:disabled {
    cursor: default;
    opacity: 0.4;
  }

  @container track-table (width < 600px) {
    .viewport {
      --active-columns: var(--columns-compact);
    }
  }

  @media (forced-colors: active) {
    .tick.filled {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }

    .status-cell.playing {
      border: var(--border-width) solid Highlight;
    }
  }

  /* A row's layout containment makes it the box this is placed in. */
  .pending {
    position: absolute;
    top: calc(50% - var(--space-1-5) / 2);
    right: var(--space-2);
    left: var(--space-2);
    height: var(--space-1-5);
    border-radius: var(--radius-xs);
    background: var(--color-surface-2);
  }
</style>
