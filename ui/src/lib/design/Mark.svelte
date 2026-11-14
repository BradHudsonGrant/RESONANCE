<script lang="ts">
  interface Props {
    size?: 'sm' | 'md' | 'lg' | number;
    accentPeak?: boolean;
  }

  let { size = 'md', accentPeak = false }: Props = $props();

  const dimensions = $derived.by(() => {
    if (typeof size === 'number') return { width: size, height: size };
    if (size === 'sm') return { width: 16, height: 16 };
    if (size === 'lg') return { width: 32, height: 36 };
    return { width: 19, height: 21 };
  });
</script>

<svg
  class="mark"
  viewBox="0 0 26 28"
  width={dimensions.width}
  height={dimensions.height}
  role="img"
  aria-label="Resonance"
>
  <g fill="currentColor">
    <rect x="0" y="9" width="2" height="10" rx="1" />
    <rect x="4" y="5.5" width="2" height="17" rx="1" />
    <rect x="8" y="2" width="2" height="24" rx="1" />
    <rect x="16" y="4" width="2" height="20" rx="1" />
    <rect x="20" y="7.5" width="2" height="13" rx="1" />
    <rect x="24" y="10.5" width="2" height="7" rx="1" />
  </g>
  <rect class={['bar-peak', { accent: accentPeak }]} x="12" y="0" width="2" height="28" rx="1" />
</svg>

<style>
  .mark {
    display: inline-block;
    flex-shrink: 0;
    color: var(--color-text);
  }

  .bar-peak {
    fill: currentColor;
  }

  .accent {
    fill: var(--color-accent);
  }
</style>
