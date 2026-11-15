<script lang="ts">
  import { untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';

  import Icon from '../design/Icon.svelte';
  import TableCell from '../design/TableCell.svelte';
  import TableRow from '../design/TableRow.svelte';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { SortDirection } from '../ipc/bindings/SortDirection';
  import type { TrackSortField } from '../ipc/bindings/TrackSortField';
  import { columnWidths, compactColumnWidths, dataColumns, tableMinWidth } from './columns';
  import type { TrackPages } from './track-pages.svelte';

  interface Props {
    pages: TrackPages;
    /** The place in library order of the selected track. */
    selected?: number | null;
    /** Enter or a double click on a row. */
    onopen?: () => void;
    ontogglefavourite?: (track: LibraryTrack) => void;
    onsort?: (sort: TrackSortField, direction: SortDirection) => void;
  }

  const columnSortField: Record<string, TrackSortField> = {
    title: 'title',
    artist: 'artist',
    bpm: 'bpm',
    key: 'key',
    time: 'duration',
  };

  let { pages, selected = $bindable(null), onopen, ontogglefavourite, onsort }: Props = $props();

  // Rows drawn beyond the edges of the view, so a fast scroll does not open a gap.
  const OVERSCAN = 8;
  const INITIAL_ROWS = 8;

  const id = $props.id();
  const checkedIds = new SvelteSet<number>();
  let viewport: HTMLDivElement | undefined = $state();
  let head: HTMLDivElement | undefined = $state();
  let rowHeight = $state(33);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let headHeight = $state(0);
  let overscan = $state(0);

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

  function toggleChecked(trackId: number) {
    if (checkedIds.has(trackId)) checkedIds.delete(trackId);
    else checkedIds.add(trackId);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
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
      select(to);
    } else if (event.key === 'Enter' && selected !== null) {
      event.preventDefault();
      onopen?.();
    } else if (event.key.toLowerCase() === 'x' && selected !== null) {
      const track = pages.track(selected);
      if (track) {
        event.preventDefault();
        toggleChecked(track.id);
      }
    } else if (event.key.toLowerCase() === 'f' && selected !== null) {
      const track = pages.track(selected);
      if (track && ontogglefavourite) {
        event.preventDefault();
        ontogglefavourite(track);
      }
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
  class="table"
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
  <div
    class="viewport"
    bind:this={viewport}
    onscroll={() => (scrollTop = viewport?.scrollTop ?? 0)}
  >
    <div class="head" role="rowgroup" bind:this={head}>
      <TableRow header aria-rowindex={1} style="grid-template-columns: var(--active-columns)">
        <TableCell header compact>
          <span
            class="tick header-tick"
            aria-label="Tick all tracks"
            aria-disabled="true"
            title="Batch selection arrives in M2"
          ></span>
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
          <TableRow
            id="{id}-row-{index}"
            aria-rowindex={index + 2}
            selected={isSelected}
            style="grid-template-columns: var(--active-columns)"
            onclick={() => select(index)}
            ondblclick={() => {
              select(index);
              onopen?.();
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
                  if (track) toggleChecked(track.id);
                }}
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
              <div class="status-cell">
                {#if isSelected}
                  <svg
                    class="status-icon"
                    viewBox="0 0 14 14"
                    width="14"
                    height="14"
                    aria-hidden="true"
                  >
                    <path d="M3.5 2.4 11.5 7 3.5 11.6Z" fill="currentColor" />
                  </svg>
                {/if}
                {#if track}
                  <button
                    type="button"
                    class={[
                      'status-star',
                      { fav: track.favourite, hiddenOnSelected: isSelected && !track.favourite },
                    ]}
                    aria-label={`Favourite ${track.title}`}
                    aria-pressed={track.favourite}
                    tabindex={-1}
                    onclick={(event) => {
                      event.stopPropagation();
                      ontogglefavourite?.(track);
                    }}
                  >
                    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                      <path
                        d="m8 1.8 1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6Z"
                        fill={track.favourite ? 'currentColor' : 'none'}
                        stroke="currentColor"
                        stroke-width="1.35"
                        stroke-linejoin="round"
                      />
                    </svg>
                  </button>
                {/if}
              </div>
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
              <button
                type="button"
                class="more"
                aria-label="Track actions"
                aria-disabled="true"
                tabindex={-1}
                title="Track actions arrive in M2"
              >
                <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
                  <circle cx="3.5" cy="8" r="1.2" fill="currentColor" />
                  <circle cx="8" cy="8" r="1.2" fill="currentColor" />
                  <circle cx="12.5" cy="8" r="1.2" fill="currentColor" />
                </svg>
              </button>
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

<style>
  .table {
    --active-columns: var(--columns);
    container: track-table / inline-size;
    height: 100%;
    min-height: 0;
    padding: 0 var(--space-5);
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
    border-color: var(--color-border);
    cursor: default;
  }

  .tick:disabled {
    cursor: default;
    opacity: 0.4;
  }

  .status-cell {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height-xs);
    height: var(--control-height-xs);
  }

  .status-icon {
    color: var(--color-text);
  }

  .status-star {
    position: absolute;
    inset: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    opacity: 0;
    cursor: pointer;
    transition:
      opacity var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .status-star.fav {
    color: var(--color-text);
    opacity: 1;
  }

  .status-star.hiddenOnSelected {
    opacity: 0;
  }

  :global(.row:hover) .status-star,
  .status-star:focus-visible {
    opacity: 1;
    color: var(--color-text);
  }

  :global(.row:hover) .status-icon {
    opacity: 0;
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
    cursor: default;
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
