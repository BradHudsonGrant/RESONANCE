<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import type { AcceptedTagWriteReport } from '../ipc/bindings/AcceptedTagWriteReport';
  import type { AcceptedProposals } from '../ipc/bindings/AcceptedProposals';
  import type { ActivityJob } from '../ipc/bindings/ActivityJob';
  import type { ActivitySnapshot } from '../ipc/bindings/ActivitySnapshot';
  import type { BatchTagTool } from '../ipc/bindings/BatchTagTool';
  import type { FileTagWrite } from '../ipc/bindings/FileTagWrite';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { PrepItem } from '../ipc/bindings/PrepItem';
  import type { PrepOrderSnapshot } from '../ipc/bindings/PrepOrderSnapshot';
  import type { PrepStageCounts } from '../ipc/bindings/PrepStageCounts';
  import type { PrepStageName } from '../ipc/bindings/PrepStageName';
  import type { RenameScope } from '../ipc/bindings/RenameScope';
  import type { TagName } from '../ipc/bindings/TagName';
  import type { TagWriteReport } from '../ipc/bindings/TagWriteReport';
  import type { TrackCopies } from '../ipc/bindings/TrackCopies';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import type { TrackCandidateArtwork } from '../ipc/bindings/TrackCandidateArtwork';
  import type { TrackIdentification } from '../ipc/bindings/TrackIdentification';
  import type { UndoReport } from '../ipc/bindings/UndoReport';
  import BatchTagSheet from './BatchTagSheet.svelte';
  import PrepReview from './PrepReview.svelte';
  import PrepTrackList from './PrepTrackList.svelte';
  import WriteTagsSheet from './WriteTagsSheet.svelte';
  import { keepPlaces, nextActive, rowsToFetch } from './prep-order';
  import {
    MISSING_FIELDS,
    MISSING_LABELS,
    buildMissingQuery,
    nextListSort,
    withFindText,
    withListSort,
    createSelectionSet,
    formatSaveNotice,
    formatUrlLookupNotice,
    formatWriteNotice,
    intersectSelection,
    pruneSelection,
    reviewableFields,
    stageSummary,
    type ListColumn,
    type ListSort,
    type MissingField,
  } from './prep-view';

  export interface PrepClient {
    trackIds: (
      query: TrackQuery | null,
      trackIds: number[] | null,
      stage: PrepStageName | null,
    ) => Promise<number[]>;
    /** The view's order and stage counts; `byStage` groups it by stage instead. */
    prepOrder: (
      query: TrackQuery | null,
      trackIds: number[] | null,
      byStage: 'asc' | 'desc' | null,
    ) => Promise<PrepOrderSnapshot>;
    /** The rows for these tracks, in the order given. */
    prepItems: (trackIds: number[]) => Promise<PrepItem[]>;
    acceptAndWrite: (
      trackIds: number[],
      fields: TagName[] | null,
      includeArtwork: boolean,
      expectedReview?: string,
    ) => Promise<AcceptedTagWriteReport>;
    skipItems: (trackIds: number[], fields: TagName[]) => Promise<AcceptedProposals>;
    selectCandidate: (trackId: number, index: number, expectedReview: string) => Promise<PrepItem>;
    useCandidateArtwork: (
      trackId: number,
      index: number,
      accept: boolean,
      expectedReview: string,
    ) => Promise<PrepItem>;
    /** Fetches a match's cover for viewing, without suggesting it. */
    candidateArtwork: (trackId: number, index: number) => Promise<TrackCandidateArtwork>;
    skipArtwork: (trackId: number) => Promise<PrepItem>;
    identifyOnline: (trackIds: number[], onlySelected: boolean) => Promise<ActivitySnapshot>;
    identifyWithUrl: (trackId: number, url: string) => Promise<TrackIdentification>;
    previewBatchTag: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    applyBatchTag: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    previewTagWrites: (trackIds: number[]) => Promise<FileTagWrite[]>;
    applyTagWrites: (trackIds: number[]) => Promise<TagWriteReport>;
    undoOperation: (operationId: number) => Promise<UndoReport>;
    /** The track's own files, its copies and its versions. */
    trackCopies: (trackId: number) => Promise<TrackCopies>;
    /** Keeps one copy and moves the others to the Recycle Bin. */
    keepCopy: (trackId: number, assetId: number) => Promise<TrackCopies>;
  }

  interface Props {
    client: PrepClient;
    batchQuery?: TrackQuery | null;
    batchTrackIds?: number[] | null;
    batchLabel?: string | null;
    revision?: number;
    activeOnlineJob?: ActivityJob | null;
    recentOnlineJob?: ActivityJob | null;
    onopenactivity?: () => void;
    onopenproviders?: () => void;
    onclearbatch?: () => void;
    onopenlibrary?: () => void;
    onrename?: (scope: RenameScope, label: string) => void;
    onpreview?: (track: LibraryTrack) => void;
    onfocustrack?: (track: LibraryTrack) => void;
  }
  let {
    client,
    batchQuery = null,
    batchTrackIds = null,
    batchLabel = null,
    revision = 0,
    activeOnlineJob = null,
    recentOnlineJob = null,
    onopenactivity,
    onopenproviders,
    onclearbatch,
    onopenlibrary,
    onrename,
    onpreview,
    onfocustrack,
  }: Props = $props();

  const noCounts: PrepStageCounts = { review: 0, not_searched: 0, no_match: 0, done: 0 };
  let missing = $state<MissingField | ''>('');
  let counts = $state<PrepStageCounts>(noCounts);
  // The order of the whole view, fixed when the view loads or the user sorts. Saving or
  // searching refreshes rows in place, so a track never jumps while it is being worked on.
  let order = $state.raw<number[]>([]);
  const totalCount = $derived(order.length);
  // Tracks fetched so far, by ID. Only the rows in view are drawn, and the list is as tall as the
  // whole view from the start, so the scrollbar never jumps as more rows arrive.
  let byId = $state.raw<Readonly<Record<number, PrepItem>>>({});
  // Plain bookkeeping for rows already requested, and for rows the core no longer has, which are
  // dropped from the list rather than asked for again. Nothing renders from either.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const fetching = new Set<number>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const gone = new Set<number>();
  let activeId = $state<number | null>(null);
  const selected = createSelectionSet();
  let selectionVersion = $state(0);
  let loading = $state(true);
  let loadError = $state('');
  let notice = $state('');
  let failures = $state<TagWriteReport['failed']>([]);
  let busy = $state(false);
  let linkBusy = $state(false);
  let batchOpen = $state(false);
  let writeOpen = $state(false);
  let toolIds = $state.raw<number[]>([]);
  let refresh = $state(0);
  let chosenCoverByTrack = $state.raw<Record<number, number>>({});
  // Tracks sent to the running search, so their rows can say so, and what the last one found.
  let searchingIds = $state.raw<ReadonlySet<number>>(new Set());
  let searchSummary = $state<{ searched: number; review: number; noMatch: number } | null>(null);
  interface SavedChange {
    operation: number;
    track: number | null;
  }
  let history = $state.raw<SavedChange[]>([]);
  const lastAction = $derived(history.at(-1) ?? null);
  let workspace: HTMLElement;
  let list = $state<ReturnType<typeof PrepTrackList>>();
  let review = $state<ReturnType<typeof PrepReview>>();
  let width = $state(0);
  let reviewing = $state(false);
  const compact = $derived(width > 0 && width <= 800);
  let keyboardTarget: number | null = null;
  let reloading = $state(false);
  let fetchGeneration = 0;
  let suppressRevisions = 0;
  let lastOwnWriteAt = 0;
  let prevRevision = untrack(() => revision);
  let prevRefresh = 0;

  let findText = $state('');
  let appliedFind = $state('');
  let findTimer: ReturnType<typeof setTimeout> | undefined;
  let listSort = $state<ListSort | null>(null);
  // Bumped to sort again by the same column, such as after a search finds new matches.
  let sortGeneration = $state(0);
  const filterQuery = $derived(withFindText(buildMissingQuery(batchQuery, missing), appliedFind));
  const scopeQuery = $derived(withListSort(filterQuery, listSort));
  const current = $derived(activeId == null ? null : (byId[activeId] ?? null));
  const detailOpen = $derived(compact && reviewing && current !== null);

  let copies = $state<TrackCopies | null>(null);
  let keepingCopy = $state(false);

  const currentId = $derived(current?.track.id);

  // Read once per track, not on every library change, so a large save is not slowed by it.
  $effect(() => {
    const trackId = currentId;
    if (trackId === undefined) {
      copies = null;
      return;
    }
    client.trackCopies(trackId).then(
      (loaded) => {
        if (untrack(() => currentId) === trackId) copies = loaded;
      },
      () => {
        if (untrack(() => currentId) === trackId) copies = null;
      },
    );
  });

  async function keepCopy(trackId: number, assetId: number) {
    keepingCopy = true;
    notice = '';
    try {
      const kept = await client.keepCopy(trackId, assetId);
      copies = kept;
      notice =
        kept.removed === 1
          ? 'Moved 1 copy to the Recycle Bin'
          : `Moved ${kept.removed.toLocaleString('en-GB')} copies to the Recycle Bin`;
    } catch (err) {
      notice = message(err);
    } finally {
      keepingCopy = false;
    }
  }
  const selectedCount = $derived((void selectionVersion, selected.size));
  const searchBusy = $derived(busy || !!activeOnlineJob);
  const findCount = $derived(selectedCount || counts.not_searched);
  const toolsScopeLabel = $derived(
    selectedCount > 0
      ? selectedCount === 1
        ? '1 ticked track'
        : `${selectedCount.toLocaleString('en-GB')} ticked tracks`
      : current
        ? current.track.title
        : 'This track',
  );
  let scopeKey = '';
  let sortKey = '';
  let finishedJob = untrack(() => recentOnlineJob?.id ?? null);
  // The sort is left out: reordering keeps the ticks, which stay valid.
  const contextKey = $derived(JSON.stringify([filterQuery, batchTrackIds]));

  function typeFind(value: string) {
    findText = value;
    clearTimeout(findTimer);
    findTimer = setTimeout(() => (appliedFind = findText.trim()), 200);
  }

  function sortBy(column: ListColumn) {
    listSort = nextListSort(listSort, column);
  }

  /** Sorts by status again even when it already is, so new matches move to the top. */
  function showMatchesFirst() {
    listSort = { column: 'status', direction: 'asc' };
    sortGeneration++;
  }

  function markOwnWrite() {
    suppressRevisions++;
    lastOwnWriteAt = Date.now();
  }

  /** The view's order and counts: the query's own sort, or grouped by stage in the core. */
  function viewOrder(): Promise<PrepOrderSnapshot> {
    const byStage = listSort?.column === 'status' ? listSort.direction : null;
    return client.prepOrder(byStage ? filterQuery : scopeQuery, batchTrackIds, byStage);
  }

  /** Items for these tracks, in the order given. Tracks the core no longer has are noted. */
  async function itemsFor(ids: number[]): Promise<PrepItem[]> {
    const chunks: number[][] = [];
    for (let start = 0; start < ids.length; start += 500)
      chunks.push(ids.slice(start, start + 500));
    const items = (await Promise.all(chunks.map((chunk) => client.prepItems(chunk)))).flat();
    const found = new Set(items.map((item) => item.track.id));
    for (const id of ids) if (!found.has(id)) gone.add(id);
    return items;
  }

  $effect(() => {
    const key = contextKey;
    const sorting = JSON.stringify([listSort, sortGeneration]);
    const currentRefresh = refresh;
    const currentRevision = revision;
    void scopeQuery;

    const newScope = key !== scopeKey;
    const newSort = sorting !== sortKey;
    const onlyRevisionChanged =
      !newScope && !newSort && currentRefresh === prevRefresh && currentRevision !== prevRevision;
    prevRevision = currentRevision;
    prevRefresh = currentRefresh;
    if (onlyRevisionChanged && suppressRevisions > 0 && Date.now() - lastOwnWriteAt < 1500) {
      suppressRevisions--;
      return;
    }
    if (onlyRevisionChanged) suppressRevisions = 0;

    const request = ++fetchGeneration;
    reloading = true;
    clearTimeout(fetchTimer);
    const hadFocus = untrack(() => !!workspace?.contains(document.activeElement));
    untrack(() => {
      if (newScope) {
        scopeKey = key;
        selected.clear();
        selectionVersion++;
        order = [];
        byId = {};
        activeId = null;
        reviewing = false;
        chosenCoverByTrack = {};
        loading = true;
        keyboardTarget = null;
      } else if (!order.length) {
        loading = true;
      }
      sortKey = sorting;
      loadError = '';
    });

    // A new view draws from the top; anything else refreshes the rows on screen.
    const shown = untrack(() => (newScope || !list ? { start: 0, end: 40 } : list.range()));
    const previous = untrack(() => order);
    const previousActive = untrack(() => activeId);
    const reorder = newScope || newSort || onlyRevisionChanged;
    void (async () => {
      const next = reorder
        ? await viewOrder()
        : {
            ids: previous,
            stages: (await client.prepOrder(scopeQuery, batchTrackIds, null)).stages,
          };
      gone.clear();
      // Only a new view or a new sort reorders; a library change keeps every place.
      const ids = reorder && !newScope && !newSort ? keepPlaces(previous, next.ids) : next.ids;
      const active = nextActive(previous, ids, previousActive);
      const loaded = await itemsFor(rowsToFetch(ids, shown.start, shown.end, active));
      return { ids: ids.filter((id) => !gone.has(id)), loaded, stages: next.stages, active };
    })()
      .then(({ ids, loaded, stages, active }) => {
        if (request !== fetchGeneration) return;
        order = ids;
        counts = stages;
        byId = Object.fromEntries(loaded.map((item) => [item.track.id, item]));
        if (selected.size > 0 && pruneSelection(selected, ids)) selectionVersion++;
        if (active !== activeId) {
          setActive(active);
          if (hadFocus && active != null) keyboardTarget = active;
        }
        reloading = false;
        loading = false;
        void focusKeyboardRow();
      })
      .catch((err: unknown) => {
        if (request !== fetchGeneration) return;
        byId = {};
        order = [];
        activeId = null;
        loadError = message(err);
        reloading = false;
        loading = false;
      });
  });

  // Rows that scroll into view are fetched in one request after scrolling settles briefly.
  let fetchTimer: ReturnType<typeof setTimeout> | undefined;
  function fetchWindow(ids: number[]) {
    if (reloading) return;
    const missing = ids.filter((id) => !(id in byId) && !fetching.has(id) && !gone.has(id));
    if (!missing.length) return;
    clearTimeout(fetchTimer);
    fetchTimer = setTimeout(() => void fetchRows(missing), 60);
  }

  async function fetchRows(ids: number[]) {
    const request = fetchGeneration;
    for (const id of ids) fetching.add(id);
    try {
      const next = await itemsFor(ids);
      if (request !== fetchGeneration) return;
      byId = { ...byId, ...Object.fromEntries(next.map((item) => [item.track.id, item])) };
      if (ids.some((id) => gone.has(id))) order = order.filter((id) => !gone.has(id));
      void focusKeyboardRow();
    } catch (err) {
      notice = message(err);
    } finally {
      for (const id of ids) fetching.delete(id);
    }
  }

  $effect(() => {
    const job = recentOnlineJob;
    if (job && job.id !== finishedJob) {
      finishedJob = job.id;
      notice =
        job.status === 'failed'
          ? (job.error_message ?? 'Search failed')
          : job.status === 'cancelled'
            ? 'Search cancelled'
            : '';
      untrack(() => {
        if (job.status !== 'failed' && job.status !== 'cancelled') void summariseSearch();
        refresh++;
      });
    }
  });

  async function summariseSearch() {
    const searched = [...searchingIds];
    searchingIds = new Set();
    if (!searched.length) {
      notice = 'Search finished';
      return;
    }
    try {
      const { stages } = await client.prepOrder(scopeQuery, searched, null);
      searchSummary = {
        searched: searched.length,
        review: stages.review,
        noMatch: stages.no_match,
      };
    } catch (err) {
      notice = message(err);
    }
  }
  function message(err: unknown) {
    return err instanceof Error ? err.message : String(err);
  }
  function setActive(id: number | null) {
    activeId = id;
  }

  function isSelected(id: number) {
    return selected.has(id);
  }
  function selectOne(id: number) {
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    selectionVersion++;
  }
  function clearTicks() {
    selected.clear();
    selectionVersion++;
  }
  /** The whole view's order is always known, so ticking everything needs no request. */
  function selectAll() {
    if (!totalCount) return;
    if (selectedCount === totalCount) {
      clearTicks();
      return;
    }
    for (const id of order) selected.add(id);
    selectionVersion++;
  }
  async function resolveSelectedIds() {
    if (!selected.size) return [];
    if (missing !== '' && selected.size > 0) {
      const validIds = await client.trackIds(scopeQuery, batchTrackIds, null);
      const kept = intersectSelection(selected, validIds);
      if (kept.length !== selected.size) {
        selected.clear();
        for (const id of kept) selected.add(id);
        selectionVersion++;
      }
      return kept;
    }
    return [...selected];
  }
  async function focusKeyboardRow() {
    const target = keyboardTarget;
    if (target == null || !(target in byId)) return;
    await tick();
    if (keyboardTarget !== target || activeId !== target) return;
    keyboardTarget = null;
    workspace.querySelector<HTMLElement>('[data-active="true"] .track-name')?.focus();
    const row = byId[target];
    if (row) onfocustrack?.(row.track);
  }

  function focusItem(item: PrepItem) {
    keyboardTarget = null;
    setActive(item.track.id);
    onfocustrack?.(item.track);
    if (compact) {
      reviewing = true;
      void tick().then(() =>
        workspace.querySelector<HTMLButtonElement>('.back-to-tracks button')?.focus(),
      );
    }
  }
  function backToTracks() {
    reviewing = false;
    keyboardTarget = activeId;
    void focusKeyboardRow();
  }
  function clearChosenCover(trackId: number) {
    if (trackId in chosenCoverByTrack) {
      const next = { ...chosenCoverByTrack };
      delete next[trackId];
      chosenCoverByTrack = next;
    }
  }
  function updateItem(updated: PrepItem) {
    byId = { ...byId, [updated.track.id]: updated };
  }
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    failures = [];
    notice = '';
    try {
      await action();
    } catch (err) {
      notice = message(err);
      refresh++;
    } finally {
      busy = false;
    }
  }
  function recordSave(report: AcceptedTagWriteReport, track: number | null) {
    failures = report.failed;
    if (report.operation_id !== null) {
      history = [...history, { operation: report.operation_id, track }];
      if (track !== null) clearChosenCover(track);
      else chosenCoverByTrack = {};
    }
    notice = formatSaveNotice(report);
  }
  function acceptTrack(item: PrepItem, fields: TagName[], includeArtwork: boolean) {
    if (reloading || item.track.id !== activeId || (!fields.length && !includeArtwork)) return;
    void run(async () => {
      markOwnWrite();
      const report = await client.acceptAndWrite(
        [item.track.id],
        fields,
        includeArtwork,
        item.review_token,
      );
      recordSave(report, item.track.id);
      refresh++;
    });
  }
  function skipTrack(item: PrepItem) {
    if (reloading || busy) return;
    void run(async () => {
      markOwnWrite();
      await client.skipItems([item.track.id], []);
      notice = 'Skipped ' + item.track.title;
      refresh++;
    });
  }
  function saveTicked() {
    if (busy || reloading || !!loadError || !selectedCount) return;
    void resolveSelectedIds().then((target) => {
      if (!target.length) return;
      void run(async () => {
        markOwnWrite();
        recordSave(await client.acceptAndWrite(target, null, true), null);
        refresh++;
      });
    });
  }
  function undo() {
    const saved = lastAction;
    if (!saved) return;
    void run(async () => {
      const result = await client.undoOperation(saved.operation);
      markOwnWrite();
      if (saved.track !== null) clearChosenCover(saved.track);
      else chosenCoverByTrack = {};
      notice = result.restored
        ? 'Restored ' +
          result.restored.toLocaleString('en-GB') +
          (result.restored === 1 ? ' file' : ' files')
        : 'Library changes undone';
      history = history.slice(0, -1);
      refresh++;
    });
  }
  function runOnlineSearch(target: number[]) {
    if (!target.length) return;
    searchingIds = new Set(target);
    searchSummary = null;
    void run(async () => {
      const result = await client.identifyOnline(target, true);
      const done = result.recent.find((job) => job.kind === 'identify_online');
      const running = result.active.some((job) => job.kind === 'identify_online');
      if (!running && done) {
        finishedJob = done.id;
      }
      notice = running || done?.status !== 'failed' ? '' : (done.error_message ?? 'Search failed');
      if (!running) {
        markOwnWrite();
        await summariseSearch();
      }
      refresh++;
    });
  }
  function findMatches() {
    if (searchBusy || !!loadError || !findCount) return;
    if (selectedCount) {
      void resolveSelectedIds().then(runOnlineSearch);
      return;
    }
    void client
      .trackIds(scopeQuery, batchTrackIds, 'not_searched')
      .then(runOnlineSearch)
      .catch((err: unknown) => (notice = message(err)));
  }
  function findMatch(item: PrepItem) {
    if (!searchBusy) runOnlineSearch([item.track.id]);
  }
  function chooseMatch(item: PrepItem, index: number) {
    if (reloading || activeId !== item.track.id || index === item.selected_candidate) return;
    void run(async () => {
      const updated = await client.selectCandidate(item.track.id, index, item.review_token);
      clearChosenCover(item.track.id);
      updateItem(updated);
    });
  }
  function useCover(item: PrepItem, index: number) {
    if (reloading || activeId !== item.track.id) return;
    void run(async () => {
      const updated = await client.useCandidateArtwork(
        item.track.id,
        index,
        false,
        item.review_token,
      );
      chosenCoverByTrack = { ...chosenCoverByTrack, [item.track.id]: index };
      updateItem(updated);
    });
  }
  function keepFileCover(item: PrepItem) {
    if (reloading || activeId !== item.track.id) return;
    void run(async () => {
      markOwnWrite();
      const updated = await client.skipArtwork(item.track.id);
      chosenCoverByTrack = Object.fromEntries(
        Object.entries(chosenCoverByTrack).filter(([id]) => Number(id) !== item.track.id),
      );
      updateItem(updated);
    });
  }

  // Covers of the matches on the open track, fetched for viewing as soon as it opens. Keyed by
  // track and match, so moving between tracks keeps what was already fetched.
  let matchArt = $state.raw<Record<string, string | null>>({});
  // Plain bookkeeping for requests already sent; nothing renders from it.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  const artRequested = new Set<string>();
  $effect(() => {
    const item = current;
    if (!item) return;
    const wanted = item.candidates
      .filter((candidate) => candidate.has_artwork && !candidate.artwork_hash)
      .map((candidate) => candidate.index)
      .filter((index) => !artRequested.has(item.track.id + ':' + index))
      .slice(0, 8);
    if (!wanted.length) return;
    untrack(() => void loadMatchArt(item.track.id, wanted));
  });

  async function loadMatchArt(trackId: number, indexes: number[]) {
    for (const index of indexes) {
      const key = trackId + ':' + index;
      if (artRequested.has(key)) continue;
      artRequested.add(key);
      try {
        const result = await client.candidateArtwork(trackId, index);
        const found: Record<string, string | null> = {};
        for (const state of result.candidates) {
          found[trackId + ':' + state.index] = state.artwork_hash;
        }
        matchArt = { ...matchArt, ...found };
      } catch {
        // A cover that cannot be fetched leaves the placeholder. The match itself still works.
      }
    }
  }
  async function lookupLink(item: PrepItem, link: string): Promise<boolean> {
    if (busy || linkBusy) return false;
    linkBusy = true;
    let found = false;
    await run(async () => {
      try {
        const result = await client.identifyWithUrl(item.track.id, link);
        notice = formatUrlLookupNotice(result);
        if (result.best) {
          found = true;
          markOwnWrite();
          refresh++;
        }
      } finally {
        linkBusy = false;
      }
    });
    return found;
  }
  async function editTargets() {
    if (selectedCount > 0) return resolveSelectedIds();
    return activeId != null ? [activeId] : [];
  }
  async function showBatch() {
    const targets = await editTargets();
    toolIds = targets;
    if (toolIds.length) batchOpen = true;
  }
  async function showWrites() {
    const targets = await editTargets();
    toolIds = targets;
    if (toolIds.length) writeOpen = true;
  }
  async function renameTargets() {
    const targets = await editTargets();
    if (targets.length) onrename?.({ kind: 'tracks', track_ids: targets }, toolsScopeLabel);
  }

  function onKey(event: KeyboardEvent) {
    if (
      event.defaultPrevented ||
      event.altKey ||
      batchOpen ||
      writeOpen ||
      document.querySelector('dialog[open], [role="menu"]')
    )
      return;
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.matches(
        'input:not([type=checkbox]),textarea,select,[role="slider"],[role="combobox"],[role="listbox"]',
      ) ||
        target.isContentEditable)
    )
      return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z' && lastAction) {
      event.preventDefault();
      undo();
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a' && totalCount) {
      event.preventDefault();
      selectAll();
      return;
    }
    if (event.ctrlKey || event.metaKey || busy) return;
    if (detailOpen && event.key === 'Escape') {
      event.preventDefault();
      backToTracks();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      if (!order.length || detailOpen) return;
      const offset = event.key === 'ArrowDown' ? 1 : -1;
      const currentIdx = activeId == null ? -1 : order.indexOf(activeId);
      const nextIdx = (currentIdx === -1 ? (offset > 0 ? -1 : order.length) : currentIdx) + offset;
      if (nextIdx >= 0 && nextIdx < order.length) {
        const id = order[nextIdx]!;
        event.preventDefault();
        if (event.shiftKey && !selected.has(id)) {
          selected.add(id);
          selectionVersion++;
        }
        setActive(id);
        keyboardTarget = id;
        list?.reveal(nextIdx);
        void focusKeyboardRow();
      }
      return;
    }
    const key = event.key.toLowerCase();
    const plainTarget =
      !(target instanceof HTMLElement) ||
      target === document.body ||
      target === workspace ||
      target.classList.contains('track-name') ||
      !target.closest('button,a[href],[role="button"],[role="tab"],summary,input,select,textarea');
    if (key === 'x' && current) {
      event.preventDefault();
      selectOne(current.track.id);
    } else if (event.shiftKey && key === 'a' && selectedCount) {
      event.preventDefault();
      saveTicked();
    } else if (event.key === 'Enter' && current && plainTarget) {
      event.preventDefault();
      if (compact && !detailOpen) focusItem(current);
      else if (reviewableFields(current).length > 0 || current.artwork.proposed)
        review?.acceptAll();
      else if (current.stage !== 'done') findMatch(current);
    } else if (
      key === 's' &&
      !event.shiftKey &&
      current &&
      (reviewableFields(current).length > 0 || !!current.artwork.proposed)
    ) {
      event.preventDefault();
      skipTrack(current);
    } else if (key === 'f' && !event.shiftKey) {
      event.preventDefault();
      if (selectedCount) findMatches();
      else if (current) findMatch(current);
    } else if (/^[1-9]$/.test(event.key) && current && current.candidates.length > 1) {
      const candidate = current.candidates[Number(event.key) - 1];
      if (candidate) {
        event.preventDefault();
        chooseMatch(current, candidate.index);
      }
    } else if (key === 'b' || key === 'e') {
      event.preventDefault();
      void showBatch();
    } else if (key === 'w') {
      event.preventDefault();
      void showWrites();
    } else if (
      event.key === ' ' &&
      current &&
      target instanceof HTMLElement &&
      target.classList.contains('track-name')
    ) {
      event.preventDefault();
      onpreview?.(current.track);
    }
  }
</script>

<svelte:window onkeydown={onKey} />
<section
  class="prep"
  aria-label="Prep"
  tabindex="-1"
  bind:this={workspace}
  bind:clientWidth={width}
>
  <div class="bar" hidden={detailOpen}>
    <div class="tools" role="toolbar" aria-label="Prep controls">
      <input
        class="find"
        type="search"
        aria-label="Find tracks"
        placeholder="Find tracks"
        value={findText}
        oninput={(event) => typeFind(event.currentTarget.value)}
        onkeydown={(event) => {
          if (event.key === 'Escape' && findText) {
            event.stopPropagation();
            typeFind('');
          }
        }}
      />
      <select
        aria-label="Filter tracks"
        value={missing}
        onchange={(event) => (missing = event.currentTarget.value as MissingField | '')}
      >
        <option value="">Any tags</option>
        {#each MISSING_FIELDS as field (field)}
          <option value={field}>Missing {MISSING_LABELS[field].toLowerCase()}</option>
        {/each}
      </select>
      {#if batchTrackIds !== null || batchQuery !== null}
        <Button
          size="sm"
          onclick={onclearbatch}
          aria-label="Clear library scope"
          title={'Clear ' + (batchLabel ?? 'Library selection')}
          ><span class="scope-label">{batchLabel ?? 'Library selection'}</span>
          <Icon name="close" size="xs" />
        </Button>
      {/if}
      {#if stageSummary(counts).length}<span class="summary"
          >{#each stageSummary(counts) as part, index (part.words)}{index > 0 ? ' · ' : ''}<span
              class="numeric">{part.number}</span
            >
            {part.words}{/each}</span
        >{/if}
      <span class="spacer"></span>
      {#if !activeOnlineJob}
        {#if selectedCount > 0}
          <span class="ticked numeric">{selectedCount.toLocaleString('en-GB')} ticked</span>
          <Button size="sm" onclick={clearTicks}>Clear</Button>
          <Button
            size="sm"
            disabled={busy || reloading || !!loadError}
            title="Save every suggestion for the ticked tracks (Shift+A)"
            onclick={saveTicked}>Save ticked</Button
          >
        {/if}
        {#if findCount > 0}
          <Button
            variant="primary"
            size="sm"
            disabled={searchBusy || !!loadError}
            title="Search online (F)"
            onclick={findMatches}
          >
            <Icon name="search" size="xs" />
            {selectedCount
              ? 'Find matches'
              : findCount === 1
                ? 'Find a match'
                : 'Find matches for ' + findCount.toLocaleString('en-GB')}
          </Button>
        {/if}
      {/if}
      {#if lastAction && (lastAction.track === null || lastAction.track !== activeId)}
        <Button
          size="sm"
          ariaLabel="Undo save"
          title="Undo the last save (Ctrl+Z)"
          disabled={busy}
          onclick={undo}><Icon name="undo" size="xs" /></Button
        >
      {/if}
    </div>
  </div>

  {#if activeOnlineJob}
    {@const done = activeOnlineJob.done_items}
    {@const all = activeOnlineJob.total_items}
    <div class="search-strip" role="status" aria-live="polite">
      <span class="search-text">
        {activeOnlineJob.paused ? 'Search paused' : 'Searching online'}
        {#if all != null}
          <span class="numeric"
            >{done.toLocaleString('en-GB')} of {all.toLocaleString('en-GB')} tracks</span
          >
        {/if}
      </span>
      <span class="search-track" aria-hidden="true">
        <span
          class="search-fill"
          style:transform={'scaleX(' + (all ? Math.min(1, done / all) : 0) + ')'}
        ></span>
      </span>
      {#if onopenactivity}
        <Button size="sm" variant="quiet" onclick={onopenactivity}>Details</Button>
      {/if}
    </div>
  {:else if searchSummary}
    {@const summary = searchSummary}
    <div class="search-strip done" role="status" aria-live="polite">
      <span class="search-text">
        Search finished for <span class="numeric">{summary.searched.toLocaleString('en-GB')}</span>
        {summary.searched === 1 ? 'track' : 'tracks'} ·
        <span class="numeric">{summary.review.toLocaleString('en-GB')}</span> to review ·
        <span class="numeric">{summary.noMatch.toLocaleString('en-GB')}</span> with no match
      </span>
      {#if summary.review > 0}
        <Button size="sm" onclick={showMatchesFirst}>Show them first</Button>
      {/if}
      <Button size="sm" variant="icon" ariaLabel="Dismiss" onclick={() => (searchSummary = null)}
        ><Icon name="close" size="xs" /></Button
      >
    </div>
  {/if}

  {#if notice || failures.length}
    <div class="feedback" hidden={detailOpen && !failures.length}>
      <span role="status">{notice}</span>
      {#if failures.length}
        <details>
          <summary
            >{failures.length.toLocaleString('en-GB')}
            {failures.length === 1 ? 'file needs' : 'files need'} attention</summary
          >
          <ul>
            {#each failures as failure (failure.track_id)}
              <li>{failure.path} · {failure.reason}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </div>
  {/if}

  {#if loadError}
    <div class="empty" role="alert">
      <p>{loadError}</p>
      <Button size="sm" onclick={() => refresh++}>Try again</Button>
    </div>
  {:else if loading && !totalCount}
    <p class="empty" role="status">Loading tracks…</p>
  {:else if !totalCount}
    <div class="empty">
      <h2>
        {missing
          ? 'No tracks missing ' + MISSING_LABELS[missing].toLowerCase()
          : appliedFind
            ? 'No tracks match ' + appliedFind
            : 'No tracks in this view'}
      </h2>
      {#if appliedFind}
        <Button size="sm" onclick={() => typeFind('')}>Clear find</Button>
      {:else if missing}
        <Button size="sm" onclick={() => (missing = '')}>Show all tracks</Button>
      {:else if onopenlibrary}
        <Button size="sm" onclick={onopenlibrary}>Go to Library</Button>
      {/if}
    </div>
  {:else}
    <div class="workspace" class:detail-open={detailOpen}>
      <PrepTrackList
        bind:this={list}
        {order}
        {byId}
        {activeId}
        sort={listSort}
        tickedCount={selectedCount}
        tickVersion={selectionVersion}
        isTicked={isSelected}
        {searchingIds}
        searching={!!activeOnlineJob}
        {loading}
        onsort={sortBy}
        ontickall={selectAll}
        ontick={selectOne}
        onopen={focusItem}
        onwindow={fetchWindow}
      />
      {#if current}
        {@const item = current}
        <PrepReview
          bind:this={review}
          {item}
          busy={busy || reloading}
          searching={searchBusy}
          {linkBusy}
          {compact}
          chosenCover={chosenCoverByTrack[item.track.id]}
          artFor={(index) => matchArt[item.track.id + ':' + index] ?? null}
          onkeepcover={() => keepFileCover(item)}
          canUndo={lastAction !== null && lastAction.track === item.track.id}
          onaccept={(fields, artwork) => acceptTrack(item, fields, artwork)}
          onskip={() => skipTrack(item)}
          onchoose={(index) => chooseMatch(item, index)}
          oncover={(index) => useCover(item, index)}
          onlookup={(link) => lookupLink(item, link)}
          onsearch={() => findMatch(item)}
          onedit={() => void showBatch()}
          onwrite={() => void showWrites()}
          onrename={onrename ? () => void renameTargets() : undefined}
          onpreview={onpreview ? () => onpreview(item.track) : undefined}
          onback={backToTracks}
          onundo={undo}
          {onopenproviders}
          copies={copies?.track_id === item.track.id ? copies : null}
          copyBusy={keepingCopy}
          onkeepcopy={(assetId) => void keepCopy(item.track.id, assetId)}
        />
      {:else if loading}
        <p class="empty" role="status">Loading tracks…</p>
      {/if}
    </div>
  {/if}
</section>

{#if batchOpen}
  <BatchTagSheet
    bind:open={batchOpen}
    trackIds={toolIds}
    sourceTrackId={activeId}
    sourceTrackTitle={current?.track.title}
    scopeLabel={toolsScopeLabel}
    client={{ preview: client.previewBatchTag, apply: client.applyBatchTag }}
    onapplied={() => {
      markOwnWrite();
      notice = 'Edits saved in the library. Write them to the files with W';
      refresh++;
    }}
  />
{/if}
{#if writeOpen}
  <WriteTagsSheet
    bind:open={writeOpen}
    trackIds={toolIds}
    scopeLabel={toolsScopeLabel}
    client={{ preview: client.previewTagWrites, apply: client.applyTagWrites }}
    onwritten={(result) => {
      failures = result.failed;
      if (result.operation_id != null) {
        markOwnWrite();
        history = [...history, { operation: result.operation_id, track: null }];
      }
      notice = formatWriteNotice(result);
      refresh++;
    }}
  />
{/if}

<style>
  .prep {
    min-width: 0;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    container-type: inline-size;
    background: var(--color-background);
    color: var(--color-text);
  }
  .bar {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-2-5);
    min-width: 0;
    padding: var(--space-3) var(--space-4);
    background: var(--color-surface);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .bar[hidden] {
    display: none;
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2-5);
    min-width: 0;
  }
  .tools :global(button) {
    gap: var(--space-1-5);
  }
  .spacer {
    flex: 1;
  }
  .search-strip {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    background: var(--color-surface-2);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .search-strip.done {
    grid-template-columns: minmax(0, 1fr) auto auto;
  }
  .search-text {
    display: inline-flex;
    flex-wrap: wrap;
    gap: var(--space-1-5);
    align-items: baseline;
    color: var(--color-text);
  }
  .search-track {
    height: var(--space-1);
    border-radius: var(--radius-xs);
    background: var(--color-border);
    overflow: hidden;
  }
  .search-fill {
    display: block;
    height: 100%;
    background: var(--color-text);
    transform-origin: left;
    transition: transform var(--duration-fast) var(--ease-out);
  }
  .ticked,
  .feedback,
  .summary {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  select,
  .find {
    font: inherit;
    font-size: var(--text-sm);
    height: var(--control-height-sm);
    max-width: 100%;
    min-width: 0;
    color: var(--color-text);
    background: var(--color-surface-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    padding-inline: var(--space-2-5);
  }
  .feedback {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .feedback[hidden] {
    display: none;
  }
  .feedback ul {
    margin: var(--space-2) 0 0;
    padding-left: var(--space-4);
  }
  .workspace {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    min-width: 0;
    min-height: 0;
  }
  .find {
    width: 12rem;
  }
  .empty {
    display: grid;
    justify-items: center;
    align-content: center;
    gap: var(--space-3);
    flex: 1;
    margin: 0;
    padding: var(--space-8) var(--space-4);
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }
  .empty h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    color: var(--color-text);
  }
  .scope-label {
    max-width: 16rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @container (max-width: 800px) {
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
    .workspace:not(.detail-open) > :global(.review),
    .workspace.detail-open > :global(.tracks) {
      display: none;
    }
    .workspace :global(.tracks) {
      border-right: 0;
    }
  }
  @media (forced-colors: active) {
    .search-fill {
      background: Highlight;
    }
    .search-track {
      outline: 1px solid CanvasText;
    }
  }
</style>
