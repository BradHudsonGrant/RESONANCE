<script lang="ts">
  import Mark from './Mark.svelte';
</script>

<span class="wordmark"><Mark size={20} /><span class="name">Resonance</span></span>

<style>
  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .name {
    font-family: var(--font-heading);
    font-weight: 500;
    letter-spacing: 0.32em;
    text-transform: uppercase;
  }
</style>
