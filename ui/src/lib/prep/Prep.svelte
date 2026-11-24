<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import SegmentedTabs from '../design/SegmentedTabs.svelte';
  import Thumb from '../design/Thumb.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { AcceptedTagWriteReport } from '../ipc/bindings/AcceptedTagWriteReport';
  import type { AcceptedProposals } from '../ipc/bindings/AcceptedProposals';
  import type { ActivityJob } from '../ipc/bindings/ActivityJob';
  import type { ActivitySnapshot } from '../ipc/bindings/ActivitySnapshot';
  import type { BatchTagTool } from '../ipc/bindings/BatchTagTool';
  import type { FileTagWrite } from '../ipc/bindings/FileTagWrite';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { PrepItem } from '../ipc/bindings/PrepItem';
  import type { PrepPageSnapshot } from '../ipc/bindings/PrepPageSnapshot';
  import type { PrepStageCounts } from '../ipc/bindings/PrepStageCounts';
  import type { PrepStageName } from '../ipc/bindings/PrepStageName';
  import type { RenameScope } from '../ipc/bindings/RenameScope';
  import type { TagName } from '../ipc/bindings/TagName';
  import type { TagWriteReport } from '../ipc/bindings/TagWriteReport';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import type { TrackIdentification } from '../ipc/bindings/TrackIdentification';
  import type { UndoReport } from '../ipc/bindings/UndoReport';
  import BatchTagSheet from './BatchTagSheet.svelte';
  import PrepReview from './PrepReview.svelte';
  import WriteTagsSheet from './WriteTagsSheet.svelte';
  import {
    MISSING_FIELDS,
    MISSING_LABELS,
    STAGE_TABS,
    buildMissingQuery,
    createSelectionSet,
    emptyStageText,
    firstWorkingTab,
    formatSaveNotice,
    formatUrlLookupNotice,
    formatWriteNotice,
    intersectSelection,
    pruneSelection,
    rowStatus,
    stageCount,
    type MissingField,
    type StageTab,
  } from './prep-view';

  export interface PrepClient {
    trackIds: (
      query: TrackQuery | null,
      trackIds: number[] | null,
      stage: PrepStageName | null,
    ) => Promise<number[]>;
    prepPage: (
      query: TrackQuery | null,
      trackIds: number[] | null,
      stage: PrepStageName | null,
      offset: number,
      limit: number,
    ) => Promise<PrepPageSnapshot>;
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
    identifyOnline: (trackIds: number[], onlySelected: boolean) => Promise<ActivitySnapshot>;
    identifyWithUrl: (trackId: number, url: string) => Promise<TrackIdentification>;
    previewBatchTag: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    applyBatchTag: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    previewTagWrites: (trackIds: number[]) => Promise<FileTagWrite[]>;
    applyTagWrites: (trackIds: number[]) => Promise<TagWriteReport>;
    undoOperation: (operationId: number) => Promise<UndoReport>;
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

  const pageSize = 50;
  const noCounts: PrepStageCounts = { review: 0, not_searched: 0, no_match: 0, done: 0 };
  let missing = $state<MissingField | ''>('');
  let stage = $state<StageTab>('all');
  // The first load in a scope opens the tab with work in it; after that the user's choice stands.
  let stageChosen = false;
  let counts = $state<PrepStageCounts>(noCounts);
  let totalCount = $state(0);
  let items = $state.raw<PrepItem[]>([]);
  let page = $state(0);
  let activeId = $state<number | null>(null);
  let activePosition = 0;
  const selected = createSelectionSet();
  let selectionVersion = $state(0);
  let selectingAll = $state(false);
  let selectAllPromise: Promise<void> | null = null;
  let pendingSelectToggles: number[] = [];
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
  interface SavedChange {
    operation: number;
    track: number | null;
  }
  let history = $state.raw<SavedChange[]>([]);
  const lastAction = $derived(history.at(-1) ?? null);
  let workspace: HTMLElement;
  let review = $state<ReturnType<typeof PrepReview>>();
  let width = $state(0);
  let reviewing = $state(false);
  const compact = $derived(width > 0 && width <= 800);
  let keyboardTarget: number | null = null;
  let pendingEdge = $state<'first' | 'last' | null>(null);
  let pendingShiftSelect = false;
  let reloading = $state(false);
  let scopeGeneration = 0;
  let fetchGeneration = 0;
  let suppressRevisions = 0;
  let lastOwnWriteAt = 0;
  let prevRevision = untrack(() => revision);
  let prevRefresh = 0;
  let prevPage = 0;

  const stageFilter = $derived<PrepStageName | null>(stage === 'all' ? null : stage);
  const scopeQuery = $derived(buildMissingQuery(batchQuery, missing));
  const current = $derived(items.find((item) => item.track.id === activeId) ?? null);
  const detailOpen = $derived(compact && reviewing && current !== null);
  const selectedCount = $derived((void selectionVersion, selected.size));
  const pageCount = $derived(Math.ceil(totalCount / pageSize));
  const searchBusy = $derived(busy || !!activeOnlineJob || selectingAll);
  const viewCount = $derived(stageCount(counts, 'all'));
  const tabs = $derived(
    STAGE_TABS.map((tab) => ({ id: tab.id, label: tab.label, count: stageCount(counts, tab.id) })),
  );
  const findCount = $derived(selectedCount || (stage === 'done' ? 0 : totalCount));
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
  let finishedJob = untrack(() => recentOnlineJob?.id ?? null);
  // The stage is left out: moving between tabs keeps the ticks, which stay valid across them.
  const contextKey = $derived(JSON.stringify([scopeQuery, batchTrackIds]));

  function markOwnWrite() {
    suppressRevisions++;
    lastOwnWriteAt = Date.now();
  }

  $effect(() => {
    const key = contextKey;
    const query = scopeQuery;
    const scope = batchTrackIds;
    const wanted = stageFilter;
    const currentPage = page;
    const currentRefresh = refresh;
    const currentRevision = revision;

    const onlyRevisionChanged =
      key === scopeKey &&
      currentPage === prevPage &&
      currentRefresh === prevRefresh &&
      currentRevision !== prevRevision;
    prevRevision = currentRevision;
    prevRefresh = currentRefresh;
    prevPage = currentPage;
    if (onlyRevisionChanged && suppressRevisions > 0 && Date.now() - lastOwnWriteAt < 1500) {
      suppressRevisions--;
      return;
    }
    if (onlyRevisionChanged) {
      suppressRevisions = 0;
    }

    const request = ++fetchGeneration;
    reloading = true;
    untrack(() => {
      if (scopeKey !== key) {
        scopeKey = key;
        scopeGeneration++;
        selected.clear();
        selectionVersion++;
        selectingAll = false;
        selectAllPromise = null;
        totalCount = 0;
        items = [];
        page = 0;
        prevPage = 0;
        activeId = null;
        activePosition = 0;
        reviewing = false;
        chosenCoverByTrack = {};
        loading = true;
        keyboardTarget = null;
        pendingEdge = null;
        pendingShiftSelect = false;
      } else if (!items.length) {
        loading = true;
      }
      loadError = '';
    });

    const offset = untrack(() => page) * pageSize;
    void client
      .prepPage(query, scope, wanted, offset, pageSize)
      .then((result) => {
        if (request !== fetchGeneration) return;
        counts = result.stages;
        if (!stageChosen) {
          stageChosen = true;
          const opening = firstWorkingTab(result.stages);
          if (opening !== stage) {
            stage = opening;
            return;
          }
        }
        totalCount = result.total;
        if (result.total > 0 && offset >= result.total) {
          page = Math.max(0, Math.ceil(result.total / pageSize) - 1);
          return;
        }
        items = result.items;
        if (viewCount === 0) {
          if (selected.size > 0) {
            selected.clear();
            selectionVersion++;
          }
        } else if (
          wanted === null &&
          page === 0 &&
          result.items.length === result.total &&
          selected.size > 0 &&
          pruneSelection(
            selected,
            result.items.map((item) => item.track.id),
          )
        ) {
          selectionVersion++;
        }

        if (pendingEdge && result.items.length > 0) {
          const edgeItem =
            pendingEdge === 'first' ? result.items[0] : result.items[result.items.length - 1];
          pendingEdge = null;
          const shiftSelect = pendingShiftSelect;
          pendingShiftSelect = false;
          if (edgeItem) {
            if (shiftSelect && !selected.has(edgeItem.track.id)) {
              selected.add(edgeItem.track.id);
              selectionVersion++;
            }
            setActive(edgeItem.track.id);
            keyboardTarget = edgeItem.track.id;
          }
        } else {
          pendingEdge = null;
          pendingShiftSelect = false;
          if (!items.some((item) => item.track.id === activeId)) {
            // A saved track leaves its tab, so the next one moves up into its place.
            const next = items[Math.min(activePosition, items.length - 1)] ?? null;
            const hadFocus = workspace.contains(document.activeElement);
            setActive(next?.track.id ?? null);
            if (next && hadFocus) keyboardTarget = next.track.id;
          }
        }
        reloading = false;
        loading = false;
        void focusKeyboardRow();
      })
      .catch((err: unknown) => {
        if (request !== fetchGeneration) return;
        pendingEdge = null;
        pendingShiftSelect = false;
        items = [];
        totalCount = 0;
        activeId = null;
        loadError = message(err);
        reloading = false;
        loading = false;
      });
  });

  $effect(() => {
    const job = recentOnlineJob;
    if (job && job.id !== finishedJob) {
      finishedJob = job.id;
      notice =
        job.status === 'failed'
          ? (job.error_message ?? 'Search failed')
          : job.status === 'cancelled'
            ? 'Search cancelled'
            : 'Search finished';
      untrack(() => refresh++);
    }
  });

  function message(err: unknown) {
    return err instanceof Error ? err.message : String(err);
  }
  function setActive(id: number | null) {
    activeId = id;
    const position = items.findIndex((item) => item.track.id === id);
    if (position >= 0) activePosition = position;
  }
  function chooseStage(id: string) {
    stageChosen = true;
    stage = id as StageTab;
    page = 0;
    activePosition = 0;
    reviewing = false;
  }
  function isSelected(id: number) {
    void selectionVersion;
    const toggled = pendingSelectToggles.includes(id);
    return selectAllPromise ? (selectingAll ? !toggled : toggled) : selected.has(id);
  }
  function selectOne(id: number) {
    if (selectAllPromise) {
      const idx = pendingSelectToggles.indexOf(id);
      if (idx >= 0) pendingSelectToggles.splice(idx, 1);
      else pendingSelectToggles.push(id);
      selectionVersion++;
      return;
    }
    if (selected.has(id)) selected.delete(id);
    else selected.add(id);
    selectionVersion++;
  }
  function clearTicks() {
    scopeGeneration++;
    selectingAll = false;
    selectAllPromise = null;
    pendingSelectToggles = [];
    selected.clear();
    selectionVersion++;
  }
  function selectAll() {
    if (!totalCount) return;
    if ((selectedCount === totalCount && !selectAllPromise) || (selectAllPromise && selectingAll)) {
      clearTicks();
      return;
    }
    if (page === 0 && items.length === totalCount) {
      selected.clear();
      for (const item of items) selected.add(item.track.id);
      pendingSelectToggles = [];
      selectionVersion++;
      return;
    }
    const gen = scopeGeneration;
    pendingSelectToggles = [];
    selectingAll = true;
    const promise = client
      .trackIds(scopeQuery, batchTrackIds, stageFilter)
      .then((allIds) => {
        if (gen !== scopeGeneration) return;
        selected.clear();
        for (const id of allIds) selected.add(id);
        for (const id of pendingSelectToggles) {
          if (selected.has(id)) selected.delete(id);
          else selected.add(id);
        }
        pendingSelectToggles = [];
        selectionVersion++;
      })
      .catch((err: unknown) => {
        if (gen === scopeGeneration) notice = message(err);
      })
      .finally(() => {
        if (gen === scopeGeneration) {
          selectingAll = false;
          selectAllPromise = null;
        }
      });
    selectAllPromise = promise;
  }
  async function resolveSelectedIds() {
    if (selectAllPromise) await selectAllPromise;
    if (!selected.size) return [];
    if (missing !== '' && totalCount > items.length) {
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
    if (target == null || !items.some((item) => item.track.id === target)) return;
    await tick();
    if (keyboardTarget !== target || activeId !== target) return;
    keyboardTarget = null;
    workspace.querySelector<HTMLElement>('[data-active="true"] .track-name')?.focus();
    const row = items.find((item) => item.track.id === target);
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
    items = items.map((item) => (item.track.id === updated.track.id ? updated : item));
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
  function saveTrack(
    item: PrepItem,
    fields: TagName[],
    includeArtwork: boolean,
    skipRest: boolean,
  ) {
    if (reloading || item.track.id !== activeId) return;
    void run(async () => {
      markOwnWrite();
      if (fields.length || includeArtwork) {
        const report = await client.acceptAndWrite(
          [item.track.id],
          fields,
          includeArtwork,
          item.review_token,
        );
        recordSave(report, item.track.id);
        if (report.failed.length) {
          refresh++;
          return;
        }
      } else {
        notice = 'Skipped';
      }
      if (skipRest) await client.skipItems([item.track.id], []);
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
    void run(async () => {
      const result = await client.identifyOnline(target, true);
      const done = result.recent.find((job) => job.kind === 'identify_online');
      const running = result.active.some((job) => job.kind === 'identify_online');
      if (!running && done) {
        finishedJob = done.id;
      }
      notice = running
        ? ''
        : done?.status === 'failed'
          ? (done.error_message ?? 'Search failed')
          : 'Search finished';
      if (!running) markOwnWrite();
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
      .trackIds(scopeQuery, batchTrackIds, stageFilter)
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
      if (!items.length || detailOpen) return;
      if (target instanceof HTMLElement && target.closest('[role="tablist"]')) return;
      const offset = event.key === 'ArrowDown' ? 1 : -1;
      const currentIdx = items.findIndex((item) => item.track.id === activeId);
      const nextIdx = (currentIdx === -1 ? (offset > 0 ? -1 : items.length) : currentIdx) + offset;
      if (nextIdx >= 0 && nextIdx < items.length) {
        const id = items[nextIdx]!.track.id;
        event.preventDefault();
        if (event.shiftKey && !selected.has(id)) {
          selected.add(id);
          selectionVersion++;
        }
        setActive(id);
        keyboardTarget = id;
        void focusKeyboardRow();
        return;
      }
      if (offset > 0 && page + 1 < pageCount) {
        event.preventDefault();
        pendingEdge = 'first';
        pendingShiftSelect = event.shiftKey;
        page++;
        return;
      }
      if (offset < 0 && page > 0) {
        event.preventDefault();
        pendingEdge = 'last';
        pendingShiftSelect = event.shiftKey;
        page--;
        return;
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
      else if (current.stage === 'not_searched') findMatch(current);
      else review?.save();
    } else if (key === 's' && !event.shiftKey && current && current.stage === 'review') {
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
    <div class="stages">
      <SegmentedTabs items={tabs} value={stage} ariaLabel="Prep stages" onChange={chooseStage} />
    </div>
    <div class="tools" role="toolbar" aria-label="Prep controls">
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
      <span class="spacer"></span>
      {#if activeOnlineJob}
        <span class="progress" role="status">
          {activeOnlineJob.paused ? 'Search paused' : 'Searching'}
          {#if activeOnlineJob.total_items != null}
            <span class="numeric"
              >{activeOnlineJob.done_items.toLocaleString('en-GB')} / {activeOnlineJob.total_items.toLocaleString(
                'en-GB',
              )}</span
            >
          {/if}
        </span>
        {#if onopenactivity}
          <Button size="sm" onclick={onopenactivity}>Activity</Button>
        {/if}
      {:else}
        {#if selectedCount > 0}
          <span class="ticked numeric">{selectedCount.toLocaleString('en-GB')} ticked</span>
          <Button size="sm" onclick={clearTicks}>Clear</Button>
          <Button
            size="sm"
            disabled={busy || reloading || !!loadError || selectingAll}
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
        {missing && (viewCount === 0 || stage === 'all')
          ? 'No tracks missing ' + MISSING_LABELS[missing].toLowerCase()
          : viewCount === 0
            ? 'No tracks in this view'
            : emptyStageText(stage)}
      </h2>
      {#if viewCount > 0 && stage !== 'all'}
        <Button size="sm" onclick={() => chooseStage('all')}>Show all tracks</Button>
      {:else if missing}
        <Button size="sm" onclick={() => (missing = '')}>Show all tracks</Button>
      {:else if onopenlibrary}
        <Button size="sm" onclick={onopenlibrary}>Go to Library</Button>
      {/if}
    </div>
  {:else}
    <div class="workspace" class:detail-open={detailOpen}>
      <section class="tracks" aria-label="Track list">
        <div class="list-head">
          <input
            type="checkbox"
            aria-label={selectedCount === totalCount ? 'Deselect all tracks' : 'Select all tracks'}
            title="Tick every track in this tab (Ctrl+A)"
            checked={selectedCount === totalCount && totalCount > 0}
            indeterminate={selectedCount > 0 && selectedCount < totalCount}
            onchange={selectAll}
          />
          <span>Track</span>
          <span class="numeric">{totalCount.toLocaleString('en-GB')}</span>
        </div>
        <ul class="list-scroll" aria-busy={loading}>
          {#each items as item (item.track.id)}
            {@const status = rowStatus(item)}
            {@const art = item.artwork.proposed?.artwork ?? item.artwork.artwork}
            <li
              class="row"
              class:active={item.track.id === activeId}
              data-active={item.track.id === activeId}
            >
              <input
                type="checkbox"
                aria-label={'Tick ' + item.track.title}
                checked={isSelected(item.track.id)}
                onchange={() => selectOne(item.track.id)}
              />
              <button
                type="button"
                class="track-name"
                aria-pressed={item.track.id === activeId}
                onclick={() => focusItem(item)}
                title={item.track.location}
              >
                <Thumb seed={item.track.id} size="sm" src={art ? artworkUrl(art) : null} alt="" />
                <span class="names">
                  <span class="title">{item.track.title}</span>
                  <span class="artist">{item.track.artist || 'No artist'}</span>
                </span>
                <span class="status" data-stage={item.stage}>
                  <span class="status-main"
                    >{#if status.confidence}<span class="numeric">{status.confidence}</span
                      >{/if}{status.primary}</span
                  >
                  {#if status.secondary}<span class="status-sub">{status.secondary}</span>{/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
        {#if pageCount > 1}
          <nav aria-label="Track pages">
            <Button size="sm" disabled={page === 0} onclick={() => page--}>Previous</Button>
            <span class="numeric"
              >{(page + 1).toLocaleString('en-GB')} / {pageCount.toLocaleString('en-GB')}</span
            >
            <Button size="sm" disabled={page + 1 >= pageCount} onclick={() => page++}>Next</Button>
          </nav>
        {/if}
      </section>

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
          canUndo={lastAction !== null && lastAction.track === item.track.id}
          onsave={(fields, artwork, skipRest) => saveTrack(item, fields, artwork, skipRest)}
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
  .stages {
    min-width: 0;
    overflow-x: auto;
  }
  /* Each tab as wide as its own label, so five tabs fit the narrowest window. */
  .stages :global([role='tablist']) {
    grid-auto-columns: auto;
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
  .progress,
  .ticked,
  .feedback,
  nav {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }
  .progress {
    display: inline-flex;
    gap: var(--space-2);
  }
  select {
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
  input[type='checkbox'] {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    accent-color: var(--color-text);
    display: block;
    margin: 0;
    cursor: pointer;
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
    grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
    min-width: 0;
    min-height: 0;
  }
  .tracks {
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--color-surface);
    border-right: var(--border-width) solid var(--color-border);
  }
  .list-head {
    display: grid;
    grid-template-columns: var(--checkbox-size) minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
    height: var(--table-header-height);
    padding-inline: var(--space-4);
    color: var(--color-text-2);
    font-size: var(--text-sm);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .list-head .numeric {
    color: var(--color-muted);
  }
  .list-scroll {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    position: relative;
    display: grid;
    grid-template-columns: var(--checkbox-size) minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
    padding-left: var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
    transition: background-color var(--duration-fast) var(--ease-out);
  }
  .row:hover {
    background: var(--color-surface-2);
  }
  .row.active {
    background: var(--color-raised);
  }
  .row.active::before {
    content: '';
    position: absolute;
    inset: 0 auto 0 0;
    width: 2px;
    background: var(--color-accent);
  }
  .track-name {
    min-width: 0;
    display: grid;
    grid-template-columns: var(--thumb-sm) minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4) var(--space-2) 0;
    font: inherit;
    text-align: left;
    color: inherit;
    background: transparent;
    border: 0;
    cursor: pointer;
  }
  .track-name:focus-visible {
    outline: var(--focus-ring-width) solid var(--color-text);
    outline-offset: -2px;
  }
  .names,
  .status {
    min-width: 0;
    display: grid;
    gap: var(--space-1);
  }
  .title,
  .artist {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title {
    font-size: var(--text-md);
  }
  .artist,
  .status {
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .status {
    justify-items: end;
    text-align: right;
  }
  .status-main {
    display: inline-flex;
    gap: var(--space-2);
    white-space: nowrap;
  }
  .status-main .numeric {
    color: var(--color-text);
  }
  .status[data-stage='review'] .status-main {
    color: var(--color-text);
  }
  .status[data-stage='not_searched'],
  .status[data-stage='done'] {
    color: var(--color-muted);
  }
  .status-sub {
    color: var(--color-muted);
    white-space: nowrap;
  }
  nav {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-top: var(--border-width) solid var(--color-border);
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
    .workspace.detail-open > .tracks {
      display: none;
    }
    .tracks {
      border-right: 0;
    }
  }
  @media (forced-colors: active) {
    .row.active::before {
      background: Highlight;
    }
  }
</style>
