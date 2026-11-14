<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** BPM, time, key and other numbers: fixed-width digits, set to the right. */
    numeric?: boolean;
    header?: boolean;
    children: Snippet;
  }

  let { numeric = false, header = false, children }: Props = $props();
</script>

<div class={['cell', { numeric, header }]} role={header ? 'columnheader' : 'cell'}>
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

  .numeric {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .header {
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-compact);
  }
</style>
