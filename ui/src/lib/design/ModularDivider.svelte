<script lang="ts">
  import MonoLabel from './MonoLabel.svelte';

  interface Props {
    orientation?: 'horizontal' | 'vertical';
    inset?: 'none' | 'md';
    ticks?: boolean;
    label?: string;
  }

  let { orientation = 'horizontal', inset = 'none', ticks = false, label }: Props = $props();
</script>

<div
  class={['divider', orientation, `inset-${inset}`, { ticks }]}
  role="separator"
  aria-orientation={orientation}
>
  {#if ticks}
    <span class="tick start" aria-hidden="true"></span>
  {/if}
  <span class="line" aria-hidden="true"></span>
  {#if label && orientation === 'horizontal'}
    <span class="label"><MonoLabel text={label} size="2xs" /></span>
    <span class="line" aria-hidden="true"></span>
  {/if}
  {#if ticks}
    <span class="tick end" aria-hidden="true"></span>
  {/if}
</div>

<style>
  .divider {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .horizontal {
    width: 100%;
  }

  .horizontal.inset-md {
    padding: 0 var(--space-4);
  }

  .vertical {
    flex-direction: column;
    width: var(--border-width);
    height: var(--divider-vertical-height);
  }

  .line {
    flex: 1;
    background: var(--color-border);
  }

  .horizontal .line {
    height: var(--border-width);
  }

  .vertical .line {
    width: var(--border-width);
  }

  .tick {
    flex-shrink: 0;
    background: var(--color-border-control);
  }

  .horizontal .tick {
    width: var(--border-width);
    height: var(--divider-tick-size);
  }

  .vertical .tick {
    width: var(--divider-tick-size);
    height: var(--border-width);
  }

  .label {
    display: inline-flex;
    flex-shrink: 0;
  }
</style>
