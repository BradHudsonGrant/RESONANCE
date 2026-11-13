<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';

  interface Props extends HTMLInputAttributes {
    /** Always given, for screen readers. Shown above the field unless `hideLabel` is set. */
    label: string;
    hideLabel?: boolean;
    value?: string;
  }

  let { label, hideLabel = false, value = $bindable(''), ...rest }: Props = $props();
</script>

<label class="field">
  <span class={['label', { 'visually-hidden': hideLabel }]}>{label}</span>
  <input class="input" bind:value {...rest} />
</label>

<style>
  .field {
    display: grid;
    gap: var(--space-1);
  }

  .label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .input {
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--color-border-control);
    border-radius: var(--radius-control);
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
  }

  .input::placeholder {
    color: var(--color-muted);
  }

  .input:disabled {
    border-color: var(--color-border);
    color: var(--color-muted);
  }
</style>
