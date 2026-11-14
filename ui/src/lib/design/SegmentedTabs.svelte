<script lang="ts">
  export interface SegmentedTabItem {
    id: string;
    label: string;
    count?: number;
    disabled?: boolean;
  }

  interface Props {
    items: ReadonlyArray<SegmentedTabItem>;
    value: string;
    ariaLabel?: string;
    onChange?: (id: string) => void;
  }

  let { items, value = $bindable(), ariaLabel = 'Sections', onChange }: Props = $props();

  let tablist: HTMLDivElement;

  function choose(id: string) {
    value = id;
    onChange?.(id);
  }

  function onkeydown(event: KeyboardEvent) {
    const enabled = items.filter((item) => !item.disabled);
    if (enabled.length === 0) return;
    const currentIndex = Math.max(
      0,
      enabled.findIndex((item) => item.id === value),
    );
    let nextIndex = -1;
    if (event.key === 'ArrowRight') {
      nextIndex = (currentIndex + 1) % enabled.length;
    } else if (event.key === 'ArrowLeft') {
      nextIndex = (currentIndex - 1 + enabled.length) % enabled.length;
    } else if (event.key === 'Home') {
      nextIndex = 0;
    } else if (event.key === 'End') {
      nextIndex = enabled.length - 1;
    }
    if (nextIndex >= 0) {
      event.preventDefault();
      const targetItem = enabled[nextIndex];
      if (!targetItem) return;
      choose(targetItem.id);
      const buttons = tablist?.querySelectorAll<HTMLButtonElement>('[role="tab"]:not(:disabled)');
      buttons?.[nextIndex]?.focus();
    }
  }
</script>

<div bind:this={tablist} class="tabs" role="tablist" aria-label={ariaLabel}>
  {#each items as item (item.id)}
    {@const active = item.id === value}
    <button
      type="button"
      role="tab"
      class={['tab', { active }]}
      aria-selected={active}
      tabindex={active ? 0 : -1}
      disabled={item.disabled}
      onclick={() => choose(item.id)}
      {onkeydown}
    >
      <span>{item.label}</span>
      {#if item.count !== undefined}
        <span class="count numeric">{item.count.toLocaleString('en-GB')}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: inline-grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    align-items: center;
    gap: var(--space-0-5);
    height: var(--control-height-nav);
    padding: var(--segmented-pad);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-2);
  }

  .tab {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1-5);
    height: var(--segmented-tab-height);
    padding: 0 var(--space-3);
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-sm);
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
      border-color var(--duration-fast) var(--ease-out),
      color var(--duration-fast) var(--ease-out);
  }

  .tab:hover:not(:disabled) {
    color: var(--color-text);
  }

  .active {
    border-color: var(--color-border-control);
    background: var(--color-raised);
    color: var(--color-text);
  }

  .count {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .active .count {
    color: var(--color-text-2);
  }

  .tab:disabled {
    color: var(--color-muted);
    cursor: default;
  }
</style>
