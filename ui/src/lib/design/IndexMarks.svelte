<script lang="ts">
  interface Props {
    index: number;
  }

  let { index }: Props = $props();
  const formatted = $derived(`[${String(Math.max(0, Math.trunc(index))).padStart(2, '0')}]`);
</script>

<span class="index-mark numeric">{formatted}</span>

<style>
  .index-mark {
    display: inline-block;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    font-weight: var(--weight-medium);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-mono-wide);
  }
</style>
