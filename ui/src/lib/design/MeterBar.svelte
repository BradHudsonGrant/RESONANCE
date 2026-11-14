<script lang="ts">
  interface Props {
    value: 0 | 1 | 2 | 3 | 4 | 5 | null;
    label?: string;
  }

  let { value, label }: Props = $props();

  const resolvedLabel = $derived(
    label ?? (value === null ? 'Energy unanalysed' : `Energy ${value} of 5`),
  );
  const bars = [1, 2, 3, 4, 5] as const;
</script>

<span
  class={['meter', { unanalysed: value === null }]}
  role="meter"
  aria-label={resolvedLabel}
  aria-valuemin={0}
  aria-valuemax={5}
  aria-valuenow={value ?? undefined}
>
  {#each bars as bar (bar)}
    <span class={['bar', `bar-${bar}`, { lit: value !== null && bar <= value }]}></span>
  {/each}
</span>

<style>
  .meter {
    display: inline-flex;
    align-items: flex-end;
    gap: var(--meter-bar-gap);
    width: var(--meter-width);
    height: var(--meter-height);
  }

  .bar {
    width: var(--meter-bar-width);
    border-radius: var(--meter-bar-radius);
    background: color-mix(in srgb, var(--color-border-control) 35%, transparent);
  }

  .bar-1 {
    height: var(--meter-bar-1);
  }

  .bar-2 {
    height: var(--meter-bar-2);
  }

  .bar-3 {
    height: var(--meter-bar-3);
  }

  .bar-4 {
    height: var(--meter-bar-4);
  }

  .bar-5 {
    height: var(--meter-bar-5);
  }

  .lit {
    background: var(--color-text);
  }

  @media (prefers-contrast: more), (forced-colors: active) {
    .bar:not(.lit) {
      border: var(--border-width) solid var(--color-border-control);
      background: var(--color-surface-2);
    }
  }
</style>
