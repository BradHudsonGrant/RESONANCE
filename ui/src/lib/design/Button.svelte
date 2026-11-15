<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';

  import Icon from './Icon.svelte';

  interface Props extends HTMLButtonAttributes {
    variant?: 'default' | 'primary' | 'quiet' | 'icon' | 'circle' | 'select';
    size?: 'xs' | 'sm' | 'md' | 'lg';
    active?: boolean;
    ariaLabel?: string;
    children: Snippet;
  }

  let {
    variant = 'default',
    size = 'md',
    active = false,
    ariaLabel,
    type = 'button',
    children,
    ...rest
  }: Props = $props();
</script>

<button
  class={['button', variant, `size-${size}`, { active }]}
  {type}
  aria-label={ariaLabel ?? rest['aria-label']}
  aria-pressed={active ? true : rest['aria-pressed']}
  {...rest}
>
  {@render children()}
  {#if variant === 'select'}
    <Icon name="chevron-down" size="xs" />
  {/if}
</button>

<style>
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: var(--control-height);
    padding: 0 var(--space-3);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-compact);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .size-xs {
    height: var(--control-height-xs);
    padding: 0 var(--space-2);
    font-size: var(--text-xs);
  }

  .size-sm {
    height: var(--control-height-sm);
    padding: 0 var(--space-2-5);
  }

  .size-lg {
    height: var(--control-height-md);
    padding: 0 var(--space-4);
  }

  .button:hover:not(:disabled, [aria-disabled='true']) {
    background: var(--color-raised);
  }

  .button:active:not(:disabled, [aria-disabled='true']),
  .active {
    border-color: var(--color-border-control);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .primary {
    border-color: var(--color-text);
    background: var(--color-text);
    color: var(--color-background);
  }

  .primary:hover:not(:disabled, [aria-disabled='true']),
  .primary:active:not(:disabled, [aria-disabled='true']) {
    background: color-mix(in srgb, var(--color-text) 88%, var(--color-background));
    color: var(--color-background);
  }

  .quiet {
    border-color: transparent;
    background: transparent;
    color: var(--color-text-2);
  }

  .quiet:hover:not(:disabled, [aria-disabled='true']),
  .quiet:focus-visible:not(:disabled, [aria-disabled='true']) {
    background: var(--color-raised);
    color: var(--color-text);
  }

  .icon {
    width: var(--control-height);
    height: var(--control-height);
    padding: 0;
    border-color: transparent;
    background: transparent;
    color: var(--color-text-2);
  }

  .icon.size-xs {
    width: var(--control-height-xs);
    height: var(--control-height-xs);
  }

  .icon.size-sm {
    width: var(--control-height-keybadge);
    height: var(--control-height-keybadge);
    border-color: var(--color-border);
    border-radius: var(--radius-xs);
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .icon.size-md {
    width: var(--control-height);
    height: var(--control-height);
  }

  .icon.size-lg {
    width: var(--control-height-md);
    height: var(--control-height-md);
    border-color: var(--color-border);
    background: var(--color-surface-2);
  }

  .icon:hover:not(:disabled, [aria-disabled='true']),
  .icon.active {
    background: var(--color-raised);
    color: var(--color-text);
  }

  .circle {
    width: var(--control-height);
    height: var(--control-height);
    padding: 0;
    border-color: var(--color-border);
    border-radius: var(--radius-pill);
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .circle.size-sm {
    width: var(--control-height-sm);
    height: var(--control-height-sm);
  }

  .circle.size-lg {
    width: var(--control-circle-lg);
    height: var(--control-circle-lg);
    border-color: var(--color-text);
    background: var(--color-text);
    color: var(--color-background);
  }

  .circle.size-lg:hover:not(:disabled, [aria-disabled='true']) {
    background: color-mix(in srgb, var(--color-text) 88%, var(--color-background));
    color: var(--color-background);
  }

  .select {
    height: var(--control-height-sm);
    padding: 0 var(--space-2-5);
    border-color: var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text);
  }

  .button:disabled {
    border-color: var(--color-border);
    background: transparent;
    color: var(--color-muted);
    cursor: default;
  }

  /* Not built yet: it stays hoverable so its tooltip can say which milestone brings it. */
  .button[aria-disabled='true'] {
    color: var(--color-muted);
    cursor: default;
  }

  .circle.size-lg[aria-disabled='true'] {
    border-color: var(--color-border);
    background: var(--color-surface-2);
  }

  @media (forced-colors: active) {
    .button:disabled,
    .button[aria-disabled='true'] {
      color: GrayText;
    }
  }
</style>
