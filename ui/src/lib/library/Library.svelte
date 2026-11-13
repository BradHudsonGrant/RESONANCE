<script lang="ts">
  import Button from '../design/Button.svelte';
  import Wave from '../wave/Wave.svelte';
  import type { TrackPages } from './track-pages.svelte';
  import TrackTable from './TrackTable.svelte';

  interface Props {
    pages: TrackPages;
    selected?: number | null;
    onopen?: () => void;
    onscan: () => void;
  }

  let { pages, selected = $bindable(null), onopen, onscan }: Props = $props();
</script>

{#if pages.state.kind === 'loading'}
  <div class="loading" aria-busy="true">
    <p class="visually-hidden" role="status">Reading the library</p>
    {#each { length: 12 }, row (row)}
      <span class="skeleton" style:width="{40 + ((row * 37) % 45)}%"></span>
    {/each}
  </div>
{:else if pages.state.kind === 'error'}
  <div class="state" role="alert">
    <h2>The library could not be shown</h2>
    <p>{pages.state.message}</p>
    <Button onclick={() => pages.load()}>Try again</Button>
  </div>
{:else if pages.total === 0}
  <div class="empty-wave"><Wave /></div>
  <div class="state">
    <h2>No tracks yet</h2>
    <p>Add a music folder to see its tracks here. Nothing in the folder is changed.</p>
    <Button variant="primary" onclick={onscan}>Add a music folder</Button>
  </div>
{:else}
  <TrackTable {pages} bind:selected {onopen} />
{/if}

<style>
  .loading {
    display: grid;
    align-content: start;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-3);
  }

  .skeleton {
    height: 8px;
    border-radius: 4px;
    background: var(--color-surface-2);
  }

  .empty-wave {
    height: 240px;
  }

  .state {
    display: grid;
    justify-items: start;
    align-content: start;
    gap: var(--space-3);
    max-width: 34rem;
    padding: var(--space-8) var(--space-6);
  }

  .state h2 {
    font-size: var(--text-lg);
  }

  .state p {
    margin: 0;
    color: var(--color-text-2);
  }
</style>
