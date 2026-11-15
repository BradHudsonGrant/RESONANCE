<script lang="ts">
  import { SvelteSet } from 'svelte/reactivity';

  import TableCell from '../design/TableCell.svelte';
  import TableRow from '../design/TableRow.svelte';
  import { columnWidths, compactColumnWidths, dataColumns, tableMinWidth } from './columns';
  import type { TrackPages } from './track-pages.svelte';

  interface Props {
    pages: TrackPages;
    /** The place in library order of the selected track. */
    selected?: number | null;
    /** Enter or a double click on a row. */
    onopen?: () => void;
  }

  let { pages, selected = $bindable(null), onopen }: Props = $props();

  // Rows drawn beyond the edges of the view, so a fast scroll does not open a gap.
  const OVERSCAN = 8;

  const id = $props.id();
  const checkedIds = new SvelteSet<number>();
  let viewport: HTMLDivElement | undefined = $state();
  let rowHeight = $state(33);
  let scrollTop = $state(0);
  let viewportHeight = $state(0);
  let headHeight = $state(0);

  // The header sticks to the top of the scrolling area, so the rows show below it.
  const rowsHeight = $derived(Math.max(0, viewportHeight - headHeight));
  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const shown = $derived(
    Math.max(0, Math.min(pages.total - first, Math.ceil(rowsHeight / rowHeight) + 2 * OVERSCAN)),
  );
  const rowsInView = $derived(Math.max(1, Math.floor(rowsHeight / rowHeight)));

  $effect(() => {
    if (!viewport) return;
    const height = parseFloat(getComputedStyle(viewport).getPropertyValue('--row-height'));
    if (height > 0) rowHeight = height;
  });

  $effect(() => pages.want(first, first + shown));

  // The cold-start budget runs from launch to this mark.
  let filledOnce = false;
  $effect(() => {
    if (filledOnce || shown === 0 || !pages.track(first)) return;
    filledOnce = true;
    performance.mark('tracks-filled');
  });

  $effect(() => {
    if (selected !== null) reveal(selected);
  });

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
    }
  }
</script>

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
    bind:clientHeight={viewportHeight}
    onscroll={() => (scrollTop = viewport?.scrollTop ?? 0)}
  >
    <div class="head" role="rowgroup" bind:offsetHeight={headHeight}>
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
        <TableCell header numeric>#</TableCell>
        {#each dataColumns as column (column.key)}
          <TableCell header numeric={column.numeric}>{column.label}</TableCell>
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

  .status-icon {
    color: var(--color-text);
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
