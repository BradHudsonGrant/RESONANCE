<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    text: string;
    /** The keys that do the same thing, shown after the text. */
    shortcut?: string;
    /** Where it opens. Beside the anchor for controls at the edge of the window. */
    side?: 'above' | 'below' | 'right' | 'left';
    children: Snippet;
  }

  let { text, shortcut, side = 'above', children }: Props = $props();
  let shown = $state(false);
  const id = $props.id();
</script>

<span
  class="anchor"
  role="presentation"
  aria-describedby={id}
  onmouseenter={() => (shown = true)}
  onmouseleave={() => (shown = false)}
  onfocusin={() => (shown = true)}
  onfocusout={() => (shown = false)}
  onkeydown={(event) => {
    if (event.key === 'Escape') shown = false;
  }}
>
  {@render children()}
  <span {id} class={['tooltip', side, { shown }]} role="tooltip">
    {text}
    {#if shortcut}
      <kbd class="numeric">{shortcut}</kbd>
    {/if}
  </span>
</span>

<style>
  .anchor {
    position: relative;
    display: inline-flex;
  }

  .tooltip {
    position: absolute;
    bottom: calc(100% + var(--space-1));
    left: 50%;
    z-index: 10;
    display: flex;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-raised);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
    white-space: nowrap;
    visibility: hidden;
    opacity: 0;
    transform: translateX(-50%);
    transition: opacity var(--duration-fast) var(--ease-out);
    pointer-events: none;
  }

  .below {
    top: calc(100% + var(--space-1));
    bottom: auto;
  }

  .right,
  .left {
    top: 50%;
    bottom: auto;
    transform: translateY(-50%);
  }

  .right {
    left: calc(100% + var(--space-1));
  }

  .left {
    right: calc(100% + var(--space-1));
    left: auto;
  }

  .shown {
    visibility: visible;
    opacity: 1;
  }

  kbd {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }
</style>
