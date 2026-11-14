<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';

  interface Props {
    label: string;
    variant?: 'pill' | 'rect' | 'removable' | 'score' | 'keybadge';
    leadingIcon?: IconName;
    accent?: boolean;
    onRemove?: () => void;
  }

  let { label, variant = 'pill', leadingIcon, accent = false, onRemove }: Props = $props();
</script>

<span class={['tag', variant, { accent }]}>
  {#if leadingIcon}
    <Icon name={leadingIcon} size="xs" />
  {/if}
  <span class="text">{label}</span>
  {#if variant === 'removable' || onRemove}
    <button type="button" class="remove" aria-label="Remove {label}" onclick={onRemove}>
      <Icon name="close" size="xs" />
    </button>
  {/if}
</span>

<style>
  .tag {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-1-5);
    height: var(--control-height-xs);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-pill);
    background: var(--color-surface-2);
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
    font-variant-numeric: tabular-nums;
    line-height: var(--leading-mono);
    white-space: nowrap;
  }

  .rect {
    border-radius: var(--radius-sm);
  }

  .removable {
    height: var(--control-height-sm);
    border-radius: var(--radius-sm);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
  }

  .score {
    min-width: var(--score-min-width);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
  }

  .keybadge {
    height: var(--control-height-keybadge);
    padding: 0 var(--space-2);
    border-radius: var(--radius-sm);
    color: var(--color-text-2);
  }

  .accent {
    border-color: var(--color-accent);
  }

  .remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 0;
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .remove:hover {
    color: var(--color-text);
  }
</style>
