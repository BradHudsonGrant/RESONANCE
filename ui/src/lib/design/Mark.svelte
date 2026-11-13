<script lang="ts">
  /** Seven bars tracing a resonance peak. Six take the text colour and the centre one the accent. */
  let { size = 24 }: { size?: number } = $props();
</script>

<svg viewBox="0 0 64 64" width={size} height={size} role="img" aria-label="Resonance">
  <g fill="currentColor">
    <rect x="6" y="28" width="4" height="8" rx="2" />
    <rect x="14" y="25" width="4" height="14" rx="2" />
    <rect x="22" y="17" width="4" height="30" rx="2" />
    <rect x="38" y="17" width="4" height="30" rx="2" />
    <rect x="46" y="25" width="4" height="14" rx="2" />
    <rect x="54" y="28" width="4" height="8" rx="2" />
  </g>
  <rect class="peak" x="30" y="8" width="4" height="48" rx="2" />
</svg>

<style>
  .peak {
    fill: var(--color-accent);
  }
</style>
