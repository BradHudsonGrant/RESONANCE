<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, children }: { title?: string; children: Snippet } = $props();
</script>

<section class="panel">
  {#if title}
    <h2 class="title">{title}</h2>
  {/if}
  {@render children()}
</section>

<style>
  .panel {
    padding: var(--space-4);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-panel);
    background: var(--color-surface);
  }

  .title {
    margin-bottom: var(--space-3);
    font-size: var(--text-lg);
  }
</style>
