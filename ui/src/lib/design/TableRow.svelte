<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLDivElement> {
    selected?: boolean;
    header?: boolean;
    disabled?: boolean;
    children: Snippet;
  }

  let { selected = false, header = false, disabled = false, children, ...rest }: Props = $props();
</script>

<div
  class={['row', { selected, header, disabled }]}
  role="row"
  aria-selected={header ? undefined : selected}
  aria-disabled={disabled ? true : undefined}
  {...rest}
>
  {@render children()}
</div>

<style>
  .row {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    align-items: center;
    height: var(--row-height);
    border-bottom: var(--border-width) solid var(--color-border);
    /* Marks the selected row without moving its cells. */
    border-left: var(--selection-bar-width) solid transparent;
  }

  .header {
    height: var(--table-header-height);
  }

  .row:not(.header):not(.disabled):hover {
    background: var(--color-surface-2);
  }

  .selected,
  .selected:not(.header):not(.disabled):hover {
    border-left-color: var(--color-accent);
    border-radius: var(--radius-sm);
    background: var(--color-raised);
    outline: var(--border-width) solid var(--color-border-control);
    outline-offset: calc(-1 * var(--border-width));
  }

  .disabled {
    color: var(--color-muted);
  }
</style>
