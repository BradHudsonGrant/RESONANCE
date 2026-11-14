<script lang="ts">
  import type { Snippet } from 'svelte';

  import Icon from './Icon.svelte';
  import { describe, type Origin, originFromSource, type ProvenanceSource } from './provenance';
  import Tooltip from './Tooltip.svelte';

  interface Props {
    origin?: Origin;
    source?: ProvenanceSource;
    provider?: string;
    confidence?: number;
    children?: Snippet;
  }

  let { origin, source, provider, confidence, children }: Props = $props();

  const resolvedOrigin = $derived(
    origin ?? originFromSource(source ?? 'file', provider, confidence),
  );
  const description = $derived(describe(resolvedOrigin));
  const low = $derived(resolvedOrigin.kind === 'estimated' && resolvedOrigin.low);
  const badgeText = $derived.by(() => {
    if (!source) return null;
    if (source === 'file') return 'FILE';
    if (source === 'user') return 'USER';
    const name = (provider ?? source).toUpperCase();
    return confidence !== undefined ? `${name} ${Math.round(confidence * 100)}%` : name;
  });
</script>

<span class={['value', { low, badge: !children }]}>
  {#if children}
    {@render children()}
  {:else if badgeText}
    <span class="tag">{badgeText}</span>
  {/if}
  {#if resolvedOrigin.kind === 'estimated' && description}
    <Tooltip text={description}>
      <span class="dot" role="img" aria-label={description}></span>
    </Tooltip>
  {:else if resolvedOrigin.kind === 'confirmed' && description}
    <Tooltip text={description}>
      <span class="tick" role="img" aria-label={description}><Icon name="check" size="xs" /></span>
    </Tooltip>
  {/if}
</span>

<style>
  .value {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .badge {
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-mono-wide);
    text-transform: uppercase;
  }

  .tag {
    color: var(--color-text-2);
  }

  .low,
  .low .tag {
    color: var(--color-muted);
  }

  .dot {
    width: var(--provenance-dot-size);
    height: var(--provenance-dot-size);
    border: var(--border-width) solid currentColor;
    border-radius: var(--radius-pill);
  }

  .tick {
    display: inline-flex;
    color: var(--color-text-2);
  }
</style>
