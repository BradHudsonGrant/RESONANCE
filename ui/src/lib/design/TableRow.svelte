<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLDivElement> {
    selected?: boolean;
    children: Snippet;
  }

  let { selected = false, children, ...rest }: Props = $props();
</script>

<div class={['row', { selected }]} role="row" aria-selected={selected} {...rest}>
  {@render children()}
</div>

<style>
  .row {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    align-items: center;
    height: var(--row-height);
    border-bottom: 1px solid var(--color-border);
    /* Marks the selected row without moving its cells. */
    border-left: 2px solid transparent;
  }

  .row:hover {
    background: var(--color-surface-2);
  }

  .selected,
  .selected:hover {
    border-left-color: var(--color-accent);
    background: var(--color-raised);
  }
</style>
