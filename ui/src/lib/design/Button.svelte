<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  interface Props extends HTMLButtonAttributes {
    /** `primary` is the one action a view is for, so a view has at most one. */
    variant?: 'default' | 'primary' | 'quiet';
    children: Snippet;
  }

  let { variant = 'default', type = 'button', children, ...rest }: Props = $props();
</script>

<button class={['button', variant]} {type} {...rest}>{@render children()}</button>

<style>
  .button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-control);
    background: var(--color-surface-2);
    color: var(--color-text);
    font: inherit;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--motion-fast) var(--ease-out);
  }

  .button:hover:not(:disabled) {
    background: var(--color-raised);
  }

  .primary {
    border-color: var(--color-accent);
    background: var(--color-accent);
    color: var(--color-on-accent);
  }

  .primary:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-accent) 88%, var(--color-text));
  }

  .quiet {
    border-color: transparent;
    background: transparent;
    color: var(--color-text-2);
  }

  .quiet:hover:not(:disabled) {
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .button:disabled {
    border-color: var(--color-border);
    background: transparent;
    color: var(--color-muted);
    cursor: default;
  }
</style>
