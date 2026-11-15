<script lang="ts">
  import Button from '../design/Button.svelte';
  import FilterPill from '../design/FilterPill.svelte';
  import Icon, { type IconName } from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import Wave from '../wave/Wave.svelte';
  import { trackCount } from './format';
  import type { TrackPages } from './track-pages.svelte';
  import TrackTable from './TrackTable.svelte';

  interface Props {
    pages: TrackPages;
    selected?: number | null;
    onopen?: () => void;
    onscan: () => void;
  }

  type FilterId = 'all' | 'analysed' | 'unanalysed' | 'favourites' | 'recent';

  interface Pill {
    id: FilterId;
    label: string;
    icon?: IconName;
    arrives?: string;
    note?: string;
  }

  const pills: Pill[] = [
    { id: 'all', label: 'All tracks' },
    {
      id: 'analysed',
      label: 'Analysed',
      icon: 'check',
      arrives: 'M5',
      note: 'Analysed tracks will show here once audio analysis lands in M5.',
    },
    {
      id: 'unanalysed',
      label: 'Unanalysed',
      icon: 'unanalysed',
      arrives: 'M5',
      note: 'Unanalysed tracks will show here once audio analysis lands in M5.',
    },
    {
      id: 'favourites',
      label: 'Favourites',
      icon: 'star',
      arrives: 'M2',
      note: 'Favourites arrive with library filters in M2.',
    },
    {
      id: 'recent',
      label: 'Recent',
      icon: 'recent',
      arrives: 'M2',
      note: 'Recently added tracks arrive with library filters in M2.',
    },
  ];

  let { pages, selected = $bindable(null), onopen, onscan }: Props = $props();
  let activeFilter = $state<FilterId>('all');

  const activePill = $derived(pills.find((pill) => pill.id === activeFilter) ?? pills[0]!);
</script>

<section class="library" aria-label="Library">
  <div class="header">
    <div class="title-row">
      <div class="title-group">
        <h1 class="heading">Library</h1>
        {#if pages.state.kind === 'ready'}
          <span class="count numeric">{trackCount(pages.total)}</span>
        {/if}
      </div>
      <div class="actions">
        {#if pages.state.kind === 'ready' && pages.total > 0}
          <Button variant="quiet" onclick={onscan}>Scan folder</Button>
        {/if}
        <Tooltip text="Filters arrive in M2" side="left">
          <Button aria-label="Filters" aria-disabled="true">
            <Icon name="sliders" size="sm" />Filters
          </Button>
        </Tooltip>
      </div>
    </div>

    <div class="pills" role="toolbar" aria-label="Track filters">
      {#each pills as pill (pill.id)}
        <FilterPill
          label={pill.label}
          icon={pill.icon}
          active={activeFilter === pill.id}
          onclick={() => (activeFilter = pill.id)}
        />
      {/each}
    </div>
  </div>

  <div class="stage">
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
      <div class="empty">
        <div class="empty-wave"><Wave /></div>
        <div class="state">
          <h2>No tracks yet</h2>
          <p>Add a music folder to see its tracks here. Nothing in the folder is changed.</p>
          <Button variant="primary" onclick={onscan}>Add a music folder</Button>
        </div>
      </div>
    {:else if activePill.arrives}
      <div class="state">
        <h2>{activePill.label} arrives in {activePill.arrives}</h2>
        <p>{activePill.note}</p>
        <Button onclick={() => (activeFilter = 'all')}>Show all tracks</Button>
      </div>
    {:else}
      <TrackTable {pages} bind:selected {onopen} />
    {/if}
  </div>
</section>

<style>
  .library {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .header {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5) var(--space-3);
  }

  .title-row {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .title-group {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-3);
  }

  .heading {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-2xl);
    font-weight: var(--weight-medium);
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tight);
  }

  .count {
    color: var(--color-text-2);
    font-size: var(--text-md);
  }

  .actions {
    display: flex;
    align-items: center;
    align-self: center;
    gap: var(--space-2);
  }

  .pills {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-6);
  }

  .stage {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .loading {
    display: grid;
    align-content: start;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-5);
  }

  .skeleton {
    height: var(--space-2);
    border-radius: var(--radius-xs);
    background: var(--color-surface-2);
  }

  .empty {
    display: grid;
    grid-template-rows: minmax(140px, 220px) auto;
    height: 100%;
    min-height: 0;
    overflow-y: auto;
  }

  .empty-wave {
    min-height: 0;
    mask-image: linear-gradient(to bottom, black 60%, transparent);
  }

  .state {
    display: grid;
    justify-items: start;
    align-content: start;
    gap: var(--space-3);
    max-width: 34rem;
    padding: var(--space-6) var(--space-5);
  }

  .state h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-medium);
  }

  .state p {
    margin: 0;
    color: var(--color-text-2);
  }
</style>
