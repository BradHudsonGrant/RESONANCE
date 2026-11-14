<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    spacing?: 16 | 24;
    children?: Snippet;
  }

  let { spacing = 16, children }: Props = $props();
</script>

<div class={['grid-dots', spacing === 24 ? 'spacing-24' : 'spacing-16']}>
  {#if children}
    <div class="content">{@render children()}</div>
  {/if}
</div>

<style>
  .grid-dots {
    --dot-step: var(--grid-dots-sm);
    position: relative;
    min-height: var(--control-circle-lg);
    padding: var(--space-4);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background-color: var(--color-background);
    background-image: radial-gradient(
      circle,
      var(--color-border) var(--border-width),
      transparent var(--border-width)
    );
    background-size: var(--dot-step) var(--dot-step);
  }

  .spacing-24 {
    --dot-step: var(--grid-dots-md);
  }

  .content {
    position: relative;
    z-index: 1;
  }
</style>
