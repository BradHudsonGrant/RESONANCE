<script lang="ts">
  import { flushSync, tick } from 'svelte';
  import { invoke, isTauri } from '@tauri-apps/api/core';

  import Sheet from './lib/design/Sheet.svelte';
  import type { ActivitySnapshot } from './lib/ipc/bindings/ActivitySnapshot';
  import type { LibraryCollection } from './lib/ipc/bindings/LibraryCollection';
  import type { LibraryTrack } from './lib/ipc/bindings/LibraryTrack';
  import type { RenameScope } from './lib/ipc/bindings/RenameScope';
  import type { SmartView } from './lib/ipc/bindings/SmartView';
  import type { TrackArtwork } from './lib/ipc/bindings/TrackArtwork';
  import type { TrackQuery } from './lib/ipc/bindings/TrackQuery';
  import { pictureBase64 } from './lib/ipc/artwork';
  import BatchTagSheet from './lib/prep/BatchTagSheet.svelte';
  import type { PrepClient } from './lib/prep/Prep.svelte';
  import { CoreError } from './lib/ipc/client';
  import { core } from './lib/ipc/core';
  import { FilterState } from './lib/library/filters.svelte';
  import Inspector from './lib/library/Inspector.svelte';
  import Library from './lib/library/Library.svelte';
  import RelinkSheet, { type RelinkClient } from './lib/library/RelinkSheet.svelte';
  import RenameSheet, { type RenameClient } from './lib/library/RenameSheet.svelte';
  import ScanFolder from './lib/library/ScanFolder.svelte';
  import type { SourcesClient } from './lib/library/SourcesSettings.svelte';
  import { TableLayoutState } from './lib/library/table-layout.svelte';
  import { TrackPages } from './lib/library/track-pages.svelte';
  import ActivitySheet, { type ActivityClient } from './lib/shell/ActivitySheet.svelte';
  import CommandBar, { type Command } from './lib/shell/CommandBar.svelte';
  import PlayerBar from './lib/shell/PlayerBar.svelte';
  import { PreviewPlayer } from './lib/shell/preview.svelte';
  import SettingsSheet from './lib/shell/SettingsSheet.svelte';
  import type { ProviderKeysClient } from './lib/shell/ProviderKeysSettings.svelte';
  import Sidebar, { type SpaceName } from './lib/shell/Sidebar.svelte';
  import TopBar from './lib/shell/TopBar.svelte';
  import TransitionDock from './lib/shell/TransitionDock.svelte';

  type Fold = 'collapsed' | 'expanded';

  const commandInput = 'command-bar';
  const pages = new TrackPages((offset, limit, query) =>
    core.call('tracks', { offset, limit, query }),
  );
  const tableLayout = new TableLayoutState(
    () => core.call('table_layout'),
    (change) => core.call('change_table_layout', { change }),
  );
  const filters = new FilterState({
    fields: () => core.call('filter_fields'),
    summaries: (query) => core.call('filter_summaries', { query }),
    parse: (text) => core.call('parse_filter', { text }),
    recordUse: (term) => core.call('record_filter_use', { term }),
    quickFilters: () => core.call('quick_filters'),
    changeQuickFilters: (change) => core.call('change_quick_filters', { change }),
    smartViews: () => core.call('smart_views'),
    saveView: (name, query) => core.call('save_smart_view', { name, query }),
    renameView: (view_id, name) => core.call('rename_smart_view', { view_id, name }),
    deleteView: (view_id) => core.call('delete_smart_view', { view_id }),
  });
  const preview = new PreviewPlayer(core);
  const sourcesClient: SourcesClient = {
    sources: () => core.call('sources'),
    subfolders: (source_id, parent) => core.call('source_subfolders', { source_id, parent }),
    scan: (folder) => core.call('scan', { folder }),
    updateSource: (params) => core.call('update_source', params),
    removeSource: (source_id) => core.call('remove_source', { source_id }),
    setIgnoredFolders: (folders) => core.call('set_ignored_folders', { folders }),
  };
  const providerKeysClient: ProviderKeysClient = {
    status: () => core.call('provider_keys'),
    save: (provider, value) => core.call('set_provider_key', { provider, value }),
    clear: (provider) => core.call('clear_provider_key', { provider }),
    identificationSettings: () => core.call('identification_settings'),
    saveIdentificationSettings: (settings) =>
      core.call('set_identification_settings', { settings }),
  };
  const desktop = isTauri();
  const relinkClient: RelinkClient = {
    candidates: (track_id, search_folder) =>
      core.call('relink_candidates', { track_id, search_folder: search_folder ?? null }),
    preview: (track_id, candidate_path) =>
      core.call('preview_relink', { track_id, candidate_path }),
    apply: (track_id, candidate_path, force) =>
      core.call('apply_relink', { track_id, candidate_path, force }),
    pickAudioFile: desktop ? () => invoke<string | null>('pick_audio_file') : undefined,
    pickFolder: desktop ? () => invoke<string | null>('pick_folder') : undefined,
  };
  const renameClient: RenameClient = {
    preview: (scope, template, limit) => core.call('preview_rename', { scope, template, limit }),
    apply: (scope, template) => core.call('apply_rename', { scope, template }),
    recentOperations: () => core.call('recent_operations', { limit: 20 }),
    undo: (operation_id) => core.call('undo_operation', { operation_id }),
  };
  const activityClient: ActivityClient = {
    snapshot: () => core.call('jobs_snapshot'),
    cancel: (job_id) => core.call('cancel_job', { job_id }),
    pause: (job_id) => core.call('pause_job', { job_id }),
    resume: (job_id) => core.call('resume_job', { job_id }),
    retry: (job_id) => core.call('retry_job', { job_id }),
    clearFinished: () => core.call('clear_finished_jobs'),
  };
  const prepClient: PrepClient = {
    trackIds: (query, track_ids) =>
      core.call('prep_track_ids', { query, ...(track_ids === null ? {} : { track_ids }) }),
    prepPage: (query, track_ids, offset, limit) =>
      core.call('prep_page', {
        query,
        ...(track_ids === null ? {} : { track_ids }),
        offset,
        limit,
      }),
    acceptAndWrite: (track_ids, fields, include_artwork, expected_review) =>
      core.call('accept_and_write', {
        track_ids,
        fields,
        include_artwork,
        ...(expected_review == null ? {} : { expected_review }),
      }),
    selectCandidate: (track_id, candidate_index, expected_review) =>
      core.call('select_prep_candidate', { track_id, candidate_index, expected_review }),
    useCandidateArtwork: (track_id, candidate_index, accept, expected_review) =>
      core.call('use_prep_candidate_artwork', {
        track_id,
        candidate_index,
        accept,
        expected_review,
      }),
    identifyOnline: (track_ids, only_selected) =>
      core.call('identify_online', { track_ids, only_selected }),
    identifyWithUrl: (track_id, url) => core.call('identify_with_url', { track_id, url }),
    previewBatchTag: (track_ids, tool) => core.call('preview_batch_tag', { track_ids, tool }),
    applyBatchTag: (track_ids, tool) => core.call('apply_batch_tag', { track_ids, tool }),
    previewTagWrites: (track_ids) => core.call('preview_tag_writes', { track_ids }),
    applyTagWrites: (track_ids) => core.call('apply_tag_writes', { track_ids }),
    undoOperation: (operation_id) => core.call('undo_operation', { operation_id }),
  };

  let shell: HTMLDivElement;
  let space = $state<SpaceName>('Library');
  let prepPaletteOpen = $state(false);
  $effect(() => {
    if (space !== 'Prep') prepPaletteOpen = false;
  });
  // The window size sets the layout in CSS. These hold only what the user chose over it.
  let sidebarChoice = $state<Fold>();
  let dockChoice = $state<Fold>();
  let inspectorHidden = $state(false);
  let sheetOpen = $state(false);
  let activityOpen = $state(false);
  let activityMounted = $state(false);
  let activitySnapshot = $state<ActivitySnapshot>({
    active: [],
    recent: [],
    mix_backoff_active: false,
  });
  let settingsOpen = $state(false);
  let settingsMounted = $state(false);
  let scanOpen = $state(false);
  let scanMounted = $state(false);
  let relinkOpen = $state(false);
  let relinkMounted = $state(false);
  let renameOpen = $state(false);
  let renameMounted = $state(false);
  let renameScope = $state<RenameScope>({ kind: 'tracks', track_ids: [] });
  let renameLabel = $state('');
  let batchTagOpen = $state(false);
  let batchTagMounted = $state(false);
  let batchTagTrackIds = $state<number[]>([]);
  let prepBatchQuery = $state<TrackQuery | null>(null);
  let prepBatchLabel = $state<string | null>(null);
  let prepTrackIds = $state<number[] | null>(null);
  let prepCount = $state<number | null>(null);
  let settingsFocusSection = $state<'providers' | null>(null);
  let relinkTarget = $state<LibraryTrack | null>(null);
  let sourcesRevision = $state(0);
  let mixPlaying = $state(false);
  let searchText = $state('');
  let collections = $state<LibraryCollection[]>([]);
  let activeCollectionId = $state<number | null>(null);
  let activeViewId = $state<number | null>(null);

  const activeCollection = $derived(
    collections.find((collection) => collection.id === activeCollectionId) ?? null,
  );
  const activeOnlineJob = $derived(
    activitySnapshot.active.find((job) => job.kind === 'identify_online') ?? null,
  );
  const recentOnlineJob = $derived(
    activitySnapshot.recent.find((job) => job.kind === 'identify_online') ?? null,
  );
  const identifyingOnline = $derived(activeOnlineJob != null);

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

  function openView(view: SmartView) {
    activeCollectionId = null;
    activeViewId = view.id;
    searchText = view.query.text ?? '';
    void pages.setQuery({
      ...pages.query,
      collection_id: null,
      text: view.query.text ?? null,
      pill: view.query.pill ?? null,
      filters: view.query.filters ?? null,
    });
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
    activeViewId = null;
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
      if (track && trackIds.includes(track.id)) {
        pages.selected = null;
      }
      await pages.refresh();
    }
  }

  async function createCollectionAndAdd(name: string, trackIds: number[]) {
    const created = await createCollection(name);
    await addTracksToCollection(created.id, trackIds);
  }

  let activityRefreshInFlight = false;
  let activityRefreshQueued = false;

  async function refreshActivity() {
    if (activityRefreshInFlight) {
      activityRefreshQueued = true;
      return;
    }
    activityRefreshInFlight = true;
    try {
      do {
        activityRefreshQueued = false;
        const next = await core.call('jobs_snapshot');
        if (next) {
          activitySnapshot = next;
        }
      } while (activityRefreshQueued);
    } catch {
      // Initial library load handles core error states; ignore transient snapshot failure.
    } finally {
      activityRefreshInFlight = false;
    }
  }

  function openActivity() {
    activityMounted = true;
    activityOpen = true;
  }

  function openSettings() {
    settingsFocusSection = null;
    settingsMounted = true;
    settingsOpen = true;
  }

  let prepCountInFlight = false;
  let prepCountQueued = false;

  async function refreshPrepCount() {
    if (prepCountInFlight) {
      prepCountQueued = true;
      return;
    }
    prepCountInFlight = true;
    try {
      do {
        prepCountQueued = false;
        const page = await core.call('staging_queue', { offset: 0, limit: 0 });
        prepCount = page.total;
      } while (prepCountQueued);
    } catch {
      // Initial load handles core error states; ignore transient count refresh failure.
    } finally {
      prepCountInFlight = false;
    }
  }

  function openScan() {
    scanMounted = true;
    scanOpen = true;
  }

  function openRelink(item: LibraryTrack) {
    relinkTarget = item;
    relinkMounted = true;
    relinkOpen = true;
  }

  function openRename(scope: RenameScope, label: string) {
    renameScope = scope;
    renameLabel = label;
    renameMounted = true;
    renameOpen = true;
  }

  function openBatchTag(trackIds: number[]) {
    if (trackIds.length === 0) return;
    batchTagTrackIds = trackIds;
    batchTagMounted = true;
    batchTagOpen = true;
  }

  function sendToPrep(query: TrackQuery, label: string) {
    prepTrackIds = null;
    prepBatchQuery = { ...query };
    prepBatchLabel = label;
    space = 'Prep';
  }

  function openTracksInPrep(trackIds: number[]) {
    if (trackIds.length === 0) return;
    prepBatchQuery = null;
    prepBatchLabel = null;
    prepTrackIds = [...trackIds];
    space = 'Prep';
  }

  async function identifyTracksOnline(trackIds: number[]) {
    if (trackIds.length === 0) return;
    try {
      const next = await core.call('identify_online', {
        track_ids: trackIds,
        only_selected: true,
      });
      if (next) activitySnapshot = next;
      openTracksInPrep(trackIds);
    } catch {
      openActivity();
    }
  }

  function previewTrack(item: LibraryTrack) {
    if (item.missing_since != null) return;
    void preview.toggleOrPlayTrack(item.id);
  }

  function playTrack(item: LibraryTrack) {
    if (item.missing_since != null) return;
    void preview.playTrack(item.id);
  }

  function toggleAutoPreview() {
    if (space === 'Library' || space === 'Prep') void preview.toggleAutoPreview(track?.id ?? null);
  }

  const track = $derived(pages.selectedTrack);

  let trackArtwork = $state<TrackArtwork | null>(null);
  let artworkError = $state<string | null>(null);

  async function loadArtwork(trackId: number) {
    try {
      const loaded = await core.call('track_artwork', { track_id: trackId });
      if (track?.id === trackId) trackArtwork = loaded;
    } catch {
      if (track?.id === trackId) trackArtwork = null;
    }
  }

  async function changeArtwork(trackId: number, change: () => Promise<TrackArtwork>) {
    artworkError = null;
    try {
      const changed = await change();
      if (track?.id === trackId) trackArtwork = changed;
    } catch (error) {
      if (track?.id === trackId) {
        artworkError =
          error instanceof CoreError ? error.message : 'The cover could not be changed.';
      }
    }
  }

  function pickArtwork(trackId: number, file: File) {
    void changeArtwork(trackId, async () =>
      core.call('set_track_artwork', {
        track_id: trackId,
        image_base64: await pictureBase64(file),
      }),
    );
  }

  function acceptArtwork(trackId: number) {
    void changeArtwork(trackId, () => core.call('accept_proposed_artwork', { track_id: trackId }));
  }

  function skipArtwork(trackId: number) {
    void changeArtwork(trackId, () => core.call('skip_proposed_artwork', { track_id: trackId }));
  }

  $effect(() => {
    const trackId = track?.id;
    artworkError = null;
    trackArtwork = null;
    if (trackId !== undefined) void loadArtwork(trackId);
  });
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

  // Each shell region mounts and lays out in its own short task so no single startup task or
  // frame pays to build and shape the whole shell at once.
  let shellStage = $state(0);
  $effect(() => {
    let timer = 0;
    const advance = () => {
      const next = shellStage + 1;
      flushSync(() => {
        shellStage = next;
      });
      void shell?.offsetHeight;
      if (next === 4) {
        void loadCollections();
        void refreshActivity();
        void refreshPrepCount();
      } else if (next === 5) {
        void preview.refresh();
        void preview.loadSettings();
      }
      if (next < 7) {
        timer = window.setTimeout(advance, 0);
      }
    };
    timer = window.setTimeout(advance, 0);
    return () => clearTimeout(timer);
  });

  let previousSpace: SpaceName = 'Library';
  let libraryDirty = false;
  $effect(() => {
    const next = space;
    if (next === 'Mix' && previousSpace !== 'Mix' && preview.playing) {
      void preview.pause();
    }
    if (next === 'Library' && previousSpace !== 'Library' && libraryDirty) {
      libraryDirty = false;
      void pages.refresh();
      if (track) void loadArtwork(track.id);
    }
    previousSpace = next;
  });

  $effect(() =>
    core.onLibraryChange(() => {
      sourcesRevision += 1;
      void loadCollections();
      void refreshPrepCount();
      if (space === 'Library') {
        void pages.refresh();
        if (track) void loadArtwork(track.id);
      } else {
        libraryDirty = true;
      }
    }),
  );

  $effect(() =>
    core.onActivityChange(() => {
      void refreshActivity();
    }),
  );

  const commands: Command[] = $derived([
    ...(space !== 'Library'
      ? [{ name: 'Go to the library', shortcut: '1', run: () => (space = 'Library') }]
      : []),
    ...(space !== 'Prep'
      ? [{ name: 'Go to Prep', shortcut: '2', run: () => (space = 'Prep') }]
      : []),
    ...(space !== 'Mix' ? [{ name: 'Go to Mix', shortcut: '4', run: () => (space = 'Mix') }] : []),
    { name: 'Scan a music folder', run: openScan },
    { name: 'Manage library sources', run: openSettings },
    { name: 'Open activity', run: openActivity },
    { name: 'Show or hide the inspector', shortcut: 'I', run: toggleInspector },
    {
      name: preview.settings.auto_preview ? 'Turn auto-preview off' : 'Turn auto-preview on',
      shortcut: 'P',
      run: toggleAutoPreview,
    },
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
    void refreshPrepCount();
  }

  function onScanned() {
    sourcesRevision += 1;
    void pages.refresh();
    void loadCollections();
    void refreshPrepCount();
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
    '/': () => {
      if (space === 'Prep') prepPaletteOpen = true;
      void tick().then(() => document.getElementById(commandInput)?.focus());
    },
    i: toggleInspector,
    p: toggleAutoPreview,
    '[': toggleSidebar,
    d: toggleDock,
    '1': () => (space = 'Library'),
    '2': () => (space = 'Prep'),
    '4': () => (space = 'Mix'),
  };

  function onkeydown(event: KeyboardEvent) {
    // An open sheet or menu owns the keyboard, so nothing changes behind it.
    if (event.altKey || document.querySelector('dialog[open], [role="menu"]')) return;
    const key = event.key.toLowerCase();
    const command = event.ctrlKey || event.metaKey;

    if (command && key === 'k') {
      event.preventDefault();
      if (space === 'Prep') prepPaletteOpen = true;
      void tick().then(() => document.getElementById(commandInput)?.focus());
    } else if (command && (key === '1' || key === '2' || key === '4')) {
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
      } else if (track && track.missing_since == null) {
        event.preventDefault();
        void preview.loadAndPlay(track.id);
      }
    } else if (!command && !typing(event.target) && keys[key]) {
      event.preventDefault();
      keys[key]();
    }
  }

  pages.load();
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
    {#if shellStage >= 1}
      <TopBar
        onsettings={openSettings}
        onactivity={openActivity}
        activeJobCount={activitySnapshot.active.length}
        {activityOpen}
      />
    {/if}
  </div>

  <div class="sidebar">
    {#if shellStage >= 7}
      <Sidebar
        activeSpace={space}
        trackCount={pages.state.kind === 'ready' ? pages.libraryTotal : null}
        {prepCount}
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
        smartViews={filters.views}
        {activeViewId}
        onSelectView={openView}
        onRenameView={(viewId, name) => filters.renameView(viewId, name)}
        onDeleteView={async (viewId) => {
          await filters.deleteView(viewId);
          if (activeViewId === viewId) activeViewId = null;
        }}
        onCollapse={(collapsed) => (sidebarChoice = collapsed ? 'collapsed' : 'expanded')}
      />
    {/if}
  </div>

  <div class="workspace" class:tagging={space === 'Prep' && !prepPaletteOpen}>
    {#if shellStage >= 2 && (space !== 'Prep' || prepPaletteOpen)}
      <CommandBar
        {commands}
        inputId={commandInput}
        search={space === 'Prep' ? '' : searchText}
        showExamples={space !== 'Prep'}
        onblur={() => (prepPaletteOpen = false)}
        parseFilter={(text) => filters.parse(text)}
        onsearch={(text) => {
          if (space === 'Prep' && text.trim()) space = 'Library';
          setSearch(text);
        }}
        onfilter={(filter) => {
          space = 'Library';
          filters.remember(filter);
          filters.add(pages, filter.term);
        }}
      />
    {:else if shellStage < 2}
      <div></div>
    {/if}
    <main class="workspace-content">
      {#if shellStage >= 3}
        {#if space === 'Mix'}
          {#await import('./lib/mix/Mix.svelte') then { default: Mix }}
            <Mix onplayingchange={(playing) => (mixPlaying = playing)} />
          {/await}
        {:else if space === 'Prep'}
          {#await import('./lib/prep/Prep.svelte') then { default: Prep }}
            <Prep
              client={prepClient}
              batchQuery={prepBatchQuery}
              batchLabel={prepBatchLabel}
              batchTrackIds={prepTrackIds}
              revision={sourcesRevision}
              {activeOnlineJob}
              {recentOnlineJob}
              onopenactivity={openActivity}
              onopenproviders={() => {
                settingsFocusSection = 'providers';
                settingsMounted = true;
                settingsOpen = true;
              }}
              onclearbatch={() => {
                prepBatchQuery = null;
                prepBatchLabel = null;
                prepTrackIds = null;
              }}
              onopenlibrary={() => (space = 'Library')}
              onrename={openRename}
              onpreview={previewTrack}
              onfocustrack={(item) => void preview.focus(item.id)}
            />
          {/await}
        {:else}
          <Library
            {pages}
            {filters}
            bind:selected={pages.selected}
            previewTrackId={preview.trackId}
            previewPlaying={preview.playing}
            {collections}
            {activeCollection}
            loadFacets={() => core.call('library_facets')}
            {tableLayout}
            onsendtoprep={sendToPrep}
            onopeninprep={openTracksInPrep}
            onidentifyonline={(ids) => void identifyTracksOnline(ids)}
            onbatchtag={openBatchTag}
            onopen={showInspector}
            onpreview={previewTrack}
            onplay={playTrack}
            onfocustrack={(item) => void preview.focus(item.id)}
            onrelink={openRelink}
            onrename={openRename}
            onscan={openScan}
            ontogglefavourite={toggleFavourite}
            onclearsearch={clearSearch}
            onclearcollection={() => selectCollection(null)}
            onaddtocollection={addTracksToCollection}
            onremovefromcollection={removeTracksFromCollection}
            oncreatecollectionandadd={createCollectionAndAdd}
          />
        {/if}
      {/if}
    </main>
  </div>

  {#if inspectorDocked && shellStage >= 4}
    <div class="inspector-dock">
      <Inspector
        {track}
        previewTrackId={preview.trackId}
        previewPlaying={preview.playing}
        {collections}
        {activeCollection}
        onclose={() => (inspectorHidden = true)}
        onpreview={previewTrack}
        onrelink={openRelink}
        identifying={identifyingOnline}
        onopeninprep={(item) => openTracksInPrep([item.id])}
        onidentifyonline={(item) => void identifyTracksOnline([item.id])}
        ontogglefavourite={toggleFavourite}
        onaddtocollection={addTracksToCollection}
        onremovefromcollection={removeTracksFromCollection}
        oncreatecollectionandadd={createCollectionAndAdd}
        artwork={trackArtwork}
        {artworkError}
        onpickartwork={pickArtwork}
        onacceptartwork={acceptArtwork}
        onskipartwork={skipArtwork}
      />
    </div>
  {/if}

  <!-- The Mix spike has its own decks and transport until the real decks arrive in M7. -->
  {#if space !== 'Mix' && space !== 'Prep' && shellStage >= 5}
    <div class="dock">
      <TransitionDock
        onCollapse={(collapsed) => (dockChoice = collapsed ? 'collapsed' : 'expanded')}
      />
    </div>
  {/if}

  {#if space !== 'Mix' && shellStage >= 6}
    <div class="player">
      <PlayerBar
        trackId={preview.trackId}
        title={preview.title}
        artist={preview.artist}
        artwork={preview.artwork}
        playing={preview.playing}
        positionMs={preview.positionMs}
        durationMs={preview.durationMs}
        volume={preview.volume}
        muted={preview.muted}
        canPlay={preview.trackId !== null || (track != null && track.missing_since == null)}
        autoPreview={preview.settings.auto_preview}
        ontoggleautopreview={toggleAutoPreview}
        error={preview.error}
        ontoggleplay={() => {
          if (preview.trackId !== null) void preview.togglePlay();
          else if (track && track.missing_since == null) void preview.loadAndPlay(track.id);
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
      onpreview={previewTrack}
      onrelink={openRelink}
      identifying={identifyingOnline}
      onopeninprep={(item) => {
        sheetOpen = false;
        openTracksInPrep([item.id]);
      }}
      onidentifyonline={(item) => {
        sheetOpen = false;
        void identifyTracksOnline([item.id]);
      }}
      ontogglefavourite={toggleFavourite}
      onaddtocollection={addTracksToCollection}
      onremovefromcollection={removeTracksFromCollection}
      oncreatecollectionandadd={createCollectionAndAdd}
      artwork={trackArtwork}
      {artworkError}
      onpickartwork={pickArtwork}
      onacceptartwork={acceptArtwork}
      onskipartwork={skipArtwork}
    />
  </Sheet>
{/if}

{#if relinkMounted}
  <RelinkSheet
    bind:open={relinkOpen}
    track={relinkTarget ? (pages.trackById(relinkTarget.id) ?? relinkTarget) : null}
    client={relinkClient}
    onrelinked={(updated) => {
      pages.replaceLocalTrack(updated);
      relinkTarget = updated;
      void pages.refresh();
      void loadCollections();
    }}
  />
{/if}

{#if renameMounted}
  <RenameSheet
    bind:open={renameOpen}
    scope={renameScope}
    scopeLabel={renameLabel}
    client={renameClient}
    onrenamed={() => void pages.refresh()}
  />
{/if}

{#if batchTagMounted}
  <BatchTagSheet
    bind:open={batchTagOpen}
    trackIds={batchTagTrackIds}
    sourceTrackId={track?.id ?? batchTagTrackIds[0] ?? null}
    sourceTrackTitle={track?.title ?? null}
    scopeLabel="Selected tracks"
    client={{
      preview: (ids, tool) => prepClient.previewBatchTag(ids, tool),
      apply: (ids, tool) => prepClient.applyBatchTag(ids, tool),
    }}
    onapplied={() => void pages.refresh()}
  />
{/if}

{#if activityMounted}
  <ActivitySheet
    bind:open={activityOpen}
    snapshot={activitySnapshot}
    client={activityClient}
    onsnapshot={(next) => {
      activitySnapshot = next;
    }}
  />
{/if}

{#if settingsMounted}
  <SettingsSheet
    bind:open={settingsOpen}
    {sourcesClient}
    {providerKeysClient}
    {renameClient}
    {sourcesRevision}
    focusSection={settingsFocusSection}
    previewStart={preview.settings.start}
    maxStartSeconds={preview.settings.max_start_seconds}
    onpreviewstart={(start) => void preview.changeSettings({ start })}
    recommendFilters={filters.quick.recommend}
    onrecommendfilters={(on) => void filters.change({ kind: 'set_recommend', on })}
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

  .workspace.tagging {
    grid-template-rows: minmax(0, 1fr);
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
