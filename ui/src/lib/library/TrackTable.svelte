<script lang="ts">
  import TableCell from '../design/TableCell.svelte';
  import TableRow from '../design/TableRow.svelte';
  import { columns, columnWidths, tableWidth } from './columns';
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
  let viewport: HTMLDivElement | undefined = $state();
  let rowHeight = $state(30);
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
  style:--table-width={tableWidth}
  {onkeydown}
>
  <div
    class="viewport"
    bind:this={viewport}
    bind:clientHeight={viewportHeight}
    onscroll={() => (scrollTop = viewport?.scrollTop ?? 0)}
  >
    <div class="head" role="rowgroup" bind:offsetHeight={headHeight}>
      <TableRow aria-rowindex={1} style="grid-template-columns: var(--columns)">
        {#each columns as column (column.key)}
          <TableCell header numeric={column.numeric}>{column.label}</TableCell>
        {/each}
      </TableRow>
    </div>
    <div class="body" role="rowgroup" style:height="{pages.total * rowHeight}px">
      <div class="rows" style:transform="translateY({first * rowHeight}px)">
        {#each { length: shown }, offset}
          {@const index = first + offset}
          {@const track = pages.track(index)}
          <TableRow
            id="{id}-row-{index}"
            aria-rowindex={index + 2}
            selected={index === selected}
            style="grid-template-columns: var(--columns)"
            onclick={() => select(index)}
            ondblclick={() => {
              select(index);
              onopen?.();
            }}
          >
            <!-- The cells stay while a row waits for its page, so a jump changes text and does
                 not rebuild them. -->
            {#each columns as column (column.key)}
              <TableCell numeric={column.numeric}>{track ? column.text(track) : ''}</TableCell>
            {/each}
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
    height: 100%;
    min-height: 0;
  }

  .table:focus-visible {
    outline-offset: -2px;
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
    background: var(--color-surface);
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

  /* A row's layout containment makes it the box this is placed in. */
  .pending {
    position: absolute;
    top: calc(50% - 3px);
    right: var(--space-2);
    left: var(--space-2);
    height: 6px;
    border-radius: 3px;
    background: var(--color-surface-2);
  }
</style>
