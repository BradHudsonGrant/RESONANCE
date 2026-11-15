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
  let tip: HTMLSpanElement;
  let shown = $state(false);
  const id = $props.id();
  const anchor = `--tooltip-${id}`;

  // The tooltip sits in the top layer, so a panel that clips its overflow cannot cut it off.
  $effect(() => {
    if (shown) tip.showPopover();
    else if (tip.matches(':popover-open')) tip.hidePopover();
  });
</script>

<span
  class="anchor"
  role="presentation"
  aria-describedby={id}
  style:anchor-name={anchor}
  onmouseenter={() => (shown = true)}
  onmouseleave={() => (shown = false)}
  onfocusin={() => (shown = true)}
  onfocusout={() => (shown = false)}
  onkeydown={(event) => {
    if (event.key === 'Escape') shown = false;
  }}
>
  {@render children()}
  <span
    bind:this={tip}
    {id}
    class={['tooltip', side, { shown }]}
    role="tooltip"
    popover="manual"
    style:position-anchor={anchor}
  >
    {text}
    {#if shortcut}
      <kbd class="numeric">{shortcut}</kbd>
    {/if}
  </span>
</span>

<style>
  .anchor {
    display: inline-flex;
  }

  .tooltip {
    position-area: top;
    position-try-fallbacks: flip-block, flip-inline;
    inset: auto;
    gap: var(--space-2);
    margin: var(--space-1);
    padding: var(--space-1) var(--space-2);
    overflow: visible;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-raised);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-regular);
    white-space: nowrap;
    pointer-events: none;
  }

  .tooltip:popover-open {
    display: flex;
  }

  .below {
    position-area: bottom;
  }

  .right {
    position-area: right;
  }

  .left {
    position-area: left;
  }

  kbd {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }
</style>
