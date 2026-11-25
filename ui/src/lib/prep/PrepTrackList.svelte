<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from '../design/Icon.svelte';
  import Thumb from '../design/Thumb.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { PrepItem } from '../ipc/bindings/PrepItem';
  import { LIST_COLUMNS, missingNow, rowStatus, type ListColumn, type ListSort } from './prep-view';

  /** Columns whose gaps the row already shows as an empty cell. */
  const COLUMN_GAPS = new Set(['genre', 'label', 'year']);
  // Rows beyond the visible ones that are drawn and fetched, so a short scroll shows no gaps.
  const OVERSCAN = 15;

  interface Props {
    order: readonly number[];
    byId: Readonly<Record<number, PrepItem>>;
    activeId: number | null;
    sort: ListSort | null;
    tickedCount: number;
    /** Bumped whenever the ticks change, so rows read `isTicked` again. */
    tickVersion: number;
    isTicked: (id: number) => boolean;
    searchingIds: ReadonlySet<number>;
    searching: boolean;
    loading: boolean;
    onsort: (column: ListColumn) => void;
    ontickall: () => void;
    ontick: (id: number) => void;
    onopen: (item: PrepItem) => void;
    /** The tracks the list is drawing now, so the ones not loaded yet can be fetched. */
    onwindow: (ids: number[]) => void;
  }
  let {
    order,
    byId,
    activeId,
    sort,
    tickedCount,
    tickVersion,
    isTicked,
    searchingIds,
    searching,
    loading,
    onsort,
    ontickall,
    ontick,
    onopen,
    onwindow,
  }: Props = $props();

  let scroller = $state<HTMLElement>();
  let scrollTop = $state(0);
  let viewHeight = $state(0);
  let rowHeight = $state(56);
  const total = $derived(order.length);
  const windowStart = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN));
  const windowEnd = $derived(
    Math.min(total, Math.ceil((scrollTop + viewHeight) / rowHeight) + OVERSCAN),
  );
  const visibleIds = $derived(order.slice(windowStart, windowEnd));

  $effect(() => {
    const ids = visibleIds;
    untrack(() => onwindow(ids));
  });

  // The row height comes from the stylesheet; reading it keeps the drawn window lined up.
  $effect(() => {
    void byId;
    const row = scroller?.querySelector<HTMLElement>('.row');
    if (row && row.offsetHeight > 0 && row.offsetHeight !== untrack(() => rowHeight)) {
      rowHeight = row.offsetHeight;
    }
  });

  function ticked(id: number): boolean {
    void tickVersion;
    return isTicked(id);
  }

  /** The positions drawn now, with at least a screenful when the list has not measured yet. */
  export function range(): { start: number; end: number } {
    return { start: windowStart, end: Math.max(windowEnd, windowStart + 40) };
  }

  /** Scrolls just enough to show the track at this position. */
  export function reveal(position: number) {
    const list = scroller;
    if (!list || position < 0) return;
    const top = position * rowHeight;
    if (top < list.scrollTop) list.scrollTop = top;
    else if (top + rowHeight > list.scrollTop + list.clientHeight) {
      list.scrollTop = top + rowHeight - list.clientHeight;
    }
    scrollTop = list.scrollTop;
  }
</script>

<section class="tracks" aria-label="Track list">
  <div class="list-head">
    <input
      type="checkbox"
      aria-label={tickedCount === total ? 'Deselect all tracks' : 'Select all tracks'}
      title="Tick every track in the list (Ctrl+A)"
      checked={tickedCount === total && total > 0}
      indeterminate={tickedCount > 0 && tickedCount < total}
      onchange={ontickall}
    />
    <div class="columns">
      {#each LIST_COLUMNS as column (column.id)}
        {@const sorted = sort?.column === column.id ? sort.direction : null}
        <button
          type="button"
          class="column"
          data-column={column.id}
          aria-label={'Sort by ' +
            column.label.toLowerCase() +
            (sorted === 'asc' ? ', A to Z' : sorted === 'desc' ? ', Z to A' : '')}
          data-sorted={sorted ?? undefined}
          onclick={() => onsort(column.id)}
          >{column.label}{#if column.id === 'title'}<span class="numeric count"
              >{total.toLocaleString('en-GB')}</span
            >{/if}{#if sorted}<Icon
              name={sorted === 'asc' ? 'chevron-up' : 'chevron-down'}
              size="xs"
            />{/if}</button
        >
      {/each}
    </div>
  </div>
  <div
    class="list-scroll"
    bind:this={scroller}
    bind:clientHeight={viewHeight}
    onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  >
    <ul
      class="rows"
      aria-busy={loading}
      style:padding-top={windowStart * rowHeight + 'px'}
      style:padding-bottom={(total - windowEnd) * rowHeight + 'px'}
    >
      {#each visibleIds as id (id)}
        {@const item = byId[id]}
        {#if !item}
          <li class="row placeholder" aria-hidden="true" style:height={rowHeight + 'px'}></li>
        {:else}
          {@const status = rowStatus(item)}
          {@const gaps = missingNow(item)}
          {@const isSearching = searching && searchingIds.has(item.track.id)}
          {@const art = item.artwork.proposed?.artwork ?? item.artwork.artwork}
          <li
            class="row"
            class:active={item.track.id === activeId}
            data-active={item.track.id === activeId}
          >
            <input
              type="checkbox"
              aria-label={'Tick ' + item.track.title}
              checked={ticked(item.track.id)}
              onchange={() => ontick(item.track.id)}
            />
            <button
              type="button"
              class="track-name"
              aria-pressed={item.track.id === activeId}
              onclick={() => onopen(item)}
              title={item.track.location}
            >
              <Thumb seed={item.track.id} size="sm" src={art ? artworkUrl(art) : null} alt="" />
              <span class="names">
                <span class="title">{item.track.title}</span>
                <span class="artist"
                  >{#if item.track.artist}{item.track.artist}{/if}{#if gaps.length}<span
                      class="gaps">{item.track.artist ? ' · ' : ''}Missing {gaps.join(', ')}</span
                    >{#if gaps.some((gap) => !COLUMN_GAPS.has(gap))}<span class="gaps-wide"
                        >{item.track.artist ? ' · ' : ''}Missing {gaps
                          .filter((gap) => !COLUMN_GAPS.has(gap))
                          .join(', ')}</span
                      >{/if}{/if}</span
                >
              </span>
              {#each ['genre', 'label', 'year'] as const as column (column)}
                {@const value = item.track[column]}
                <span
                  class="cell"
                  data-column={column}
                  class:numeric={column === 'year'}
                  class:empty-cell={!value}
                  title={value ?? undefined}
                  >{#if value}{value}{:else}<span aria-label={'No ' + column}>–</span>{/if}</span
                >
              {/each}
              <span class="status" data-stage={isSearching ? 'searching' : item.stage}>
                {#if isSearching && item.stage === 'not_searched'}
                  <span class="status-main">Searching</span>
                {:else}
                  <span class="status-main">{status.primary}</span>
                  {#if status.secondary.length}<span class="status-sub"
                      >{#each status.secondary as part, index (index)}{index > 0
                          ? ' · '
                          : ''}{#if part.number}<span class="numeric">{part.number}</span
                          >{part.words ? ' ' + part.words : ''}{:else}{part.words}{/if}{/each}</span
                    >{/if}
                {/if}
              </span>
            </button>
          </li>
        {/if}
      {/each}
    </ul>
  </div>
</section>

<style>
  .tracks {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
    container: tracks / inline-size;
    --col-genre: 96px;
    --col-label: 120px;
    --col-year: 40px;
    --col-status: 120px;
    --row-columns: var(--thumb-sm) minmax(0, 1fr) var(--col-genre) var(--col-label) var(--col-year)
      var(--col-status);
  }
  input[type='checkbox'] {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    accent-color: var(--color-text);
    display: block;
    margin: 0;
    cursor: pointer;
  }
  .list-head {
    display: grid;
    grid-template-columns: var(--checkbox-size) minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
    height: var(--table-header-height);
    padding-left: var(--space-4);
    color: var(--color-text-2);
    font-size: var(--text-sm);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .columns {
    min-width: 0;
    height: 100%;
    display: grid;
    grid-template-columns: var(--row-columns);
    align-items: center;
    gap: var(--space-3);
    padding-right: var(--space-4);
  }
  .column {
    min-width: 0;
    height: 100%;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    padding: 0;
    font: inherit;
    color: inherit;
    background: transparent;
    border: 0;
    cursor: pointer;
    white-space: nowrap;
    transition: color var(--duration-fast) var(--ease-out);
  }
  .column:hover,
  .column[data-sorted] {
    color: var(--color-text);
  }
  .column:focus-visible {
    outline: var(--focus-ring-width) solid var(--color-text);
    outline-offset: -2px;
  }
  .column[data-column='title'] {
    grid-column: 1 / 3;
  }
  .column[data-column='year'],
  .column[data-column='status'] {
    justify-content: end;
  }
  .count {
    color: var(--color-muted);
  }
  .cell {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .cell[data-column='year'] {
    text-align: right;
  }
  .empty-cell,
  .gaps,
  .gaps-wide {
    color: var(--color-muted);
  }
  .gaps {
    display: none;
  }
  @container tracks (max-width: 640px) {
    .columns,
    .track-name {
      --row-columns: var(--thumb-sm) minmax(0, 1fr) var(--col-genre) var(--col-year)
        var(--col-status);
    }
    [data-column='label'] {
      display: none;
    }
  }
  @container tracks (max-width: 520px) {
    .columns,
    .track-name {
      --row-columns: var(--thumb-sm) minmax(0, 1fr) var(--col-status);
    }
    [data-column='genre'],
    [data-column='year'],
    .gaps-wide {
      display: none;
    }
    .gaps {
      display: inline;
    }
  }
  .list-scroll {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    /* Rows are swapped as the list scrolls; anchoring would nudge the position each time. */
    overflow-anchor: none;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    position: relative;
    box-sizing: border-box;
    /* Every row is the same height, so the list can place rows it has not drawn. */
    height: calc(var(--thumb-sm) + var(--space-5));
    display: grid;
    grid-template-columns: var(--checkbox-size) minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
    padding-left: var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
    transition: background-color var(--duration-fast) var(--ease-out);
  }
  .row:hover {
    background: var(--color-surface-2);
  }
  .row.active {
    background: var(--color-raised);
  }
  .row.active::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 2px;
    background: var(--color-accent);
  }
  .track-name {
    min-width: 0;
    display: grid;
    grid-template-columns: var(--row-columns);
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4) var(--space-2) 0;
    font: inherit;
    text-align: left;
    color: inherit;
    background: transparent;
    border: 0;
    cursor: pointer;
  }
  .track-name:focus-visible {
    outline: var(--focus-ring-width) solid var(--color-text);
    outline-offset: -2px;
  }
  .names,
  .status {
    min-width: 0;
    display: grid;
    gap: var(--space-1);
  }
  .title,
  .artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-size: var(--text-md);
  }
  .artist,
  .status {
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .status {
    justify-items: end;
    text-align: right;
  }
  .status-main {
    display: inline-flex;
    gap: var(--space-2);
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
  }
  .status[data-stage='review'] .status-main,
  .status[data-stage='searching'] {
    color: var(--color-text);
  }
  .status[data-stage='not_searched'],
  .status[data-stage='done'],
  .status-sub {
    color: var(--color-muted);
  }
  .status-sub {
    white-space: nowrap;
  }
  .placeholder {
    display: block;
    border-bottom: var(--border-width) solid var(--color-border);
  }
  @media (forced-colors: active) {
    .row.active::before {
      background: Highlight;
    }
  }
</style>
