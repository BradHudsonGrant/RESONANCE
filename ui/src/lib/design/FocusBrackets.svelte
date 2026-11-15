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
  <span class="corner top-left" aria-hidden="true"></span>
  <span class="corner top-right" aria-hidden="true"></span>
  <span class="corner bottom-left" aria-hidden="true"></span>
  <span class="corner bottom-right" aria-hidden="true"></span>
  <div class="inner">{@render children()}</div>
</div>

<style>
  .brackets {
    --bracket-color: var(--color-border-control);
    position: relative;
    padding: var(--space-3);
  }

  .active {
    --bracket-color: var(--color-text);
  }

  .accent {
    --bracket-color: var(--color-accent);
  }

  .corner {
    position: absolute;
    width: var(--focus-bracket-size);
    height: var(--focus-bracket-size);
    border-color: var(--bracket-color);
    border-style: solid;
    border-width: 0;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--duration-fast) var(--ease-out);
  }

  .active .corner,
  .accent .corner {
    opacity: 1;
  }

  .top-left {
    top: 0;
    left: 0;
    border-top-width: var(--border-width);
    border-left-width: var(--border-width);
  }

  .top-right {
    top: 0;
    right: 0;
    border-top-width: var(--border-width);
    border-right-width: var(--border-width);
  }

  .bottom-left {
    bottom: 0;
    left: 0;
    border-bottom-width: var(--border-width);
    border-left-width: var(--border-width);
  }

  .bottom-right {
    right: 0;
    bottom: 0;
    border-right-width: var(--border-width);
    border-bottom-width: var(--border-width);
  }

  .inner {
    position: relative;
  }
</style>
