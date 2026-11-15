<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** BPM, time, row index and other numbers set in the tabular mono face and right aligned. */
    numeric?: boolean;
    /** Tabular mono face while keeping start alignment, such as Camelot keys (`8A`). */
    mono?: boolean;
    tone?: 'primary' | 'secondary' | 'muted';
    size?: 'sm' | 'md';
    weight?: 'regular' | 'medium';
    /** Tight horizontal padding for narrow icon, status and tick-box columns. */
    compact?: boolean;
    class?: string;
    header?: boolean;
    ariaSort?: 'ascending' | 'descending' | 'none';
    children: Snippet;
  }

  let {
    numeric = false,
    mono = false,
    tone = 'primary',
    size = 'md',
    weight = 'regular',
    compact = false,
    class: className = '',
    header = false,
    ariaSort,
    children,
  }: Props = $props();
</script>

<div
  class={[
    'cell',
    tone !== 'primary' && !header ? `tone-${tone}` : '',
    size === 'sm' && !header ? 'size-sm' : '',
    weight === 'medium' && !header ? 'weight-medium' : '',
    { numeric, mono, compact, header },
    className,
  ]}
  role={header ? 'columnheader' : 'cell'}
  aria-sort={header ? ariaSort : undefined}
>
  {@render children()}
</div>

<style>
  .cell {
    overflow: hidden;
    padding: 0 var(--space-2);
    font-family: var(--font-sans);
    font-size: var(--text-md);
    line-height: var(--leading-normal);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .compact {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0 var(--space-1);
  }

  .mono,
  .numeric {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  .numeric {
    text-align: right;
  }

  .size-sm {
    font-size: var(--text-sm);
  }

  .weight-medium {
    font-weight: var(--weight-medium);
  }

  .tone-secondary {
    color: var(--color-text-2);
  }

  .tone-muted {
    color: var(--color-muted);
  }

  .header {
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-compact);
  }
</style>
