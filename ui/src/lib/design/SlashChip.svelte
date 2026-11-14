<script lang="ts">
  interface Props {
    command: string;
    description: string;
    active?: boolean;
    disabled?: boolean;
    /** -1 keeps a chip out of the Tab order. */
    tabindex?: number;
    onclick?: () => void;
  }

  let {
    command,
    description,
    active = false,
    disabled = false,
    tabindex,
    onclick,
  }: Props = $props();
</script>

<!-- A chip that is not built yet stays focusable, so its tooltip can say when it arrives. -->
<button
  type="button"
  class={['slash-chip', { active }]}
  aria-pressed={active ? true : undefined}
  aria-disabled={disabled ? 'true' : undefined}
  {tabindex}
  onclick={() => {
    if (!disabled) onclick?.();
  }}
>
  <span class="code">{command}</span>
  <span class="description">{description}</span>
</button>

<style>
  .slash-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2-5);
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
  }

  .code {
    display: inline-flex;
    align-items: center;
    height: var(--control-height-xs);
    padding: 0 var(--space-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: var(--leading-mono);
    white-space: nowrap;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out);
  }

  .description {
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    white-space: nowrap;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .slash-chip:hover:not([aria-disabled='true']) .code,
  .active .code {
    border-color: var(--color-border-control);
    background: var(--color-raised);
  }

  .slash-chip:hover:not([aria-disabled='true']) .description,
  .active .description {
    color: var(--color-text);
  }

  .slash-chip[aria-disabled='true'] {
    cursor: default;
  }

  .slash-chip[aria-disabled='true'] .code,
  .slash-chip[aria-disabled='true'] .description {
    color: var(--color-muted);
  }

  @media (forced-colors: active) {
    .slash-chip[aria-disabled='true'] .code,
    .slash-chip[aria-disabled='true'] .description {
      color: GrayText;
    }
  }
</style>
