<script lang="ts">
  import type { Snippet } from 'svelte';

  import Icon from './Icon.svelte';
  import Tooltip from './Tooltip.svelte';

  interface Props {
    text?: string;
    size?: '2xs' | 'xs';
    tone?: 'muted' | 'secondary' | 'primary';
    infoTooltip?: string;
    actionLabel?: string;
    onAction?: () => void;
    children?: Snippet;
  }

  let {
    text,
    size = 'xs',
    tone = 'muted',
    infoTooltip,
    actionLabel,
    onAction,
    children,
  }: Props = $props();
</script>

<span
  class={['mono-label', `size-${size}`, `tone-${tone}`, { 'has-action': Boolean(actionLabel) }]}
>
  <span class="label-group">
    <span class="text">
      {#if children}
        {@render children()}
      {:else}
        {text}
      {/if}
    </span>
    {#if infoTooltip}
      <Tooltip text={infoTooltip}>
        <span class="info" role="img" aria-label={infoTooltip}>
          <Icon name="info" size="xs" />
        </span>
      </Tooltip>
    {/if}
  </span>
  {#if actionLabel}
    <button type="button" class="action" onclick={onAction}>
      {actionLabel}
    </button>
  {/if}
</span>

<style>
  .mono-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
  }

  .has-action {
    display: flex;
    justify-content: space-between;
    width: 100%;
  }

  .label-group {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
  }

  .text {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-mono);
    letter-spacing: var(--tracking-mono);
    text-transform: uppercase;
    white-space: pre-line;
  }

  .size-2xs .text {
    font-size: var(--text-2xs);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-mono-wide);
  }

  .tone-muted .text {
    color: var(--color-muted);
  }

  .tone-secondary .text {
    color: var(--color-text-2);
  }

  .tone-primary .text {
    color: var(--color-text);
  }

  .info {
    display: inline-flex;
    align-items: center;
    color: var(--color-muted);
  }

  .action {
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
    line-height: var(--leading-mono);
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .action:hover {
    color: var(--color-text);
  }
</style>
