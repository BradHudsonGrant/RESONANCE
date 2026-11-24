<script lang="ts">
  import { tick, untrack } from 'svelte';
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
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
  import type { RenameScope } from '../ipc/bindings/RenameScope';
  import type { TagName } from '../ipc/bindings/TagName';
  import type { TagWriteReport } from '../ipc/bindings/TagWriteReport';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import type { TrackIdentification } from '../ipc/bindings/TrackIdentification';
  import type { UndoReport } from '../ipc/bindings/UndoReport';
  import BatchTagSheet from './BatchTagSheet.svelte';
  import CandidateStrip from './CandidateStrip.svelte';
  import MatchEvidenceHeader, { formatProviderName } from './MatchEvidenceHeader.svelte';
  import WriteTagsSheet from './WriteTagsSheet.svelte';
  import {
    MISSING_FIELDS,
    MISSING_LABELS,
    buildMissingQuery,
    createSelectionSet,
    formatSaveNotice,
    formatUrlLookupNotice,
    formatWriteNotice,
    intersectSelection,
    missingNames,
    pruneSelection,
    resolveActiveCoverIndex,
    uniqueProviders,
    type MissingField,
  } from './prep-view';

  export interface PrepClient {
    trackIds: (query: TrackQuery | null, trackIds: number[] | null) => Promise<number[]>;
    prepPage: (
      query: TrackQuery | null,
      trackIds: number[] | null,
      offset: number,
      limit: number,
    ) => Promise<PrepPageSnapshot>;
    acceptAndWrite: (
      trackIds: number[],
      fields: TagName[] | null,
      includeArtwork: boolean,
      expectedReview?: string,
    ) => Promise<AcceptedTagWriteReport>;
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
  let missing = $state<MissingField | ''>('');
  let totalCount = $state(0);
  let items = $state.raw<PrepItem[]>([]);
  let page = $state(0);
  let activeId = $state<number | null>(null);
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
  let linkLookingUp = $state(false);
  let toolsOpen = $state(false);
  let batchOpen = $state(false);
  let writeOpen = $state(false);
  let toolIds = $state.raw<number[]>([]);
  let url = $state('');
  let refresh = $state(0);
  let chosenCoverByTrack = $state.raw<Record<number, number>>({});
  interface SavedChange {
    operation: number;
    track: number | null;
    field: MissingField | null;
  }
  let history = $state.raw<SavedChange[]>([]);
  const lastAction = $derived(history.at(-1) ?? null);
  let workspace: HTMLElement;
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

  const scopeQuery = $derived(buildMissingQuery(batchQuery, missing));
  const current = $derived(items.find((item) => item.track.id === activeId) ?? null);
  const detailOpen = $derived(compact && reviewing && current !== null);
  const allCount = $derived(totalCount);
  const selectedCount = $derived((void selectionVersion, selected.size));
  const pageCount = $derived(Math.ceil(totalCount / pageSize));
  const searchBusy = $derived(busy || !!activeOnlineJob || selectingAll);
  const showLocalUndo = $derived(
    lastAction !== null && lastAction.track !== null && current?.track.id === lastAction.track,
  );
  const toolsScopeLabel = $derived(
    selectedCount > 0
      ? selectedCount === 1
        ? '1 selected track'
        : `${selectedCount.toLocaleString('en-GB')} selected tracks`
      : current
        ? `Focused track: ${current.track.title}`
        : 'Focused track',
  );
  let scopeKey = '';
  let finishedJob = untrack(() => recentOnlineJob?.id ?? null);
  const contextKey = $derived(JSON.stringify([scopeQuery, batchTrackIds]));

  function markOwnWrite() {
    suppressRevisions++;
    lastOwnWriteAt = Date.now();
  }

  $effect(() => {
    const key = contextKey;
    const query = scopeQuery;
    const scope = batchTrackIds;
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
      .prepPage(query, scope, offset, pageSize)
      .then((result) => {
        if (request !== fetchGeneration) return;
        totalCount = result.total;
        if (result.total > 0 && offset >= result.total) {
          page = Math.max(0, Math.ceil(result.total / pageSize) - 1);
          return;
        }
        items = result.items;
        if (result.total === 0) {
          if (selected.size > 0) {
            selected.clear();
            selectionVersion++;
          }
        } else if (
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
            activeId = edgeItem.track.id;
            keyboardTarget = edgeItem.track.id;
          }
        } else {
          pendingEdge = null;
          pendingShiftSelect = false;
          if (!items.some((item) => item.track.id === activeId)) {
            activeId = items[0]?.track.id ?? null;
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
  $effect(() => {
    void activeId;
    toolsOpen = false;
    url = '';
  });

  function message(err: unknown) {
    return err instanceof Error ? err.message : String(err);
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
  function selectAll() {
    if (!totalCount) return;
    if (selectedCount === totalCount && !selectAllPromise) {
      selected.clear();
      pendingSelectToggles = [];
      selectionVersion++;
      selectingAll = false;
      return;
    }
    if (page === 0 && items.length === totalCount) {
      selected.clear();
      for (const item of items) selected.add(item.track.id);
      pendingSelectToggles = [];
      selectionVersion++;
      return;
    }
    if (selectAllPromise && selectingAll) {
      scopeGeneration++;
      selectingAll = false;
      selectAllPromise = null;
      pendingSelectToggles = [];
      selected.clear();
      selectionVersion++;
      return;
    }
    const gen = scopeGeneration;
    const query = scopeQuery;
    const scope = batchTrackIds;
    pendingSelectToggles = [];
    selectingAll = true;
    const promise = client
      .trackIds(query, scope)
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
  function canResolveSync() {
    return !selectAllPromise && (missing === '' || totalCount <= items.length);
  }
  async function resolveSelectedIds() {
    if (selectAllPromise) await selectAllPromise;
    if (!selected.size) return [];
    if (missing !== '' && totalCount > items.length) {
      const validIds = await client.trackIds(scopeQuery, batchTrackIds);
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
  function trackSubtitle(item: PrepItem) {
    if (!item.online_checked) return item.track.artist;
    const count = item.candidates.length;
    let matches = 'No online match';
    if (count) matches = count.toLocaleString('en-GB') + (count === 1 ? ' match' : ' matches');
    return [item.track.artist, matches].filter(Boolean).join(' · ');
  }

  function focusItem(item: PrepItem) {
    keyboardTarget = null;
    activeId = item.track.id;
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
  function accept(target: number[], field: MissingField | null, reviewed: PrepItem | null) {
    const track = reviewed?.track.id ?? null;
    if (!target.length || reloading || (track !== null && track !== activeId)) return;
    const fields: TagName[] | null =
      field !== null
        ? field === 'artwork'
          ? []
          : [field]
        : reviewed !== null
          ? reviewed.fields.filter((f) => f.changes && !f.locked_by_user).map((f) => f.field)
          : null;
    const includeArtwork =
      field === null ? (reviewed ? !!reviewed.artwork.proposed : true) : field === 'artwork';
    void run(async () => {
      const report = await client.acceptAndWrite(
        target,
        fields,
        includeArtwork,
        reviewed?.review_token,
      );
      failures = report.failed;
      if (report.operation_id !== null) {
        markOwnWrite();
        history = [...history, { operation: report.operation_id, track, field }];
        if (track !== null) clearChosenCover(track);
        else chosenCoverByTrack = {};
      }
      notice = formatSaveNotice(report);
      refresh++;
    });
  }
  function acceptSelected() {
    if (busy || reloading || !!loadError) return;
    if (canResolveSync()) {
      const target = [...selected];
      if (target.length) accept(target, null, null);
      return;
    }
    void resolveSelectedIds().then((target) => {
      if (target.length) accept(target, null, null);
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
        ? 'Searching online'
        : done?.status === 'failed'
          ? (done.error_message ?? 'Search failed')
          : 'Search finished';
      if (!running) markOwnWrite();
      refresh++;
    });
  }
  function search() {
    if (!selectedCount || searchBusy) return;
    if (canResolveSync()) {
      runOnlineSearch([...selected]);
      return;
    }
    void resolveSelectedIds().then(runOnlineSearch);
  }
  function useMatch(item: PrepItem, index: number) {
    if (reloading || activeId !== item.track.id) return;
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
  function lookupLink(item: PrepItem) {
    const link = url.trim();
    if (!link || busy) return;
    linkLookingUp = true;
    void run(async () => {
      try {
        const result = await client.identifyWithUrl(item.track.id, link);
        notice = formatUrlLookupNotice(result);
        if (result.best) {
          url = '';
          toolsOpen = false;
          markOwnWrite();
          refresh++;
        }
      } finally {
        linkLookingUp = false;
      }
    });
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
        activeId = id;
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
    if (event.key.toLowerCase() === 'x' && current) {
      event.preventDefault();
      selectOne(current.track.id);
    } else if (event.shiftKey && event.key.toLowerCase() === 'a' && selectedCount) {
      event.preventDefault();
      void acceptSelected();
    } else if (
      event.key === 'Enter' &&
      current &&
      (!(target instanceof HTMLElement) ||
        target === document.body ||
        target === workspace ||
        target.classList.contains('track-name') ||
        !target.closest('button,a[href],[role="button"],summary,input,select,textarea'))
    ) {
      event.preventDefault();
      if (compact && !detailOpen) focusItem(current);
      else accept([current.track.id], null, current);
    } else if (event.key.toLowerCase() === 'b' || event.key.toLowerCase() === 'e') {
      event.preventDefault();
      void showBatch();
    } else if (event.key.toLowerCase() === 'w') {
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
  <div class="bar" hidden={detailOpen} role="toolbar" aria-label="Prep controls">
    <label class="filter">
      <select
        aria-label="Filter tracks"
        value={missing}
        onchange={(event) => (missing = event.currentTarget.value as MissingField | '')}
      >
        <option value="">All tracks</option>
        {#each MISSING_FIELDS as field (field)}
          <option value={field}>Missing {MISSING_LABELS[field].toLowerCase()}</option>
        {/each}
      </select>
    </label>
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
    <span class="count numeric"
      >{(selectedCount || allCount).toLocaleString('en-GB')}
      {selectedCount ? 'selected' : allCount === 1 ? 'track' : 'tracks'}</span
    >
    <div class="actions">
      <Button
        variant="primary"
        size="sm"
        disabled={!selectedCount || searchBusy || !!loadError}
        title={selectedCount ? 'Search only the selected tracks' : 'Select tracks to search online'}
        onclick={search}
      >
        <Icon name="search" size="xs" />Search online</Button
      >
      {#if selectedCount > 0}
        <Button
          size="sm"
          disabled={busy || reloading || !!loadError || selectingAll}
          onclick={() => void acceptSelected()}
          title="Write all suggestions for the selected tracks (Shift+A)">Accept selected</Button
        >
      {/if}
      {#if lastAction && !showLocalUndo}{@render undoButton()}
      {/if}
    </div>
  </div>

  {#if activeOnlineJob || notice || failures.length}
    <div class="feedback">
      <span role="status">
        {#if activeOnlineJob}
          {activeOnlineJob.paused ? 'Search paused' : 'Searching online'}
          {#if activeOnlineJob.total_items != null}
            <span class="numeric">
              {activeOnlineJob.done_items.toLocaleString('en-GB')} / {activeOnlineJob.total_items.toLocaleString(
                'en-GB',
              )}</span
            >
          {/if}
          {#if notice && notice !== 'Searching online'}
            · {notice}
          {/if}
        {:else}{notice}{/if}
      </span>
      {#if activeOnlineJob && onopenactivity}
        <Button size="sm" onclick={onopenactivity}>Activity</Button>
      {/if}
      {#if failures.length}
        <details>
          <summary
            >{failures.length.toLocaleString('en-GB')}
            {failures.length === 1 ? 'file needs' : 'files need'} attention</summary
          >
          <ul>
            {#each failures as failure (failure.track_id)}
              <li>
                {failure.path} · {failure.reason}
              </li>
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
          : 'No tracks in this view'}
      </h2>
      {#if missing}
        <Button size="sm" onclick={() => (missing = '')}>Show all tracks</Button>
      {:else if onopenlibrary}
        <Button size="sm" onclick={onopenlibrary}>Go to Library</Button>
      {/if}
    </div>
  {:else}
    <div class="workspace" class:detail-open={detailOpen}>
      <section class="tracks" aria-label="Track list">
        <div class="list-scroll" aria-busy={loading}>
          <table class="track-table">
            <thead>
              <tr>
                <th class="check-cell" scope="col">
                  <input
                    type="checkbox"
                    aria-label={selectedCount === allCount
                      ? 'Deselect all tracks'
                      : 'Select all tracks'}
                    checked={selectedCount === allCount && allCount > 0}
                    indeterminate={selectedCount > 0 && selectedCount < allCount}
                    onchange={selectAll}
                  />
                </th>
                <th scope="col">Track</th>
                <th scope="col" class="missing-col">Missing tags</th>
              </tr>
            </thead>
            <tbody>
              {#each items as item (item.track.id)}
                {@const absent = missingNames(item)}
                {@const subtitle = trackSubtitle(item)}
                <tr
                  class:active={item.track.id === activeId}
                  data-active={item.track.id === activeId}
                >
                  <td class="check-cell">
                    <input
                      type="checkbox"
                      aria-label={'Select ' + item.track.title}
                      checked={isSelected(item.track.id)}
                      onchange={() => selectOne(item.track.id)}
                    />
                  </td>
                  <td>
                    <button
                      type="button"
                      class="track-name"
                      aria-pressed={item.track.id === activeId}
                      onclick={() => focusItem(item)}
                      title={item.track.location}
                    >
                      <span>{item.track.title}</span>
                      {#if subtitle}
                        <small>{subtitle}</small>
                      {/if}
                    </button>
                  </td>
                  <td class="missing-text" title={absent || 'No missing tags'}>{absent || '—'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
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

      <section class="review" aria-label="Suggested tags" aria-busy={busy}>
        {#if current}
          {@const item = current}
          {@const savedField = showLocalUndo ? lastAction?.field : null}
          {@const changes = item.fields.filter(
            (field) => (field.changes && !field.locked_by_user) || field.field === savedField,
          )}
          {@const cover = item.artwork.proposed}
          {@const covers = item.candidates.filter((candidate) => candidate.has_artwork)}
          {@const activeCoverIndex = resolveActiveCoverIndex(
            item,
            chosenCoverByTrack[item.track.id],
            covers,
          )}
          {@const writable =
            changes.some((field) => field.changes && !field.locked_by_user) || !!cover}
          {@const sources = uniqueProviders([
            ...changes.filter((field) => field.changes).map((field) => field.provider),
            cover?.provider,
          ])}
          {@const sharedSource = sources.length === 1 ? sources[0]! : null}
          {@const headUndo =
            lastAction !== null &&
            ((showLocalUndo && !savedField) || (detailOpen && !showLocalUndo))}
          <header class="review-head">
            {#if compact}
              <span class="back-to-tracks"
                ><Button size="sm" onclick={backToTracks}>Back to tracks</Button></span
              >
            {/if}
            <h2 title={item.track.title}>{item.track.title}</h2>
            {#if writable || headUndo}
              <div class="actions">
                {#if writable || (showLocalUndo && !savedField)}
                  <Button
                    size="sm"
                    disabled={!writable || busy || reloading}
                    title="Write all suggestions for this track"
                    onclick={() => accept([item.track.id], null, item)}>Accept all</Button
                  >
                {/if}
                {#if headUndo}{@render undoButton()}
                {/if}
              </div>
            {/if}
          </header>
          {#if item.candidates.length}
            <CandidateStrip
              candidates={item.candidates}
              selected={item.selected_candidate}
              busy={busy || reloading}
              onselect={(index) => useMatch(item, index)}
            />
          {/if}
          {#if changes.length || cover || covers.length || savedField === 'artwork'}
            <table class="diff">
              <thead>
                <tr>
                  <th scope="col">Tag</th>
                  <th scope="col">Current</th>
                  <th scope="col"
                    >Suggested{#if sharedSource}<small>{sharedSource}</small>{/if}</th
                  >
                  <th scope="col">
                    <span class="visually-hidden">Action</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {#each changes as field (field.field)}
                  <tr>
                    <th scope="row">{MISSING_LABELS[field.field]}</th>
                    <td>{field.current_value || '—'}</td>
                    <td
                      >{field.proposed_value || '—'}{#if field.provider && !sharedSource}
                        <small>{formatProviderName(field.provider)}</small>
                      {/if}
                    </td>
                    <td>
                      {#if savedField === field.field}{@render undoButton()}
                      {:else}
                        <Button
                          size="sm"
                          disabled={busy || reloading}
                          aria-label={'Accept ' + MISSING_LABELS[field.field]}
                          title={field.field === 'mix' || field.field === 'subgenre'
                            ? 'Save to library'
                            : 'Write ' + MISSING_LABELS[field.field].toLowerCase() + ' to file'}
                          onclick={() => accept([item.track.id], field.field, item)}>Accept</Button
                        >
                      {/if}
                    </td>
                  </tr>
                {/each}
                {#if cover || covers.length || savedField === 'artwork'}
                  <tr>
                    <th scope="row">Artwork</th>
                    <td>
                      {#if item.artwork.artwork}
                        <Thumb
                          seed={item.track.id}
                          size="sm"
                          src={artworkUrl(item.artwork.artwork)}
                          alt="Current cover"
                        />
                      {:else}—{/if}
                    </td>
                    <td>
                      <div class="cover-cell">
                        {#if cover}
                          <Thumb
                            seed={item.track.id}
                            size="sm"
                            src={artworkUrl(cover.artwork)}
                            alt="Suggested cover"
                          />
                          {#if !sharedSource}
                            <small>{formatProviderName(cover.provider)}</small>
                          {/if}
                        {/if}
                        {#if covers.length && (covers.length > 1 || (!cover && savedField !== 'artwork'))}
                          <select
                            aria-label="Choose suggested cover"
                            disabled={busy || reloading}
                            value={activeCoverIndex != null ? String(activeCoverIndex) : ''}
                            onchange={(event) => {
                              const raw = event.currentTarget.value;
                              if (!raw) return;
                              const index = Number(raw);
                              if (Number.isInteger(index)) useCover(item, index);
                            }}
                          >
                            <option value="" disabled>Choose cover</option>
                            {#each covers as candidate (candidate.index)}
                              <option value={String(candidate.index)}
                                >{formatProviderName(
                                  candidate.artwork_provider ?? candidate.provider,
                                )} · {candidate.album ?? candidate.title ?? 'Release'}</option
                              >
                            {/each}
                          </select>
                        {/if}
                      </div>
                    </td>
                    <td>
                      {#if savedField === 'artwork'}{@render undoButton()}
                      {:else if cover}
                        <Button
                          size="sm"
                          disabled={busy || reloading}
                          aria-label="Accept artwork"
                          onclick={() => accept([item.track.id], 'artwork', item)}>Accept</Button
                        >
                      {/if}
                    </td>
                  </tr>
                {/if}
              </tbody>
            </table>
          {:else}
            <p class="hint">
              {item.online_checked
                ? 'No new tags suggested'
                : 'Select tracks and search online for suggestions'}
            </p>
          {/if}
          <MatchEvidenceHeader evidence={item.evidence} steps={item.steps} {onopenproviders} />
          <details class="tools" bind:open={toolsOpen}>
            <summary>Track tools</summary>
            {#if toolsOpen}
              <form
                onsubmit={(event) => {
                  event.preventDefault();
                  lookupLink(item);
                }}
              >
                <label
                  >Release link<input
                    type="url"
                    bind:value={url}
                    placeholder="Paste a release link"
                  />
                </label>
                <Button type="submit" size="sm" disabled={busy || !url.trim()}
                  >{linkLookingUp ? 'Searching…' : 'Find match'}</Button
                >
              </form>
              <div class="actions">
                <Button size="sm" onclick={() => void showBatch()}>Edit tags</Button>
                <Button size="sm" onclick={() => void showWrites()}>Write manual edits</Button>
                {#if onrename}
                  <Button size="sm" onclick={() => void renameTargets()}>Rename files</Button>
                {/if}
              </div>
            {/if}
          </details>
        {:else if loading}
          <p role="status">Loading tracks…</p>
        {/if}
      </section>
    </div>
  {/if}
</section>

{#snippet undoButton()}
  <Button
    size="sm"
    aria-label="Undo save"
    title="Undo this save (Ctrl+Z)"
    disabled={busy}
    onclick={undo}
  >
    <Icon name="undo" size="xs" />
  </Button>
{/snippet}

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
      notice = 'Manual edits are ready to write';
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
        history = [...history, { operation: result.operation_id, track: null, field: null }];
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
  .bar,
  .actions,
  .filter,
  nav,
  .feedback,
  .review-head,
  form {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .bar,
  .feedback {
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .bar {
    column-gap: var(--space-3);
    background: var(--color-surface);
  }
  .filter,
  .count,
  .hint,
  .feedback,
  nav {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }
  .actions {
    flex-wrap: wrap;
  }
  .bar > .actions {
    margin-left: auto;
  }
  select,
  input[type='url'] {
    font: inherit;
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
  .workspace {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 7fr) minmax(0, 6fr);
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
  .list-scroll {
    min-width: 0;
    min-height: 0;
    overflow: auto;
    flex: 1;
  }
  table {
    width: 100%;
    border-spacing: 0;
    table-layout: fixed;
    font-size: var(--text-sm);
    text-align: left;
  }
  td,
  th {
    text-align: left;
    padding: var(--space-1) var(--space-2);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  th {
    font-weight: var(--weight-medium);
  }
  thead th {
    height: var(--table-header-height);
    color: var(--color-text-2);
    background: var(--color-surface);
  }
  .track-table thead th {
    position: sticky;
    top: 0;
    z-index: 1;
  }
  .check-cell {
    width: calc(var(--checkbox-size) + var(--space-3) * 2);
    padding-inline: var(--space-3);
  }
  .missing-col {
    width: 40%;
  }
  .track-table tr.active td {
    background: var(--color-raised);
  }
  .track-table tr.active td:first-child {
    box-shadow: inset var(--selection-bar-width) 0 var(--color-accent);
  }
  .track-name {
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-width: 0;
    width: 100%;
    min-height: var(--control-height-md);
    border: 0;
    background: transparent;
    color: inherit;
    padding: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .track-name span,
  .track-name small,
  .missing-text,
  h2 {
    display: block;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .missing-text {
    display: table-cell;
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }
  small {
    display: block;
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }
  nav {
    justify-content: center;
    padding: var(--space-2);
    border-top: var(--border-width) solid var(--color-border);
  }
  .review {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    padding: var(--space-3) var(--space-4);
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-3);
  }
  .review > * {
    flex-shrink: 0;
  }
  .review-head {
    justify-content: space-between;
  }
  .back-to-tracks {
    flex-shrink: 0;
  }
  h2 {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    min-width: 0;
    margin: 0;
  }
  .review-head h2 {
    flex: 1;
  }
  .review-head .actions {
    flex-shrink: 0;
  }
  .diff th,
  .diff td {
    overflow-wrap: anywhere;
    vertical-align: middle;
  }
  .diff th:first-child {
    width: 18%;
  }
  .diff th:last-child {
    width: calc(var(--space-8) * 2 + var(--space-5));
  }
  .diff th,
  .diff td {
    padding: var(--space-1-5) var(--space-1);
  }
  .diff td:last-child {
    text-align: right;
  }
  .diff small {
    margin-top: var(--space-1);
  }
  .diff select {
    width: 100%;
  }
  .cover-cell {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1-5);
    min-width: 0;
  }
  .tools {
    border-top: var(--border-width) solid var(--color-border);
    padding-top: var(--space-2);
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }
  summary {
    width: fit-content;
    cursor: pointer;
  }
  form,
  .tools > .actions {
    margin-top: var(--space-2);
  }
  form {
    flex-wrap: wrap;
    align-items: flex-end;
  }
  form label {
    display: grid;
    row-gap: var(--space-1);
    flex: 1;
    min-width: 0;
  }
  .empty {
    margin: auto;
    padding: var(--space-4);
    display: grid;
    justify-items: center;
    gap: var(--space-3);
  }
  .empty p,
  .hint {
    margin: 0;
  }
  .feedback ul {
    margin-block: var(--space-2);
    max-block-size: calc(var(--space-8) * 4);
    overflow: auto;
    overflow-wrap: anywhere;
  }
  .scope-label {
    max-width: 14ch;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar[hidden] {
    display: none;
  }
  @container (max-width: 800px) {
    .workspace {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr);
    }
    .tracks {
      border-right: 0;
      border-bottom: var(--border-width) solid var(--color-border-control);
    }
    .workspace:not(.detail-open) .review,
    .workspace.detail-open .tracks {
      display: none;
    }
    .filter select {
      max-inline-size: calc(var(--space-8) * 5);
    }
    .bar {
      padding-inline: var(--space-2);
    }
    .review {
      padding: var(--space-2) var(--space-3);
    }
  }
  @media (forced-colors: active) {
    .track-table tr.active td {
      border-bottom-color: Highlight;
    }
    .track-table tr.active td:first-child {
      border-left: var(--selection-bar-width) solid Highlight;
    }
  }
</style>
