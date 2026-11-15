<script lang="ts">
  import Sheet from './lib/design/Sheet.svelte';
  import type { LibraryCollection } from './lib/ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from './lib/ipc/bindings/LibraryTrack';
  import { core } from './lib/ipc/core';
  import Inspector from './lib/library/Inspector.svelte';
  import Library from './lib/library/Library.svelte';
  import ScanFolder from './lib/library/ScanFolder.svelte';
  import type { SourcesClient } from './lib/library/SourcesSettings.svelte';
  import { TrackPages } from './lib/library/track-pages.svelte';
  import CommandBar, { type Command } from './lib/shell/CommandBar.svelte';
  import PlayerBar from './lib/shell/PlayerBar.svelte';
  import { PreviewPlayer } from './lib/shell/preview.svelte';
  import SettingsSheet from './lib/shell/SettingsSheet.svelte';
  import Sidebar, { type SpaceName } from './lib/shell/Sidebar.svelte';
  import TopBar from './lib/shell/TopBar.svelte';
  import TransitionDock from './lib/shell/TransitionDock.svelte';

  type Fold = 'collapsed' | 'expanded';

  const commandInput = 'command-bar';
  const pages = new TrackPages((offset, limit, query) =>
    core.call('tracks', { offset, limit, query }),
  );
  const preview = new PreviewPlayer(core);
  const sourcesClient: SourcesClient = {
    sources: () => core.call('sources'),
    subfolders: (source_id, parent) => core.call('source_subfolders', { source_id, parent }),
    scan: (folder) => core.call('scan', { folder }),
    updateSource: (params) => core.call('update_source', params),
    removeSource: (source_id) => core.call('remove_source', { source_id }),
    setIgnoredFolders: (folders) => core.call('set_ignored_folders', { folders }),
  };

  let shell: HTMLDivElement;
  let space = $state<SpaceName>('Library');
  // The window size sets the layout in CSS. These hold only what the user chose over it.
  let sidebarChoice = $state<Fold>();
  let dockChoice = $state<Fold>();
  let inspectorHidden = $state(false);
  let sheetOpen = $state(false);
  let settingsOpen = $state(false);
  let settingsMounted = $state(false);
  let scanOpen = $state(false);
  let scanMounted = $state(false);
  let sourcesRevision = $state(0);
  let mixPlaying = $state(false);
  let searchText = $state('');
  let collections = $state<LibraryCollection[]>([]);
  let activeCollectionId = $state<number | null>(null);

  const activeCollection = $derived(
    collections.find((collection) => collection.id === activeCollectionId) ?? null,
  );

  function setSearch(text: string) {
    searchText = text;
    const nextText = text.trim() || null;
    if ((pages.query.text ?? null) !== nextText) {
      void pages.setQuery({ ...pages.query, text: nextText });
    }
  }

  function clearSearch() {
    searchText = '';
  }

  function setBpmFilter(min: number | null, max: number | null) {
    void pages.setQuery({ ...pages.query, bpm_min: min, bpm_max: max });
  }

  async function toggleFavourite(target: LibraryTrack) {
    const next = !target.favourite;
    pages.setLocalFavourite(target.id, next);
    try {
      const updated = await core.call('set_track_favourite', {
        track_id: target.id,
        favourite: next,
      });
      pages.setLocalFavourite(target.id, updated.favourite);
      if (pages.query.pill === 'favourites' && !updated.favourite) {
        await pages.refresh();
      }
    } catch {
      pages.setLocalFavourite(target.id, target.favourite);
    }
  }

  async function loadCollections() {
    try {
      const list = await core.call('collections');
      collections = list;
      if (activeCollectionId !== null && !list.some((col) => col.id === activeCollectionId)) {
        activeCollectionId = null;
        if (pages.query.collection_id != null) {
          void pages.setQuery({ ...pages.query, collection_id: null });
        }
      }
    } catch {
      // Initial library load handles core error states; ignore transient collection fetch failure.
    }
  }

  function mergeCollectionSummary(updated: LibraryCollection) {
    const exists = collections.some((col) => col.id === updated.id);
    collections = exists
      ? collections.map((col) => (col.id === updated.id ? updated : col))
      : [...collections, updated];
  }

  function selectCollection(collectionId: number | null) {
    activeCollectionId = collectionId;
    if ((pages.query.collection_id ?? null) !== collectionId) {
      void pages.setQuery({ ...pages.query, collection_id: collectionId });
    }
  }

  async function createCollection(name: string): Promise<LibraryCollection> {
    const created = await core.call('create_collection', { name });
    mergeCollectionSummary(created);
    return created;
  }

  async function renameCollection(collectionId: number, name: string) {
    const updated = await core.call('rename_collection', {
      collection_id: collectionId,
      name,
    });
    mergeCollectionSummary(updated);
    pages.renameLocalCollection(updated.id, updated.name);
  }

  async function deleteCollection(collectionId: number) {
    const remaining = await core.call('delete_collection', {
      collection_id: collectionId,
    });
    collections = remaining;
    pages.removeLocalCollection(collectionId);
    if (activeCollectionId === collectionId) {
      activeCollectionId = null;
      await pages.setQuery({ ...pages.query, collection_id: null });
    }
  }

  async function addTracksToCollection(collectionId: number, trackIds: number[]) {
    if (trackIds.length === 0) return;
    const updated = await core.call('add_tracks_to_collection', {
      collection_id: collectionId,
      track_ids: trackIds,
    });
    mergeCollectionSummary(updated);
    pages.addTracksToLocalCollection(trackIds, { id: updated.id, name: updated.name });
    if (activeCollectionId === collectionId) {
      await pages.refresh();
    }
  }

  async function removeTracksFromCollection(collectionId: number, trackIds: number[]) {
    if (trackIds.length === 0) return;
    const updated = await core.call('remove_tracks_from_collection', {
      collection_id: collectionId,
      track_ids: trackIds,
    });
    mergeCollectionSummary(updated);
    pages.removeTracksFromLocalCollection(collectionId, trackIds);
    if (activeCollectionId === collectionId) {
      await pages.refresh();
    }
  }

  async function createCollectionAndAdd(name: string, trackIds: number[]) {
    const created = await createCollection(name);
    await addTracksToCollection(created.id, trackIds);
  }

  function openSettings() {
    settingsMounted = true;
    settingsOpen = true;
  }

  function openScan() {
    scanMounted = true;
    scanOpen = true;
  }

  const track = $derived(pages.selectedTrack);
  const inspectorDocked = $derived(space === 'Library' && !inspectorHidden);

  /** What the stylesheet decided, read only when the user asks for the opposite. */
  function layout(property: '--sidebar-state' | '--dock-state' | '--inspector-mode'): string {
    return getComputedStyle(shell).getPropertyValue(property).trim();
  }

  function flipped(choice: Fold | undefined, property: '--sidebar-state' | '--dock-state'): Fold {
    return (choice ?? layout(property)) === 'collapsed' ? 'expanded' : 'collapsed';
  }

  function toggleSidebar() {
    sidebarChoice = flipped(sidebarChoice, '--sidebar-state');
  }

  function toggleDock() {
    dockChoice = flipped(dockChoice, '--dock-state');
  }

  function toggleInspector() {
    if (layout('--inspector-mode') === 'sheet') sheetOpen = !sheetOpen;
    else inspectorHidden = !inspectorHidden;
  }

  function showInspector() {
    if (layout('--inspector-mode') === 'sheet') sheetOpen = true;
    else inspectorHidden = false;
  }

  $effect(() => {
    if (!sheetOpen) return;
    const closeWhenDocked = () => {
      if (layout('--inspector-mode') !== 'sheet') sheetOpen = false;
    };
    addEventListener('resize', closeWhenDocked);
    return () => removeEventListener('resize', closeWhenDocked);
  });

  $effect(() => () => preview.destroy());

  let previousSpace: SpaceName = 'Library';
  $effect(() => {
    const next = space;
    if (next === 'Mix' && previousSpace !== 'Mix' && preview.playing) {
      void preview.pause();
    }
    previousSpace = next;
  });

  $effect(() =>
    core.onLibraryChange(() => {
      sourcesRevision += 1;
      void pages.refresh();
      void loadCollections();
    }),
  );

  const commands: Command[] = $derived([
    space === 'Mix'
      ? { name: 'Go to the library', shortcut: '1', run: () => (space = 'Library') }
      : { name: 'Go to Mix', shortcut: '4', run: () => (space = 'Mix') },
    { name: 'Scan a music folder', run: openScan },
    { name: 'Manage library sources', run: openSettings },
    { name: 'Show or hide the inspector', shortcut: 'I', run: toggleInspector },
    { name: 'Collapse or expand the sidebar', shortcut: '[', run: toggleSidebar },
    { name: 'Collapse or expand the dock', shortcut: 'D', run: toggleDock },
    { name: 'Open settings', run: openSettings },
    { name: 'Go to the first track', shortcut: 'Home', run: () => goTo(0) },
    { name: 'Go to the last track', shortcut: 'End', run: () => goTo(pages.total - 1) },
    { name: 'Reload the library', run: reload },
  ]);

  function goTo(index: number) {
    if (pages.total > 0) pages.selected = index;
  }

  function reload() {
    void pages.load();
    void loadCollections();
  }

  function onScanned() {
    sourcesRevision += 1;
    void pages.refresh();
    void loadCollections();
  }

  function typing(target: EventTarget | null): boolean {
    return (
      target instanceof HTMLElement &&
      (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
    );
  }

  function interactiveControl(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;
    return (
      typing(target) ||
      Boolean(
        target.closest(
          'button, [role="button"], [role="combobox"], [role="menuitem"], [role="menuitemcheckbox"], [role="option"], [role="tab"], [role="checkbox"], a[href]',
        ),
      )
    );
  }

  const keys: Record<string, () => void> = {
    '/': () => document.getElementById(commandInput)?.focus(),
    i: toggleInspector,
    '[': toggleSidebar,
    d: toggleDock,
    '1': () => (space = 'Library'),
    '4': () => (space = 'Mix'),
  };

  function onkeydown(event: KeyboardEvent) {
    // An open sheet or menu owns the keyboard, so nothing changes behind it.
    if (event.altKey || document.querySelector('dialog[open], [role="menu"]')) return;
    const key = event.key.toLowerCase();
    const command = event.ctrlKey || event.metaKey;

    if (command && key === 'k') {
      event.preventDefault();
      document.getElementById(commandInput)?.focus();
    } else if (command && (key === '1' || key === '4')) {
      event.preventDefault();
      keys[key]?.();
    } else if (
      event.key === ' ' &&
      space === 'Library' &&
      !command &&
      !interactiveControl(event.target)
    ) {
      if (preview.trackId !== null) {
        event.preventDefault();
        void preview.togglePlay();
      } else if (track) {
        event.preventDefault();
        void preview.loadAndPlay(track.id);
      }
    } else if (!command && !typing(event.target) && keys[key]) {
      event.preventDefault();
      keys[key]();
    }
  }

  pages.load();
  void loadCollections();
  void preview.refresh();
</script>

<svelte:window {onkeydown} />

<div
  bind:this={shell}
  class="app-shell"
  data-sidebar={sidebarChoice}
  data-dock={dockChoice}
  data-inspector={inspectorDocked ? undefined : 'hidden'}
>
  <div class="topbar">
    <TopBar onsettings={openSettings} />
  </div>

  <div class="sidebar">
    <Sidebar
      activeSpace={space}
      trackCount={pages.state.kind === 'ready' ? pages.libraryTotal : null}
      wavePaused={space === 'Mix' && mixPlaying}
      {collections}
      {activeCollectionId}
      onSelectSpace={(nextSpace) => (space = nextSpace)}
      onSelectCollection={selectCollection}
      onCreateCollection={async (name) => {
        await createCollection(name);
      }}
      onRenameCollection={renameCollection}
      onDeleteCollection={deleteCollection}
      onCollapse={(collapsed) => (sidebarChoice = collapsed ? 'collapsed' : 'expanded')}
    />
  </div>

  <div class="workspace">
    <CommandBar
      {commands}
      inputId={commandInput}
      search={searchText}
      bpmActive={pages.query.bpm_min != null || pages.query.bpm_max != null}
      onsearch={setSearch}
      onbpm={setBpmFilter}
    />
    <main class="workspace-content">
      {#if space === 'Mix'}
        {#await import('./lib/mix/Mix.svelte') then { default: Mix }}
          <Mix onplayingchange={(playing) => (mixPlaying = playing)} />
        {/await}
      {:else}
        <Library
          {pages}
          bind:selected={pages.selected}
          previewTrackId={preview.trackId}
          previewPlaying={preview.playing}
          {collections}
          {activeCollection}
          loadFacets={() => core.call('library_facets')}
          onopen={showInspector}
          onpreview={(item) => void preview.toggleOrPlayTrack(item.id)}
          onscan={openScan}
          ontogglefavourite={toggleFavourite}
          onclearsearch={clearSearch}
          onclearcollection={() => selectCollection(null)}
          onaddtocollection={addTracksToCollection}
          onremovefromcollection={removeTracksFromCollection}
          oncreatecollectionandadd={createCollectionAndAdd}
        />
      {/if}
    </main>
  </div>

  {#if inspectorDocked}
    <div class="inspector-dock">
      <Inspector
        {track}
        previewTrackId={preview.trackId}
        previewPlaying={preview.playing}
        {collections}
        {activeCollection}
        onclose={() => (inspectorHidden = true)}
        onpreview={(item) => void preview.toggleOrPlayTrack(item.id)}
        ontogglefavourite={toggleFavourite}
        onaddtocollection={addTracksToCollection}
        onremovefromcollection={removeTracksFromCollection}
        oncreatecollectionandadd={createCollectionAndAdd}
      />
    </div>
  {/if}

  <!-- The Mix spike has its own decks and transport until the real decks arrive in M7. -->
  {#if space !== 'Mix'}
    <div class="dock">
      <TransitionDock
        onCollapse={(collapsed) => (dockChoice = collapsed ? 'collapsed' : 'expanded')}
      />
    </div>

    <div class="player">
      <PlayerBar
        trackId={preview.trackId}
        title={preview.title}
        artist={preview.artist}
        playing={preview.playing}
        positionMs={preview.positionMs}
        durationMs={preview.durationMs}
        volume={preview.volume}
        muted={preview.muted}
        canPlay={preview.trackId !== null || track != null}
        error={preview.error}
        ontoggleplay={() => {
          if (preview.trackId !== null) void preview.togglePlay();
          else if (track) void preview.loadAndPlay(track.id);
        }}
        onseek={(ms) => void preview.seek(ms)}
        onvolume={(vol) => void preview.setVolume(vol)}
        ontogglemute={() => void preview.toggleMute()}
      />
    </div>
  {/if}
</div>

{#if sheetOpen && space === 'Library'}
  <Sheet bind:open={sheetOpen} variant="drawer" title="Inspector">
    <Inspector
      {track}
      inSheet
      previewTrackId={preview.trackId}
      previewPlaying={preview.playing}
      {collections}
      {activeCollection}
      onclose={() => (sheetOpen = false)}
      onpreview={(item) => void preview.toggleOrPlayTrack(item.id)}
      ontogglefavourite={toggleFavourite}
      onaddtocollection={addTracksToCollection}
      onremovefromcollection={removeTracksFromCollection}
      oncreatecollectionandadd={createCollectionAndAdd}
    />
  </Sheet>
{/if}

{#if settingsMounted}
  <SettingsSheet
    bind:open={settingsOpen}
    {sourcesClient}
    {sourcesRevision}
    onsourceschanged={() => {
      void pages.refresh();
      void loadCollections();
    }}
  />
{/if}

{#if scanMounted}
  <ScanFolder
    bind:open={scanOpen}
    scan={(folder) => core.call('scan', { folder })}
    onscanned={onScanned}
  />
{/if}

<style>
  /*
   * The window size decides the layout here. Each state is a custom property that the sidebar
   * and dock read through style queries, and that App reads when a shortcut flips it. Media
   * queries cannot read tokens, so the breakpoints are written out.
   */
  .app-shell {
    --sidebar-state: expanded;
    --sidebar-column: var(--sidebar-width);
    --dock-state: expanded;
    --inspector-mode: docked;
    --inspector-column: clamp(var(--inspector-min-width), 26.5vw, var(--inspector-max-width));

    display: grid;
    grid-template:
      'topbar topbar topbar' var(--topbar-height)
      'sidebar workspace inspector' minmax(var(--workspace-min-height), 1fr)
      'dock dock dock' auto
      'player player player' auto
      / var(--sidebar-column) minmax(var(--workspace-min-width), 1fr) var(--inspector-column);
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    background: var(--color-background);
    color: var(--color-text);
  }

  @media (width < 1360px) {
    .app-shell {
      --sidebar-state: collapsed;
      --sidebar-column: var(--sidebar-collapsed-width);
    }
  }

  @media (width < 1280px) {
    .app-shell {
      --inspector-mode: sheet;
      --inspector-column: 0px;
    }

    .inspector-dock {
      display: none;
    }
  }

  @media (height < 820px) {
    .app-shell {
      --dock-state: collapsed;
    }
  }

  .app-shell[data-sidebar='collapsed'] {
    --sidebar-state: collapsed;
    --sidebar-column: var(--sidebar-collapsed-width);
  }

  .app-shell[data-sidebar='expanded'] {
    --sidebar-state: expanded;
    --sidebar-column: var(--sidebar-width);
  }

  .app-shell[data-dock='collapsed'] {
    --dock-state: collapsed;
  }

  .app-shell[data-dock='expanded'] {
    --dock-state: expanded;
  }

  .app-shell[data-inspector='hidden'] {
    --inspector-column: 0px;
  }

  .topbar {
    grid-area: topbar;
  }

  .sidebar {
    grid-area: sidebar;
    min-height: 0;
    overflow: hidden;
  }

  .workspace {
    display: grid;
    grid-area: workspace;
    grid-template-rows: auto minmax(0, 1fr);
    grid-template-columns: minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .workspace-content {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--color-background);
  }

  .inspector-dock {
    grid-area: inspector;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .dock {
    grid-area: dock;
  }

  .player {
    grid-area: player;
  }
</style>
