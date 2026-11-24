<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    active?: boolean;
    accent?: boolean;
    children: Snippet;
  }

  let { active = true, accent = false, children }: Props = $props();
</script>

<div class={['brackets', { active, accent }]}>
  <div class="inner">{@render children()}</div>
</div>

<style>
  .brackets {
    border-radius: var(--radius-lg);
    outline: var(--border-width) solid transparent;
    outline-offset: calc(-1 * var(--border-width));
    transition: outline-color var(--duration-fast) var(--ease-out);
  }

  .active {
    outline-color: var(--color-border);
  }

  .accent {
    outline-color: var(--color-accent);
  }

  .inner {
    min-width: 0;
  }
</style>
