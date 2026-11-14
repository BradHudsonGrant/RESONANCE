<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  import DataTag from './DataTag.svelte';
  import Icon, { type IconName } from './Icon.svelte';

  interface Props extends Omit<HTMLInputAttributes, 'size'> {
    /** Always given, for screen readers. Shown above the field unless `hideLabel` is set. */
    label?: string;
    hideLabel?: boolean;
    value?: string;
    size?: 'md' | 'lg';
    leadingIcon?: IconName;
    shortcutBadge?: string;
  }

  let {
    label,
    hideLabel = false,
    value = $bindable(''),
    size = 'md',
    leadingIcon,
    shortcutBadge,
    placeholder,
    disabled = false,
    ...rest
  }: Props = $props();

  const labelId = $props.id();
  const resolvedLabel = $derived(label ?? placeholder ?? 'Text input');
  const shouldHideLabel = $derived(hideLabel || !label);
</script>

<label class="field">
  <span id={labelId} class={['label', { 'visually-hidden': shouldHideLabel }]}>{resolvedLabel}</span
  >
  <span class={['box', `size-${size}`, { disabled }]}>
    {#if leadingIcon}
      <span class="leading">
        <Icon name={leadingIcon} size="sm" />
      </span>
    {/if}
    <input class="input" aria-labelledby={labelId} bind:value {placeholder} {disabled} {...rest} />
    {#if shortcutBadge}
      <span class="badge" aria-hidden="true">
        <DataTag label={shortcutBadge} variant="keybadge" />
      </span>
    {/if}
  </span>
</label>

<style>
  .field {
    display: grid;
    min-width: 0;
    gap: var(--space-1);
  }

  .label {
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
  }

  .box {
    display: flex;
    align-items: center;
    gap: var(--space-2-5);
    min-width: 0;
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    transition: border-color var(--duration-fast) var(--ease-out);
  }

  .box.size-lg {
    height: var(--control-height-lg);
    padding: 0 var(--space-3-5);
    border-radius: var(--radius-lg);
  }

  .box:focus-within {
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: var(--focus-ring-offset);
  }

  .leading {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--color-text-2);
  }

  .input {
    flex: 1;
    min-width: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text);
    font-family: var(--font-sans);
    font-size: var(--text-md);
  }

  .size-lg .input {
    font-size: var(--text-base);
  }

  .input:focus-visible {
    outline: none;
  }

  .input::placeholder {
    color: var(--color-muted);
  }

  .badge {
    display: inline-flex;
    flex-shrink: 0;
  }

  .disabled,
  .input:disabled {
    border-color: var(--color-border);
    color: var(--color-muted);
  }
</style>
