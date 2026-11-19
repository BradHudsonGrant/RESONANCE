<script lang="ts" module>
  export interface FilterMenuItem {
    label: string;
    run: () => void;
    disabled?: boolean;
  }

  /** Where the menu opens: under an element, or at the pointer for a right click. */
  export type FilterMenuAnchor = { left: number; right: number; top: number; bottom: number };
</script>

<script lang="ts">
  import { untrack } from 'svelte';

  interface Props {
    open: boolean;
    anchor: FilterMenuAnchor | null;
    label: string;
    items: FilterMenuItem[];
    /** Gets focus back when the menu closes. */
    returnFocus?: HTMLElement | null;
    onclose: () => void;
  }

  let { open, anchor, label, items, returnFocus = null, onclose }: Props = $props();

  const ITEMS = '[role="menuitem"]:not([aria-disabled="true"])';

  let menuEl = $state<HTMLDivElement | undefined>();
  let pos = $state({ top: 0, left: 0 });

  function close() {
    onclose();
    if (returnFocus?.isConnected) returnFocus.focus();
  }

  function entries(): HTMLElement[] {
    return Array.from(menuEl?.querySelectorAll<HTMLElement>(ITEMS) ?? []);
  }

  $effect(() => {
    if (!open || !menuEl) return;
    try {
      menuEl.showPopover?.();
    } catch {
      // Without the Popover API the menu still shows in place.
    }
    const margin = 8;
    const width = menuEl.offsetWidth || 220;
    const height = menuEl.offsetHeight || 120;
    const vw = window.innerWidth || 1280;
    const vh = window.innerHeight || 800;
    if (anchor) {
      const left = Math.max(margin, Math.min(vw - width - margin, anchor.left));
      const below = anchor.bottom + 4;
      const top = below + height <= vh - margin ? below : Math.max(margin, anchor.top - height - 4);
      pos = { top, left };
    } else {
      pos = { top: margin, left: margin };
    }
    untrack(() => entries()[0]?.focus());

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && menuEl?.contains(target)) return;
      onclose();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' || event.key === 'Tab') {
        if (event.key === 'Escape') event.preventDefault();
        event.stopPropagation();
        close();
        return;
      }
      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      const list = entries();
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

  function choose(item: FilterMenuItem) {
    if (item.disabled) return;
    close();
    item.run();
  }
</script>

{#if open}
  <div
    bind:this={menuEl}
    class="menu"
    role="menu"
    aria-label={label}
    popover="manual"
    style:top="{pos.top}px"
    style:left="{pos.left}px"
  >
    {#each items as item (item.label)}
      <button
        type="button"
        class="item"
        role="menuitem"
        tabindex={-1}
        aria-disabled={item.disabled ? 'true' : undefined}
        onclick={() => choose(item)}
      >
        {item.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    inset: auto;
    display: grid;
    min-width: 200px;
    max-width: 320px;
    margin: 0;
    padding: var(--space-1);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
  }

  .item {
    display: block;
    width: 100%;
    min-height: var(--control-height-sm);
    padding: var(--space-1) var(--space-2);
    overflow: hidden;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    color: var(--color-muted);
    cursor: default;
  }

  @media (forced-colors: active) {
    .item:focus-visible {
      outline-color: Highlight;
    }
  }
</style>
