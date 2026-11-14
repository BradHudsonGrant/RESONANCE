<script lang="ts">
  interface Props {
    label: string;
    level: 0 | 1 | 2 | 3 | 4;
  }

  let { label, level }: Props = $props();
  const dots = [1, 2, 3, 4] as const;
</script>

<span
  class="level-dots"
  role="meter"
  aria-label="{label} {level} of 4"
  aria-valuemin={0}
  aria-valuemax={4}
  aria-valuenow={level}
>
  <span class="label">{label}</span>
  <span class="dots" aria-hidden="true">
    {#each dots as dot (dot)}
      <span class={['dot', { lit: dot <= level }]}></span>
    {/each}
  </span>
</span>

<style>
  .level-dots {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .label {
    color: var(--color-text-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-mono);
    letter-spacing: var(--tracking-mono);
    text-transform: uppercase;
  }

  .dots {
    display: inline-flex;
    align-items: center;
    gap: var(--space-0-5);
  }

  .dot {
    width: var(--level-dot-size);
    height: var(--level-dot-size);
    border-radius: var(--radius-pill);
    background: var(--color-border);
  }

  .lit {
    background: var(--color-text);
  }
</style>
