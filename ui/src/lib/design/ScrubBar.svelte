<script lang="ts">
  interface Props {
    value?: number;
    min?: number;
    max?: number;
    step?: number;
    size?: 'sm' | 'md';
    label?: string;
    valueText?: string;
    disabled?: boolean;
    onInput?: (value: number) => void;
  }

  let {
    value = $bindable(0),
    min = 0,
    max = 100,
    step = 1,
    size = 'md',
    label,
    valueText,
    disabled = false,
    onInput,
  }: Props = $props();

  const ratio = $derived(max > min ? Math.min(1, Math.max(0, (value - min) / (max - min))) : 0);

  function handleInput(event: Event & { currentTarget: HTMLInputElement }) {
    const next = Number(event.currentTarget.value);
    value = next;
    onInput?.(next);
  }
</script>

<label class={['scrub', `size-${size}`, { disabled }]}>
  {#if label}
    <span class="label">{label}</span>
  {/if}
  <span class="track-wrap">
    <span class="track" aria-hidden="true">
      <span class="fill" style:width="{ratio * 100}%"></span>
    </span>
    <span class="thumb" style:left="{ratio * 100}%" aria-hidden="true"></span>
    <input
      type="range"
      class="range"
      aria-label={label ?? 'Scrub'}
      aria-valuetext={valueText}
      {min}
      {max}
      {step}
      {value}
      {disabled}
      oninput={handleInput}
    />
  </span>
  {#if valueText}
    <span class="readout numeric">{valueText}</span>
  {/if}
</label>

<style>
  .scrub {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .size-md {
    width: 100%;
    max-width: var(--scrub-width-md);
  }

  .label {
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    white-space: nowrap;
  }

  .track-wrap {
    position: relative;
    display: flex;
    flex: 1;
    align-items: center;
    min-width: var(--scrub-width-sm);
    height: var(--control-height-xs);
  }

  .size-sm .track-wrap {
    width: var(--scrub-width-sm);
    flex: 0 0 var(--scrub-width-sm);
  }

  .track {
    position: relative;
    overflow: hidden;
    width: 100%;
    height: var(--scrub-track-height);
    border-radius: var(--scrub-track-radius);
    background: var(--color-surface-2);
  }

  .fill {
    display: block;
    height: 100%;
    border-radius: var(--scrub-track-radius);
    background: var(--color-text);
  }

  .thumb {
    position: absolute;
    top: 50%;
    width: var(--scrub-thumb-md);
    height: var(--scrub-thumb-md);
    border: var(--selection-bar-width) solid var(--color-surface);
    border-radius: var(--radius-pill);
    background: var(--color-text);
    transform: translate(-50%, -50%);
    pointer-events: none;
  }

  .size-sm .thumb {
    width: var(--scrub-thumb-sm);
    height: var(--scrub-thumb-sm);
  }

  .range {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .track-wrap:focus-within {
    border-radius: var(--radius-xs);
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: var(--focus-ring-offset);
  }

  .readout {
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-compact);
    white-space: nowrap;
  }

  .disabled {
    opacity: 0.5;
  }

  .disabled .range {
    cursor: default;
  }
</style>
