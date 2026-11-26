<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import FilterPill from '../design/FilterPill.svelte';
  import GridDots from '../design/GridDots.svelte';
  import Icon, { type IconName } from '../design/Icon.svelte';
  import Sheet from '../design/Sheet.svelte';
  import Tooltip from '../design/Tooltip.svelte';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
  import type { LibraryFacets } from '../ipc/bindings/LibraryFacets';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { RenameScope } from '../ipc/bindings/RenameScope';
  import type { FilterTerm } from '../ipc/bindings/FilterTerm';
  import type { TrackFilterPill } from '../ipc/bindings/TrackFilterPill';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import FilterDrawer from './FilterDrawer.svelte';
  import { FilterState, termsOf, withTerms } from './filters.svelte';
  import QuickFilterBar from './QuickFilterBar.svelte';
  import { trackCount } from './format';
  import { sameSort, sortQuery, TableLayoutState } from './table-layout.svelte';
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
    /** The table's columns, widths, wrap and sort, kept by the core. */
    tableLayout?: TableLayoutState;
    /** Quick filters, token words and smart views, kept by the core. */
    filters?: FilterState;
    /** The hook for sending a filter's tracks to Prep, wired when Prep lands. */
    onsendtoprep?: (query: TrackQuery, label: string) => void;
    onopeninprep?: (trackIds: number[]) => void;
    onidentifyonline?: (trackIds: number[]) => void;
    /** Opens the batch tag tools sheet for the selected tracks. */
    onbatchtag?: (trackIds: number[]) => void;
    onopen?: () => void;
    onpreview?: (track: LibraryTrack) => void;
    onplay?: (track: LibraryTrack) => void;
    onfocustrack?: (track: LibraryTrack) => void;
    onrelink?: (track: LibraryTrack) => void;
    onreplace?: (track: LibraryTrack) => void;
    /** Opens the Rename sheet for `scope`, named `label`. */
    onrename?: (scope: RenameScope, label: string) => void;
    /** Asks before moving the selected tracks' files to the bin. */
    ondelete?: (trackIds: number[]) => void;
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
    tableLayout = new TableLayoutState(),
    filters = new FilterState(),
    onsendtoprep,
    onopeninprep,
    onidentifyonline,
    onbatchtag,
    onopen,
    onpreview,
    onplay,
    onfocustrack,
    onrelink,
    onreplace,
    onrename,
    ondelete,
    onscan,
    ontogglefavourite,
    onclearsearch,
    onclearcollection,
    onaddtocollection,
    onremovefromcollection,
    oncreatecollectionandadd,
  }: Props = $props();

  let table: TrackTable | undefined = $state();
  const progressiveStartup = untrack(() => pages.state.kind !== 'ready');

  $effect(() => {
    void tableLayout.load().catch(() => {
      // The table keeps the default columns until the core can be read.
    });
  });

  // The kept sort is the table's, so the query follows it whenever it changes.
  $effect(() => {
    const wanted = sortQuery(tableLayout.current);
    untrack(() => {
      if (!sameSort(pages.query, wanted)) void pages.setQuery({ ...pages.query, ...wanted });
    });
  });
  let checkedCount = $state(0);
  let allChecked = $state(false);
  const selecting = $derived(checkedCount > 0);

  let m5Gate = $state<'analysed' | 'unanalysed' | null>(null);
  let drawerOpen = $state(false);
  let drawerMounted = $state(false);
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
    void pages.setQuery({ ...pages.query, filters: null, missing_only: null });
  }

  const terms = $derived(termsOf(pages.query));
  const summaries = $derived(terms.map((term) => filters.summaryFor(term)));
  let startColumn = $state<string | null>(null);
  let saveOpen = $state(false);
  let saveMounted = $state(false);
  let saveName = $state('');
  let saveError = $state<string | null>(null);

  $effect(() => {
    const readFilters = () => {
      void filters.load().catch(() => {
        // The bar shows no quick filters until the core can be read.
      });
      void filters.loadFields().catch(() => {
        // The header and cell menus offer no filters until the core can be read.
      });
    };
    if (!progressiveStartup) {
      readFilters();
      return;
    }
    const timer = window.setTimeout(readFilters, 0);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    const query = pages.query;
    untrack(() => filters.refreshSummaries(query));
  });

  function addFilter(term: FilterTerm) {
    filters.add(pages, term);
  }

  /** Opens the drawer with a new condition on `column`. */
  function filterByColumn(column: string) {
    startColumn = column;
    openDrawer();
  }

  function removeFilter(index: number) {
    void pages.setQuery(
      withTerms(
        pages.query,
        terms.filter((_, at) => at !== index),
      ),
    );
  }

  function openSave() {
    saveName = summaries
      .map((summary) => summary?.label)
      .filter(Boolean)
      .join(', ')
      .slice(0, 60);
    saveError = null;
    saveMounted = true;
    saveOpen = true;
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    saveError = null;
    try {
      await filters.saveView(saveName, {
        text: pages.query.text ?? null,
        pill: pages.query.pill ?? null,
        filters: pages.query.filters ?? null,
      });
      saveOpen = false;
    } catch (error) {
      saveError = error instanceof Error ? error.message : String(error);
    }
  }

  function clearAllFilters() {
    m5Gate = null;
    onclearsearch?.();
    const keepContext: TrackQuery = {};
    if (pages.query.collection_id != null) keepContext.collection_id = pages.query.collection_id;
    if (pages.query.sort) keepContext.sort = pages.query.sort;
    if (pages.query.direction) keepContext.direction = pages.query.direction;
    if (pages.query.then_sort) keepContext.then_sort = pages.query.then_sort;
    if (pages.query.then_direction) keepContext.then_direction = pages.query.then_direction;
    void pages.setQuery(keepContext);
  }

  const hasFiltersInView = $derived(hasQueryFilters(pages.query));

  function openDrawer() {
    drawerMounted = true;
    void filters.loadFields().then(() => (drawerOpen = true));
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
          {#if onrename}
            <Button
              variant="quiet"
              onclick={() =>
                onrename(
                  { kind: 'query', query: pages.query },
                  activeCollection?.name ??
                    (hasFiltersInView ? 'Tracks in this view' : 'Whole library'),
                )}
            >
              Rename files
            </Button>
          {/if}
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
      <!-- The pills and the selection actions share one cell, and the hidden pills keep its
           height, so swapping them never moves the table. -->
      <div class="toolbar-slot">
        <div
          class={['pills', { covered: selecting }]}
          role="toolbar"
          aria-label="Track filters"
          inert={selecting}
        >
          {#each pills as pill (pill.id)}
            <FilterPill
              label={pill.label}
              icon={pill.icon}
              active={activeFilter === pill.id}
              onclick={() => selectPill(pill)}
            />
          {/each}
          {#if filters.available}
            <span class="bar-rule" aria-hidden="true"></span>
            <QuickFilterBar
              {terms}
              {summaries}
              total={pages.total}
              quick={filters.quick}
              onadd={addFilter}
              onremove={removeFilter}
              onedit={openDrawer}
              onsave={openSave}
              onclear={clearDrawerFilters}
              onchange={(change) => void filters.change(change)}
              onsendtoprep={onsendtoprep
                ? () =>
                    onsendtoprep(
                      pages.query,
                      summaries
                        .map((summary) => summary?.label)
                        .filter(Boolean)
                        .join(', ') || 'Filtered tracks',
                    )
                : undefined}
            />
          {/if}
        </div>
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <div
          class="selection"
          role="region"
          aria-label="Selected tracks"
          hidden={!selecting}
          onkeydown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault();
              table?.clearSelection();
            }
          }}
        >
          <span class="selection-count numeric" aria-live="polite">{checkedCount} selected</span>
          <Button
            variant="quiet"
            size="sm"
            aria-disabled={allChecked}
            onclick={() => {
              if (!allChecked) table?.selectAllLoaded();
            }}
          >
            Select all
          </Button>
          <Button
            size="sm"
            onclick={(event) => table?.addSelectionToCollection(event.currentTarget)}
          >
            Add to collection
          </Button>
          {#if onopeninprep}
            <Button size="sm" onclick={() => onopeninprep(table?.checkedTrackIds() ?? [])}>
              Open in Prep
            </Button>
          {/if}
          {#if onidentifyonline}
            <Button size="sm" onclick={() => onidentifyonline(table?.checkedTrackIds() ?? [])}>
              Look up online
            </Button>
          {/if}
          {#if onbatchtag}
            <Button size="sm" onclick={() => onbatchtag(table?.checkedTrackIds() ?? [])}>
              Batch tags
            </Button>
          {/if}
          {#if onrename}
            <Button
              size="sm"
              onclick={() =>
                onrename(
                  { kind: 'tracks', track_ids: table?.checkedTrackIds() ?? [] },
                  'Selected tracks',
                )}
            >
              Rename
            </Button>
          {/if}
          {#if ondelete}
            <Button size="sm" onclick={() => ondelete(table?.checkedTrackIds() ?? [])}>
              Move to bin
            </Button>
          {/if}
          {#if activeCollection && !activeCollection.mirror}
            <Button size="sm" onclick={() => table?.removeSelectionFromCollection()}>
              <span class="collection-name">Remove from {activeCollection.name}</span>
            </Button>
          {/if}
          <Button variant="quiet" size="sm" onclick={() => table?.clearSelection()}>Clear</Button>
        </div>
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
      {#if activeCollection?.mirror && !hasFiltersInView}
        <div class="state">
          <h2>No tracks from this playlist are in your library</h2>
          <p>Scan the folders its files are in, then read the playlist again.</p>
          <Button onclick={() => onclearcollection?.()}>Show all tracks</Button>
        </div>
      {:else if activeCollection && !hasFiltersInView}
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
        bind:this={table}
        bind:checkedCount
        bind:allChecked
        {pages}
        {progressiveStartup}
        bind:selected
        {previewTrackId}
        {previewPlaying}
        {collections}
        {activeCollection}
        {onopen}
        {onpreview}
        {onplay}
        {onfocustrack}
        {onrelink}
        {onreplace}
        {onopeninprep}
        {onidentifyonline}
        {ontogglefavourite}
        {onaddtocollection}
        {onremovefromcollection}
        {oncreatecollectionandadd}
        layout={tableLayout}
        filterFields={filters.fields}
        onfilter={addFilter}
        onfiltercolumn={filterByColumn}
      />
    {/if}
  </div>
</section>

{#if drawerMounted}
  <FilterDrawer
    bind:open={drawerOpen}
    query={pages.query}
    fields={filters.fields}
    {facets}
    {startColumn}
    onchange={(next) => void pages.setQuery(next)}
    onadded={(added) => {
      startColumn = null;
      for (const term of added) void filters.recordUse(term);
    }}
    onclear={clearDrawerFilters}
  />
{/if}

{#if saveMounted}
  <Sheet bind:open={saveOpen} title="Save as smart view">
    <form id="save-view" class="save-form" onsubmit={save}>
      <label class="save-label" for="save-view-name">Name</label>
      <input id="save-view-name" class="save-input" type="text" bind:value={saveName} />
      <p class="save-note">
        The view keeps the filters, search and pill, so new tracks that match show in it.
      </p>
      {#if saveError}<p class="save-error" role="alert">{saveError}</p>{/if}
    </form>
    {#snippet actions()}
      <Button onclick={() => (saveOpen = false)}>Cancel</Button>
      <Button variant="primary" type="submit" form="save-view">Save view</Button>
    {/snippet}
  </Sheet>
{/if}

<style>
  .library {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .header {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-4);
    min-width: 0;
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

  .toolbar-slot {
    display: grid;
    min-width: 0;
  }

  .pills,
  .selection {
    grid-area: 1 / 1;
    min-width: 0;
  }

  /* One line that scrolls sideways when it is too long, so tokens and chips never push the
     table down. */
  .pills {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: var(--space-2-5);
    height: var(--control-height-sm);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .pills::-webkit-scrollbar {
    display: none;
  }

  .pills > :global(*) {
    flex: 0 0 auto;
  }

  .pills > :global(.filter-pill + .filter-pill) {
    margin-left: var(--space-3-5);
  }

  .bar-rule {
    width: var(--border-width);
    height: var(--space-3-5);
    margin: 0 var(--space-1-5);
    background: var(--color-border);
  }

  .save-form {
    display: grid;
    gap: var(--space-2);
  }

  .save-label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .save-input {
    height: var(--control-height-md);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-md);
  }

  .save-note,
  .save-error {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .save-error {
    color: var(--color-text);
  }

  @media (forced-colors: active) {
    .bar-rule {
      background: CanvasText;
    }
  }

  .pills.covered {
    visibility: hidden;
  }

  .selection {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    align-self: start;
    gap: var(--space-2);
    min-width: 0;
    height: var(--control-height-sm);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .selection::-webkit-scrollbar {
    display: none;
  }

  .selection > :global(*) {
    flex: 0 0 auto;
  }

  .selection[hidden] {
    display: none;
  }

  .selection-count {
    min-width: 11ch;
    margin-right: var(--space-2);
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    white-space: nowrap;
  }

  .collection-name {
    display: inline-block;
    max-width: 16rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (forced-colors: active) {
    .selection {
      padding-left: var(--space-2);
      border-left: var(--selection-bar-width) solid Highlight;
    }
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
