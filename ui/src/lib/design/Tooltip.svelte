<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    text: string;
    /** The keys that do the same thing, shown after the text. */
    shortcut?: string;
    children: Snippet;
  }

  let { text, shortcut, children }: Props = $props();
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
  <span {id} class={['tooltip', { shown }]} role="tooltip">
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
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-control);
    background: var(--color-raised);
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: 400;
    white-space: nowrap;
    visibility: hidden;
    opacity: 0;
    transform: translateX(-50%);
    transition: opacity var(--motion-fast) var(--ease-out);
    pointer-events: none;
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
