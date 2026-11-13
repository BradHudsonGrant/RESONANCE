<script lang="ts">
  import Specimen from './Specimen.svelte';
</script>

<!-- Every primitive, once in each theme, whatever the system is set to. -->
<main class="page">
  {#each ['dark', 'light'] as theme (theme)}
    <section class="theme" data-theme={theme} aria-label="{theme} theme">
      <Specimen {theme} />
    </section>
  {/each}
</main>

<style>
  .page {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(520px, 100%), 1fr));
    min-height: 100%;
  }

  .theme {
    padding: var(--space-6);
    background: var(--color-background);
    color: var(--color-text);
  }
</style>
