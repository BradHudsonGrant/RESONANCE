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
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .numeric {
    text-align: right;
  }

  .header {
    color: var(--color-text-2);
    font-size: var(--text-sm);
    font-weight: 500;
  }
</style>
