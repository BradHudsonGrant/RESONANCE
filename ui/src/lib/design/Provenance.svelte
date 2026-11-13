<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import type { Snippet } from 'svelte';

  import Icon from './Icon.svelte';
  import { describe, type Origin } from './provenance';
  import Tooltip from './Tooltip.svelte';

  /**
   * Shows a value with the mark for where it came from. Every data field goes through this, so
   * a measured value, an estimate and a confirmed one look the same everywhere.
   */
  let { origin, children }: { origin: Origin; children: Snippet } = $props();

  const description = $derived(describe(origin));
  const low = $derived(origin.kind === 'estimated' && origin.low);
</script>

<span class={['value', { low }]}>
  {@render children()}
  {#if origin.kind === 'estimated' && description}
    <Tooltip text={description}>
      <span class="dot" role="img" aria-label={description}></span>
    </Tooltip>
  {:else if origin.kind === 'confirmed' && description}
    <Tooltip text={description}>
      <span class="tick" role="img" aria-label={description}><Icon icon={Check} size={12} /></span>
    </Tooltip>
  {/if}
</span>

<style>
  .value {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .low {
    color: var(--color-muted);
  }

  .dot {
    width: 6px;
    height: 6px;
    border: 1px solid currentcolor;
    border-radius: 50%;
  }

  .tick {
    display: inline-flex;
    color: var(--color-text-2);
  }
</style>
