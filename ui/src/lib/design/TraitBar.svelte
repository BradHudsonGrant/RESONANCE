<script lang="ts">
  interface Props {
    label: string;
    value: number | null;
    formattedValue?: string;
  }

  let { label, value, formattedValue }: Props = $props();

  const clamped = $derived(value === null ? 0 : Math.min(1, Math.max(0, value)));
  const displayValue = $derived(formattedValue ?? (value === null ? '—' : value.toFixed(2)));
</script>

<div
  class="trait-bar"
  role="meter"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={1}
  aria-valuenow={value ?? undefined}
  aria-valuetext={displayValue}
>
  <span class="label">{label}</span>
  <span class="value numeric">{displayValue}</span>
  <span class="track" aria-hidden="true">
    <span class="fill" style:width="{clamped * 100}%"></span>
  </span>
</div>

<style>
  .trait-bar {
    display: grid;
    grid-template-columns: var(--trait-label-width) var(--trait-value-width) minmax(
        0,
        var(--trait-bar-width)
      );
    align-items: center;
    gap: var(--space-3);
    min-height: var(--trait-row-height);
  }

  .label {
    overflow: hidden;
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .value {
    color: var(--color-text);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    text-align: right;
  }

  .track {
    position: relative;
    overflow: hidden;
    width: 100%;
    height: var(--trait-bar-height);
    border-radius: var(--radius-xs);
    background: var(--color-surface-2);
  }

  .fill {
    display: block;
    height: 100%;
    border-radius: var(--radius-xs);
    background: var(--color-text);
  }
</style>
