<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    label: string;
    active?: boolean;
    disabled?: boolean;
    icon?: IconName;
    count?: number;
    onclick?: () => void;
  }

  let { label, active = false, disabled = false, icon, count, onclick }: Props = $props();
</script>

<button
  type="button"
  class={['filter-pill', { active }]}
  aria-pressed={active}
  {disabled}
  {onclick}
>
  {#if icon}
    <Icon name={icon} size="xs" />
  {/if}
  <span>{label}</span>
  {#if count !== undefined}
    <span class="count numeric">{count.toLocaleString('en-GB')}</span>
  {/if}
</button>

<style>
  .filter-pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    height: var(--control-height-sm);
    padding: 0 var(--space-3);
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-compact);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .filter-pill:hover:not(:disabled) {
    background: var(--color-raised);
    color: var(--color-text);
  }

  .active,
  .active:hover:not(:disabled) {
    background: var(--color-text);
    color: var(--color-background);
  }

  .count {
    font-size: var(--text-xs);
  }

  .filter-pill:disabled {
    color: var(--color-muted);
    cursor: default;
  }
</style>
