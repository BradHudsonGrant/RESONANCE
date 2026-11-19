<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLDivElement> {
    /** The focused row: framed, with the accent mark. */
    selected?: boolean;
    /** One of several ticked rows that does not have focus. */
    checked?: boolean;
    header?: boolean;
    disabled?: boolean;
    children: Snippet;
  }

  let {
    selected = false,
    checked = false,
    header = false,
    disabled = false,
    children,
    ...rest
  }: Props = $props();
</script>

<div
  class={['row', { selected, checked, header, disabled }]}
  role="row"
  aria-selected={header ? undefined : selected}
  aria-disabled={disabled ? true : undefined}
  {...rest}
>
  {@render children()}
</div>

<style>
  .row {
    position: relative;
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    align-items: center;
    height: var(--row-height);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .header {
    height: var(--table-header-height);
  }

  .row:not(.header):not(.disabled):hover {
    background: var(--color-surface-2);
  }

  .selected,
  .selected:not(.header):not(.disabled):hover {
    border-bottom-color: transparent;
    border-radius: var(--radius-sm);
    background: var(--color-raised);
    outline: var(--border-width) solid var(--color-border-control);
    outline-offset: calc(-1 * var(--border-width));
  }

  .selected:not(.header)::before {
    background: var(--color-accent);
  }

  .checked,
  .checked:not(.header):not(.disabled):hover {
    background: var(--color-surface-2);
  }

  .selected:not(.header)::before,
  .checked:not(.header)::before {
    position: absolute;
    top: var(--space-1-5);
    bottom: var(--space-1-5);
    left: 0;
    width: var(--selection-bar-width);
    border-radius: var(--radius-xs);
    content: '';
  }

  .checked:not(.header)::before {
    background: var(--color-border-control);
  }

  .disabled {
    color: var(--color-muted);
  }

  @media (forced-colors: active) {
    .selected,
    .selected:not(.header):not(.disabled):hover {
      outline-color: Highlight;
    }

    .selected:not(.header)::before {
      background: Highlight;
      forced-color-adjust: none;
    }

    .checked:not(.header)::before {
      background: CanvasText;
      forced-color-adjust: none;
    }
  }
</style>
