<script lang="ts">
  import Button from '../design/Button.svelte';
  import FilterPill from '../design/FilterPill.svelte';
  import GridDots from '../design/GridDots.svelte';
  import Icon, { type IconName } from '../design/Icon.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryFacets } from '../ipc/bindings/LibraryFacets';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { TrackFilterPill } from '../ipc/bindings/TrackFilterPill';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import FilterDrawer from './FilterDrawer.svelte';
  import { trackCount } from './format';
  import { countActiveDrawerFilters, hasQueryFilters, type TrackPages } from './track-pages.svelte';
  import TrackTable from './TrackTable.svelte';

  interface Props {
    pages: TrackPages;
    selected?: number | null;
    previewTrackId?: number | null;
    previewPlaying?: boolean;
    collections?: LibraryCollection[];
    activeCollection?: LibraryCollection | null;
    loadFacets?: () => Promise<LibraryFacets>;
    onopen?: () => void;
    onpreview?: (track: LibraryTrack) => void;
    onscan: () => void;
    ontogglefavourite?: (track: LibraryTrack) => void;
    onclearsearch?: () => void;
    onclearcollection?: () => void;
    onaddtocollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    onremovefromcollection?: (collectionId: number, trackIds: number[]) => Promise<void> | void;
    oncreatecollectionandadd?: (name: string, trackIds: number[]) => Promise<void> | void;
  }

  type FilterId = TrackFilterPill | 'analysed' | 'unanalysed';

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
    { id: 'favourites', label: 'Favourites', icon: 'star' },
    { id: 'recent', label: 'Recent', icon: 'recent' },
  ];

  let {
    pages,
    selected = $bindable(null),
    previewTrackId = null,
    previewPlaying = false,
    collections = [],
    activeCollection = null,
    loadFacets,
    onopen,
    onpreview,
    onscan,
    ontogglefavourite,
    onclearsearch,
    onclearcollection,
    onaddtocollection,
    onremovefromcollection,
    oncreatecollectionandadd,
  }: Props = $props();

  let m5Gate = $state<'analysed' | 'unanalysed' | null>(null);
  let drawerOpen = $state(false);
  let facets = $state<LibraryFacets | null>(null);

  const activeFilter = $derived<FilterId>(m5Gate ?? pages.query.pill ?? 'all');
  const activePill = $derived(pills.find((pill) => pill.id === activeFilter) ?? pills[0]!);
  const drawerFilterCount = $derived(countActiveDrawerFilters(pages.query));

  function selectPill(pill: Pill) {
    if (pill.id === 'analysed' || pill.id === 'unanalysed') {
      if (m5Gate === pill.id) {
        m5Gate = null;
      } else {
        m5Gate = pill.id;
      }
      return;
    }
    m5Gate = null;
    const current = pages.query.pill ?? 'all';
    const next: TrackFilterPill = current === pill.id ? 'all' : pill.id;
    void pages.setQuery({ ...pages.query, pill: next });
  }

  function clearDrawerFilters() {
    void pages.setQuery({
      ...pages.query,
      bpm_min: null,
      bpm_max: null,
      keys: null,
      year_min: null,
      year_max: null,
      label: null,
      formats: null,
    });
  }

  function clearAllFilters() {
    m5Gate = null;
    onclearsearch?.();
    const keepContext: TrackQuery = {};
    if (pages.query.collection_id != null) keepContext.collection_id = pages.query.collection_id;
    if (pages.query.sort) keepContext.sort = pages.query.sort;
    if (pages.query.direction) keepContext.direction = pages.query.direction;
    void pages.setQuery(keepContext);
  }

  const hasFiltersInView = $derived(hasQueryFilters(pages.query));

  function openDrawer() {
    drawerOpen = true;
    if (!loadFacets) return;
    void loadFacets()
      .then((next) => {
        facets = next;
      })
      .catch(() => {
        // Leave stale facets untouched if the library read fails cleanly.
      });
  }
</script>

<section class="library" aria-label="Library">
  <div class="header">
    <div class="title-row">
      <div class="title-group">
        {#if activeCollection}
          <div class="breadcrumb">
            <button
              type="button"
              class="crumb-root"
              aria-label="Back to Library"
              onclick={() => onclearcollection?.()}
            >
              Library
            </button>
            <span class="crumb-sep" aria-hidden="true">
              <Icon name="chevron-right" size="xs" />
            </span>
            <h1 class="heading">{activeCollection.name}</h1>
          </div>
          {#if pages.state.kind === 'ready'}
            <span class="count numeric">{trackCount(pages.total)}</span>
          {/if}
        {:else}
          <h1 class="heading">Library</h1>
          {#if pages.state.kind === 'ready' && pages.libraryTotal > 0}
            <span class="count numeric">{trackCount(pages.total)}</span>
          {/if}
        {/if}
      </div>
      {#if pages.state.kind === 'ready' && pages.libraryTotal > 0}
        <div class="actions">
          <Button variant="quiet" onclick={onscan}>Scan folder</Button>
          <div class="filter-action">
            <Tooltip
              text={drawerFilterCount > 0 ? `Filters (${drawerFilterCount} active)` : 'Filters'}
              side="left"
            >
              <Button
                variant="icon"
                size="lg"
                aria-label="Filters"
                aria-pressed={drawerOpen || drawerFilterCount > 0}
                onclick={openDrawer}
              >
                <Icon name="sliders" size="sm" />
              </Button>
            </Tooltip>
            {#if drawerFilterCount > 0}
              <span class="filter-count numeric" aria-hidden="true">{drawerFilterCount}</span>
            {/if}
          </div>
        </div>
      {/if}
    </div>

    {#if pages.state.kind !== 'ready' || pages.libraryTotal > 0}
      <div class="pills" role="toolbar" aria-label="Track filters">
        {#each pills as pill (pill.id)}
          <FilterPill
            label={pill.label}
            icon={pill.icon}
            active={activeFilter === pill.id}
            onclick={() => selectPill(pill)}
          />
        {/each}
      </div>
    {/if}
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
    {:else if pages.libraryTotal === 0}
      <div class="empty">
        <GridDots spacing={24}>
          <div class="empty-card">
            <h2>No tracks yet</h2>
            <p>Add a music folder to see its tracks here. Nothing in the folder is changed.</p>
            <Button variant="primary" onclick={onscan}>Add a music folder</Button>
          </div>
        </GridDots>
      </div>
    {:else if activePill.arrives}
      <div class="state">
        <h2>{activePill.label} arrives in {activePill.arrives}</h2>
        <p>{activePill.note}</p>
        <Button onclick={() => (m5Gate = null)}>Show all tracks</Button>
      </div>
    {:else if pages.total === 0}
      {#if activeCollection && !hasFiltersInView}
        <div class="state">
          <h2>This collection is empty</h2>
          <p>Add tracks from Library using row actions, the C shortcut or the Inspector.</p>
          <Button onclick={() => onclearcollection?.()}>Show all tracks</Button>
        </div>
      {:else}
        <div class="state">
          <h2>No tracks match</h2>
          <p>Try clearing the search text or adjusting your filters.</p>
          <Button onclick={clearAllFilters}>Clear filters</Button>
        </div>
      {/if}
    {:else}
      <TrackTable
        {pages}
        bind:selected
        {previewTrackId}
        {previewPlaying}
        {collections}
        {activeCollection}
        {onopen}
        {onpreview}
        {ontogglefavourite}
        {onaddtocollection}
        {onremovefromcollection}
        {oncreatecollectionandadd}
        onsort={(sort, direction) => void pages.setQuery({ ...pages.query, sort, direction })}
      />
    {/if}
  </div>
</section>

<FilterDrawer
  bind:open={drawerOpen}
  query={pages.query}
  {facets}
  onchange={(next) => void pages.setQuery(next)}
  onclear={clearDrawerFilters}
/>

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
    min-width: 0;
  }

  .breadcrumb {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .crumb-root {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--color-text-2);
    font-family: var(--font-display);
    font-size: var(--text-xl);
    font-weight: var(--weight-medium);
    line-height: var(--leading-tight);
    cursor: pointer;
    transition: color var(--duration-fast) var(--ease-out);
  }

  .crumb-root:hover {
    color: var(--color-text);
  }

  .crumb-sep {
    display: inline-flex;
    align-self: center;
    color: var(--color-muted);
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

  .filter-action {
    position: relative;
    display: inline-flex;
  }

  .filter-count {
    position: absolute;
    top: -4px;
    right: -4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border: var(--border-width) solid var(--color-background);
    border-radius: var(--radius-pill);
    background: var(--color-text);
    color: var(--color-background);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    line-height: 1;
    pointer-events: none;
  }

  @media (forced-colors: active) {
    .filter-count {
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }
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
    height: 100%;
    min-height: 0;
    padding: var(--space-2) var(--space-5) var(--space-5);
  }

  .empty :global(.grid-dots) {
    display: flex;
    align-items: center;
    height: 100%;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-lg);
  }

  .empty-card {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    max-width: 32rem;
    padding: var(--space-6);
  }

  .empty-card h2,
  .state h2 {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--text-lg);
    font-weight: var(--weight-medium);
  }

  .empty-card p,
  .state p {
    margin: 0;
    color: var(--color-text-2);
  }

  .state {
    display: grid;
    justify-items: start;
    align-content: start;
    gap: var(--space-3);
    max-width: 34rem;
    padding: var(--space-6) var(--space-5);
  }
</style>
