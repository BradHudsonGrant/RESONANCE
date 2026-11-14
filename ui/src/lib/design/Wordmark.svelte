<script lang="ts">
  import Mark from './Mark.svelte';

  interface Props {
    showMark?: boolean;
    size?: 'md' | 'lg';
    accentPeak?: boolean;
  }

  let { showMark = true, size = 'md', accentPeak = false }: Props = $props();
</script>

<span class={['wordmark', size]}>
  {#if showMark}
    <Mark size={size === 'lg' ? 'lg' : 'md'} {accentPeak} />
  {/if}
  <span class="name">Resonance</span>
</span>

<style>
  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: var(--wordmark-gap);
    color: var(--color-text);
  }

  .name {
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-medium);
    line-height: var(--leading-heading);
    letter-spacing: var(--tracking-wordmark);
    text-transform: uppercase;
  }

  .lg .name {
    font-size: var(--text-xl);
  }
</style>
