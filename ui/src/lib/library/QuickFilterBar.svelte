<script lang="ts">
  import Icon from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { DescribedFilter } from '../ipc/bindings/DescribedFilter';
  import type { FilterSummary } from '../ipc/bindings/FilterSummary';
  import type { FilterTerm } from '../ipc/bindings/FilterTerm';
  import type { QuickFilterChange } from '../ipc/bindings/QuickFilterChange';
  import type { QuickFilters } from '../ipc/bindings/QuickFilters';
  import { sameTerm } from './filters.svelte';
  import FilterMenu, { type FilterMenuAnchor, type FilterMenuItem } from './FilterMenu.svelte';

  interface Props {
    /** The filters on the table, in order. */
    terms: FilterTerm[];
    /** How each of `terms` reads, once the core has said. */
    summaries: (FilterSummary | undefined)[];
    /** Tracks the whole view matches, which is the count of a filter on its own. */
    total: number;
    quick: QuickFilters;
    onadd: (term: FilterTerm) => void;
    onremove: (index: number) => void;
    onedit: () => void;
    onsave: () => void;
    onclear: () => void;
    onchange: (change: QuickFilterChange) => void;
    /** The hook for sending the filtered tracks to Prep, wired when Prep lands. */
    onsendtoprep?: () => void;
  }

  let {
    terms,
    summaries,
    total,
    quick,
    onadd,
    onremove,
    onedit,
    onsave,
    onclear,
    onchange,
    onsendtoprep,
  }: Props = $props();

  let menuOpen = $state(false);
  let menuAnchor = $state<FilterMenuAnchor | null>(null);
  let menuLabel = $state('');
  let menuItems = $state<FilterMenuItem[]>([]);
  let menuReturn = $state<HTMLElement | null>(null);

  const activeKeys = $derived(new Set(summaries.map((summary) => summary?.key)));
  const isActive = (filter: DescribedFilter) =>
    activeKeys.has(filter.key) || terms.some((term) => sameTerm(term, filter.term));
  const pinned = $derived(quick.pinned.filter((filter) => !isActive(filter)));
  const recommended = $derived(quick.recommended.filter((filter) => !isActive(filter)));
  const shown = $derived([...pinned, ...recommended]);

  function summaryOf(index: number): FilterSummary | undefined {
    return summaries[index];
  }

  function pinnedFilter(term: FilterTerm, key: string | undefined): DescribedFilter | undefined {
    return quick.pinned.find((filter) => filter.key === key || sameTerm(filter.term, term));
  }

  function openMenu(
    target: HTMLElement,
    label: string,
    items: FilterMenuItem[],
    at?: FilterMenuAnchor,
  ) {
    const rect = target.getBoundingClientRect();
    menuAnchor = at ?? { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom };
    menuLabel = label;
    menuItems = items;
    menuReturn = target;
    menuOpen = true;
  }

  function tokenMenu(event: MouseEvent, index: number, term: FilterTerm) {
    const summary = summaryOf(index);
    const pin = pinnedFilter(term, summary?.key);
    openMenu(event.currentTarget as HTMLElement, `Filter ${summary?.label ?? ''}`.trim(), [
      { label: 'Edit in the drawer', run: onedit },
      pin
        ? { label: 'Unpin from quick filters', run: () => onchange({ kind: 'remove', term }) }
        : { label: 'Pin to quick filters', run: () => onchange({ kind: 'pin', term }) },
      { label: 'Save as smart view', run: onsave },
      ...(onsendtoprep ? [{ label: 'Send these to Prep', run: onsendtoprep }] : []),
      { label: 'Remove', run: () => onremove(index) },
    ]);
  }

  /** The place a chip has among every quick filter, shown or not, for moves. */
  function placeOf(filter: DescribedFilter): number {
    const all = [...quick.pinned, ...quick.recommended];
    return all.findIndex((item) => item.key === filter.key);
  }

  function move(filter: DescribedFilter, step: -1 | 1) {
    const to = Math.max(0, placeOf(filter) + step);
    onchange({ kind: 'move', term: filter.term, to });
  }

  function chipMenu(target: HTMLElement, filter: DescribedFilter, at?: FilterMenuAnchor) {
    const place = shown.findIndex((item) => item.key === filter.key);
    const isPinned = quick.pinned.some((item) => item.key === filter.key);
    openMenu(
      target,
      `Quick filter ${filter.label}`,
      [
        { label: 'Move left', run: () => move(filter, -1), disabled: place <= 0 },
        {
          label: 'Move right',
          run: () => move(filter, 1),
          disabled: place >= shown.length - 1,
        },
        ...(isPinned
          ? []
          : [
              {
                label: 'Pin to quick filters',
                run: () => onchange({ kind: 'pin', term: filter.term }),
              },
            ]),
        { label: 'Remove', run: () => onchange({ kind: 'remove', term: filter.term }) },
      ],
      at,
    );
  }

  function onChipKey(event: KeyboardEvent, filter: DescribedFilter) {
    const target = event.currentTarget as HTMLElement;
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      event.preventDefault();
      chipMenu(target, filter);
    } else if (event.altKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
      event.preventDefault();
      move(filter, event.key === 'ArrowLeft' ? -1 : 1);
    }
  }
</script>

{#each terms as term, index (JSON.stringify(term))}
  {@const summary = summaryOf(index)}
  {@const count = terms.length === 1 ? total : (summary?.count ?? null)}
  <span class="token" role="group" aria-label="Filter: {summary?.label ?? 'loading'}">
    <button
      type="button"
      class="token-label"
      aria-haspopup="menu"
      onclick={(event) => tokenMenu(event, index, term)}
    >
      <span class="token-text">{summary?.label ?? '…'}</span>
      {#if count != null}
        <span class="token-count numeric">{count.toLocaleString('en-GB')}</span>
      {/if}
    </button>
    <button
      type="button"
      class="token-remove"
      aria-label="Remove filter {summary?.label ?? ''}"
      onclick={() => onremove(index)}
    >
      <Icon name="close" size={10} />
    </button>
  </span>
{/each}
{#if terms.length > 1}
  <button type="button" class="clear" onclick={onclear}>Clear filters</button>
{/if}

{#if shown.length > 0}
  {#if terms.length > 0}<span class="rule" aria-hidden="true"></span>{/if}
  {#each shown as filter (filter.key)}
    {@const isRecommended = !quick.pinned.some((item) => item.key === filter.key)}
    <Tooltip
      text={isRecommended ? 'Recommended from the filters you use' : 'Pinned quick filter'}
      side="below"
    >
      <button
        type="button"
        class={['chip', { recommended: isRecommended }]}
        aria-label={isRecommended ? `${filter.label}, recommended` : undefined}
        onclick={() => onadd(filter.term)}
        oncontextmenu={(event) => {
          event.preventDefault();
          chipMenu(event.currentTarget, filter, {
            left: event.clientX,
            right: event.clientX,
            top: event.clientY,
            bottom: event.clientY,
          });
        }}
        onkeydown={(event) => onChipKey(event, filter)}
      >
        {filter.label}
      </button>
    </Tooltip>
  {/each}
{/if}

<FilterMenu
  open={menuOpen}
  anchor={menuAnchor}
  label={menuLabel}
  items={menuItems}
  returnFocus={menuReturn}
  onclose={() => (menuOpen = false)}
/>

<style>
  .token {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    height: var(--control-height-sm);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .token-label,
  .token-remove,
  .chip,
  .clear {
    border: 0;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    cursor: pointer;
  }

  .token-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    max-width: 20rem;
    height: 100%;
    padding: 0 var(--space-2-5);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .token-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .token-count {
    color: var(--color-text-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
  }

  .token-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 100%;
    border-left: var(--border-width) solid var(--color-border);
    color: var(--color-text-2);
  }

  .token-remove:hover,
  .token-label:hover {
    background: var(--color-raised);
    color: var(--color-text);
  }

  .rule {
    flex: 0 0 auto;
    width: var(--border-width);
    height: var(--space-3-5);
    margin: 0 var(--space-1);
    background: var(--color-border);
  }

  .chip {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    height: var(--control-height-sm);
    padding: 0 var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    white-space: nowrap;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .chip:hover {
    border-color: var(--color-border-control);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .clear {
    flex: 0 0 auto;
    height: var(--control-height-sm);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--color-text-2);
    font-size: var(--text-sm);
    white-space: nowrap;
  }

  .clear:hover {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  @media (forced-colors: active) {
    .token,
    .chip {
      border-color: CanvasText;
    }

    .rule {
      background: CanvasText;
    }
  }
</style>
