<script lang="ts">
  import type { Snippet } from 'svelte';

  import Wave from '../wave/Wave.svelte';

  interface Props {
    variant?: 'surface' | 'raised' | 'interactive' | 'dashed';
    radius?: 'md' | 'lg' | 'xl';
    selected?: boolean;
    disabled?: boolean;
    waveArt?: boolean;
    waveSeed?: number;
    ariaLabel?: string;
    onclick?: (event: MouseEvent) => void;
    children: Snippet;
  }

  let {
    variant = 'surface',
    radius = 'lg',
    selected = false,
    disabled = false,
    waveArt = false,
    waveSeed = 1,
    ariaLabel,
    onclick,
    children,
  }: Props = $props();

  const isButton = $derived(variant === 'interactive' || variant === 'dashed' || Boolean(onclick));
</script>

{#if isButton}
  <button
    type="button"
    class={['card', variant, `radius-${radius}`, { selected, 'has-wave': waveArt }]}
    aria-label={ariaLabel}
    aria-pressed={selected ? true : undefined}
    {disabled}
    {onclick}
  >
    {#if waveArt}
      <span class="wave-backdrop" aria-hidden="true">
        <Wave variant="card" seed={waveSeed} />
      </span>
    {/if}
    <span class="content">{@render children()}</span>
  </button>
{:else}
  <div
    class={['card', variant, `radius-${radius}`, { selected, 'has-wave': waveArt }]}
    aria-label={ariaLabel}
  >
    {#if waveArt}
      <div class="wave-backdrop" aria-hidden="true">
        <Wave variant="card" seed={waveSeed} />
      </div>
    {/if}
    <div class="content">{@render children()}</div>
  </div>
{/if}

<style>
  .card {
    position: relative;
    display: block;
    overflow: hidden;
    padding: var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
    text-align: left;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out);
  }

  .radius-md {
    border-radius: var(--radius-md);
  }

  .radius-lg {
    border-radius: var(--radius-lg);
  }

  .radius-xl {
    border-radius: var(--radius-xl);
  }

  .raised {
    border-color: var(--color-border-control);
    background: var(--color-raised);
  }

  .interactive {
    cursor: pointer;
  }

  .interactive:hover:not(:disabled) {
    border-color: var(--color-border-control);
    background: var(--color-raised);
  }

  .dashed {
    border-style: dashed;
    border-color: var(--color-border-control);
    background: transparent;
    color: var(--color-text-2);
    cursor: pointer;
  }

  .dashed:hover:not(:disabled) {
    border-color: var(--color-text);
    color: var(--color-text);
  }

  .selected {
    border-color: var(--color-border-control);
    box-shadow: inset var(--selection-bar-width) 0 0 var(--color-accent);
  }

  .card:disabled {
    border-color: var(--color-border);
    color: var(--color-muted);
    cursor: default;
  }

  .wave-backdrop {
    position: absolute;
    inset: 0;
    opacity: 0.32;
    pointer-events: none;
  }

  .content {
    position: relative;
    z-index: 1;
    display: block;
  }
</style>
