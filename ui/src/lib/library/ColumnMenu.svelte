<script lang="ts">
  import { untrack } from 'svelte';

  import Icon from '../design/Icon.svelte';
  import type { FilterField } from '../ipc/bindings/FilterField';
  import type { LibraryTableLayout } from '../ipc/bindings/LibraryTableLayout';
  import type { TableColumn } from '../ipc/bindings/TableColumn';
  import type { TableColumnGroup } from '../ipc/bindings/TableColumnGroup';
  import type { TableLayoutChange } from '../ipc/bindings/TableLayoutChange';

  /** Where the menu opens: under a button, or at the pointer for a right click. */
  export type MenuAnchor = { left: number; right: number; top: number; bottom: number };

  interface Props {
    open: boolean;
    anchor: MenuAnchor | null;
    layout: LibraryTableLayout;
    /** Columns the table is too narrow to show right now. */
    hidden?: string[];
    /** The header the menu was opened on, offered first as a filter. */
    filterColumn?: FilterField | null;
    onfilter?: (column: string) => void;
    onchange: (change: TableLayoutChange) => void;
    onclose: () => void;
  }

  let {
    open,
    anchor,
    layout,
    hidden = [],
    filterColumn = null,
    onfilter,
    onchange,
    onclose,
  }: Props = $props();

  const groups: { id: TableColumnGroup; label: string }[] = [
    { id: 'track', label: 'Track' },
    { id: 'release', label: 'Release' },
    { id: 'audio', label: 'Audio' },
    { id: 'file', label: 'File' },
    { id: 'tags', label: 'Tags' },
  ];

  const ITEMS =
    '[role="menuitem"]:not([aria-disabled="true"]), [role="menuitemcheckbox"]:not([aria-disabled="true"])';

  let menuEl = $state<HTMLDivElement | undefined>();
  let searchEl = $state<HTMLInputElement | undefined>();
  let search = $state('');
  let pos = $state({ top: 0, left: 0 });

  const needle = $derived(search.trim().toLowerCase());
  const listed = $derived(
    groups
      .map((group) => ({
        ...group,
        columns: layout.available.filter(
          (column) =>
            column.group === group.id &&
            (needle === '' || column.label.toLowerCase().includes(needle)),
        ),
      }))
      .filter((group) => group.columns.length > 0),
  );

  function place() {
    if (!menuEl) return;
    const margin = 8;
    const width = menuEl.offsetWidth || 260;
    const height = menuEl.offsetHeight || 400;
    const vw = window.innerWidth || 1280;
    const vh = window.innerHeight || 800;
    if (!anchor) {
      pos = { top: margin, left: margin };
      return;
    }
    const left = Math.max(margin, Math.min(vw - width - margin, anchor.right - width));
    const below = anchor.bottom + 4;
    const top =
      below + height <= vh - margin
        ? below
        : Math.max(margin, Math.min(vh - height - margin, anchor.top - height - 4));
    pos = { top, left };
  }

  function items(): HTMLElement[] {
    return Array.from(menuEl?.querySelectorAll<HTMLElement>(ITEMS) ?? []);
  }

  $effect(() => {
    if (!open || !menuEl) {
      untrack(() => (search = ''));
      return;
    }
    try {
      menuEl.showPopover?.();
    } catch {
      // Without the Popover API the menu still shows in place.
    }
    place();
    untrack(() => items()[0]?.focus());

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && menuEl?.contains(target)) return;
      onclose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        onclose();
        return;
      }
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const inSearch = document.activeElement === searchEl;
      if (!inSearch && event.key.length === 1 && event.key !== ' ') {
        // Typing over the list searches it.
        searchEl?.focus();
        return;
      }
      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      if (inSearch && (event.key === 'Home' || event.key === 'End')) return;
      const list = items();
      if (list.length === 0) return;
      event.preventDefault();
      event.stopPropagation();
      const current = list.indexOf(document.activeElement as HTMLElement);
      let next: number;
      if (event.key === 'Home') next = 0;
      else if (event.key === 'End') next = list.length - 1;
      else if (event.key === 'ArrowDown') next = current < 0 ? 0 : (current + 1) % list.length;
      else next = current <= 0 ? list.length - 1 : current - 1;
      list[next]?.focus();
      list[next]?.scrollIntoView?.({ block: 'nearest' });
    };
    window.addEventListener('pointerdown', onPointerDown, true);
    window.addEventListener('keydown', onKeyDown, true);
    return () => {
      window.removeEventListener('pointerdown', onPointerDown, true);
      window.removeEventListener('keydown', onKeyDown, true);
      try {
        if (menuEl?.matches?.(':popover-open')) menuEl.hidePopover?.();
      } catch {
        // A detached menu has nothing to hide.
      }
    };
  });

  function toggle(column: TableColumn) {
    if (!column.removable) return;
    onchange({ kind: 'toggle', column: column.id });
  }
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="menu"
    role="menu"
    aria-label="Columns"
    popover="manual"
    style:top="{pos.top}px"
    style:left="{pos.left}px"
  >
    <div>
      {#if filterColumn && onfilter}
        <div class="filter">
          <button
            type="button"
            class="item"
            role="menuitem"
            tabindex={-1}
            onclick={() => onfilter(filterColumn.column)}
          >
            <span class="box blank" aria-hidden="true"><Icon name="sliders" size={11} /></span>
            <span class="name">Filter by {filterColumn.label}…</span>
          </button>
        </div>
      {/if}
      <div class="search">
        <Icon name="search" size="xs" />
        <input
          bind:this={searchEl}
          bind:value={search}
          type="search"
          placeholder="Find a column"
          aria-label="Find a column"
          onkeydown={(event) => {
            if (event.key === 'ArrowDown') {
              event.preventDefault();
              event.stopPropagation();
              items()[0]?.focus();
            }
          }}
        />
      </div>
    </div>
    <div class="groups">
      {#each listed as group (group.id)}
        <div class="group" role="group" aria-label={group.label}>
          <span class="group-label" aria-hidden="true">{group.label}</span>
          {#each group.columns as column (column.id)}
            {@const narrow = hidden.includes(column.id)}
            <button
              type="button"
              class="item"
              role="menuitemcheckbox"
              aria-checked={column.shown}
              aria-disabled={column.removable ? undefined : 'true'}
              tabindex={-1}
              onclick={() => toggle(column)}
            >
              <span class={['box', { ticked: column.shown }]} aria-hidden="true">
                {#if column.shown}<Icon name="check" size={11} />{/if}
              </span>
              <span class="name">{column.label}</span>
              {#if narrow}<span class="note">Hidden, too narrow</span>{/if}
            </button>
          {/each}
        </div>
      {:else}
        <p class="empty">No column matches</p>
      {/each}
    </div>
    <div class="footer">
      <button
        type="button"
        class="item"
        role="menuitemcheckbox"
        aria-checked={layout.wrap}
        tabindex={-1}
        onclick={() => onchange({ kind: 'set_wrap', wrap: !layout.wrap })}
      >
        <span class={['box', { ticked: layout.wrap }]} aria-hidden="true">
          {#if layout.wrap}<Icon name="check" size={11} />{/if}
        </span>
        <span class="name">Wrap long text</span>
      </button>
      <button
        type="button"
        class="item"
        role="menuitem"
        tabindex={-1}
        onclick={() => onchange({ kind: 'reset' })}
      >
        <span class="box blank" aria-hidden="true"></span>
        <span class="name">Reset to default</span>
      </button>
    </div>
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    inset: auto;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    width: 260px;
    max-height: min(520px, calc(100vh - var(--space-4)));
    margin: 0;
    padding: var(--space-1);
    overflow: hidden;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .filter {
    margin-bottom: var(--space-1);
    padding-bottom: var(--space-1);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .box.blank :global(svg) {
    color: var(--color-text-2);
  }

  .search {
    display: flex;
    align-items: center;
    gap: var(--space-1-5);
    height: var(--control-height-sm);
    margin-bottom: var(--space-1);
    padding: 0 var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
    color: var(--color-muted);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
  }

  .search input:focus-visible {
    outline: none;
  }

  .search:focus-within {
    border-bottom-color: var(--color-border-control);
  }

  .groups {
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .group + .group {
    margin-top: var(--space-1);
  }

  .group-label {
    display: block;
    padding: var(--space-1) var(--space-2) var(--space-0-5);
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: var(--control-height-sm);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .item:hover,
  .item:focus-visible {
    background: var(--color-surface-2);
  }

  .item:focus-visible {
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: calc(-1 * var(--focus-ring-width));
  }

  .item[aria-disabled='true'] {
    color: var(--color-text-2);
    cursor: default;
  }

  .box {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    color: var(--color-background);
  }

  .box.ticked {
    border-color: var(--color-text);
    background: var(--color-text);
  }

  .box.blank {
    border-color: transparent;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note {
    flex: 0 0 auto;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .empty {
    margin: 0;
    padding: var(--space-2);
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .footer {
    margin-top: var(--space-1);
    padding-top: var(--space-1);
    border-top: var(--border-width) solid var(--color-border);
  }

  @media (forced-colors: active) {
    .box.ticked {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }

    .item:focus-visible {
      outline-color: Highlight;
    }
  }
</style>
