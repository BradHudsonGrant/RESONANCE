<script lang="ts">
  import { tick, untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import DataTag from '../design/DataTag.svelte';
  import FilterPill from '../design/FilterPill.svelte';
  import FocusBrackets from '../design/FocusBrackets.svelte';
  import GridDots from '../design/GridDots.svelte';
  import Icon from '../design/Icon.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Provenance from '../design/Provenance.svelte';
  import Thumb from '../design/Thumb.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { AcceptedProposals } from '../ipc/bindings/AcceptedProposals';
  import type { ActivitySnapshot } from '../ipc/bindings/ActivitySnapshot';
  import type { BatchTagTool } from '../ipc/bindings/BatchTagTool';
  import type { FileTagWrite } from '../ipc/bindings/FileTagWrite';
  import type { JournalOperation } from '../ipc/bindings/JournalOperation';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { PrepItem } from '../ipc/bindings/PrepItem';
  import type { PrepQueueSnapshot } from '../ipc/bindings/PrepQueueSnapshot';
  import type { RenameScope } from '../ipc/bindings/RenameScope';
  import type { StagedField } from '../ipc/bindings/StagedField';
  import type { TagName } from '../ipc/bindings/TagName';
  import type { TagWriteReport } from '../ipc/bindings/TagWriteReport';
  import type { TrackArtwork } from '../ipc/bindings/TrackArtwork';
  import type { TrackIdentification } from '../ipc/bindings/TrackIdentification';
  import type { TrackQuery } from '../ipc/bindings/TrackQuery';
  import type { UndoReport } from '../ipc/bindings/UndoReport';
  import type { UserTagEdit } from '../ipc/bindings/UserTagEdit';
  import type { UserTagReport } from '../ipc/bindings/UserTagReport';
  import BatchTagSheet, { type BatchToolTab } from './BatchTagSheet.svelte';
  import WriteTagsSheet from './WriteTagsSheet.svelte';

  export interface PrepClient {
    queue: (query: TrackQuery | null, proposalsOnly: boolean) => Promise<PrepQueueSnapshot>;
    diff: (trackId: number) => Promise<{ track_id: number; fields: StagedField[] }>;
    artwork: (trackId: number) => Promise<TrackArtwork>;
    acceptItems: (trackIds: number[], fields?: TagName[]) => Promise<AcceptedProposals>;
    skipItems: (trackIds: number[], fields?: TagName[]) => Promise<AcceptedProposals>;
    acceptAboveThreshold: (minConfidence: number) => Promise<AcceptedProposals>;
    setUserTag: (trackIds: number[], edits: UserTagEdit[]) => Promise<UserTagReport>;
    acceptArtwork: (trackId: number) => Promise<TrackArtwork>;
    skipArtwork: (trackId: number) => Promise<TrackArtwork>;
    identifyOnline: (trackIds: number[]) => Promise<ActivitySnapshot>;
    identifyWithUrl: (trackId: number, url: string) => Promise<TrackIdentification>;
    previewTagWrites: (trackIds: number[]) => Promise<FileTagWrite[]>;
    applyTagWrites: (trackIds: number[]) => Promise<TagWriteReport>;
    previewBatchTag: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    applyBatchTag: (trackIds: number[], tool: BatchTagTool) => Promise<AcceptedProposals>;
    undoOperation: (operationId: number) => Promise<UndoReport>;
  }

  interface Props {
    client: PrepClient;
    batchQuery?: TrackQuery | null;
    batchLabel?: string | null;
    revision?: number;
    previewTrackId?: number | null;
    previewPlaying?: boolean;
    onclearbatch?: () => void;
    onopenlibrary?: () => void;
    onrename?: (scope: RenameScope, label: string) => void;
    onpreview?: (track: LibraryTrack) => void;
    onfocustrack?: (track: LibraryTrack) => void;
  }

  let {
    client,
    batchQuery = null,
    batchLabel = null,
    revision = 0,
    previewTrackId = null,
    previewPlaying = false,
    onclearbatch,
    onopenlibrary,
    onrename,
    onpreview,
    onfocustrack,
  }: Props = $props();

  const FIELD_LABELS: Record<TagName, string> = {
    title: 'Title',
    artist: 'Artist',
    mix: 'Mix',
    album: 'Album',
    album_artist: 'Album artist',
    genre: 'Genre',
    subgenre: 'Subgenre',
    label: 'Label',
    catno: 'CatNo',
    year: 'Year',
    bpm: 'BPM',
    key: 'Key',
    track_number: 'Track',
    comment: 'Comment',
    composer: 'Composer',
    remixer: 'Remixer',
  };

  const MONO_FIELDS = new Set<TagName>(['catno', 'year', 'bpm', 'key', 'track_number']);

  interface QueueEntry {
    kind: 'track' | 'release';
    id: string;
    trackIds: number[];
    primary: PrepItem;
    items: PrepItem[];
    title: string;
    subtitle: string;
    pendingFields: number;
    confidence: number;
    audioMatched: boolean;
    provider: string | null;
  }

  let items = $state<PrepItem[]>([]);
  let unwrittenTracks = $state(0);
  let lastOperation = $state<JournalOperation | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let statusNotice = $state<string | null>(null);

  let scopeMode = $state<'proposals' | 'all'>('proposals');
  let groupByRelease = $state(false);
  let thresholdPct = $state(90);

  let selectedIndex = $state(0);
  let checkedTrackIds = $state<number[]>([]);

  let editingField = $state<TagName | null>(null);
  let editValue = $state('');
  let editError = $state<string | null>(null);
  let editInputEl = $state<HTMLInputElement | undefined>();

  let urlOpen = $state(false);
  let urlValue = $state('');
  let urlBusy = $state(false);
  let urlError = $state<string | null>(null);
  let urlInputEl = $state<HTMLInputElement | undefined>();

  let batchOpen = $state(false);
  let batchTab = $state<BatchToolTab>('edit');
  let writeOpen = $state(false);

  let queueListEl = $state<HTMLDivElement | undefined>();

  $effect(() => {
    if (batchQuery != null) {
      untrack(() => {
        scopeMode = 'all';
      });
    }
  });

  const entries = $derived.by<QueueEntry[]>(() => {
    if (!groupByRelease) {
      return items.map((item) => ({
        kind: 'track',
        id: `track-${item.track.id}`,
        trackIds: [item.track.id],
        primary: item,
        items: [item],
        title: item.track.title,
        subtitle: item.track.artist ?? item.track.location,
        pendingFields: item.pending_fields,
        confidence: item.confidence,
        audioMatched: item.audio_matched,
        provider: item.provider,
      }));
    }

    const groups: Record<string, PrepItem[]> = {};
    const out: QueueEntry[] = [];

    for (const item of items) {
      const key = item.release?.key;
      if (!key) {
        out.push({
          kind: 'track',
          id: `track-${item.track.id}`,
          trackIds: [item.track.id],
          primary: item,
          items: [item],
          title: item.track.title,
          subtitle: item.track.artist ?? item.track.location,
          pendingFields: item.pending_fields,
          confidence: item.confidence,
          audioMatched: item.audio_matched,
          provider: item.provider,
        });
        continue;
      }
      const existing = groups[key];
      if (existing) {
        existing.push(item);
      } else {
        groups[key] = [item];
      }
    }

    for (const [key, memberItems] of Object.entries(groups)) {
      if (memberItems.length === 1) {
        const only = memberItems[0]!;
        out.push({
          kind: 'track',
          id: `track-${only.track.id}`,
          trackIds: [only.track.id],
          primary: only,
          items: [only],
          title: only.track.title,
          subtitle: only.track.artist ?? only.track.location,
          pendingFields: only.pending_fields,
          confidence: only.confidence,
          audioMatched: only.audio_matched,
          provider: only.provider,
        });
      } else {
        const first = memberItems[0]!;
        const release = first.release!;
        const pendingFields = memberItems.reduce((sum, i) => sum + i.pending_fields, 0);
        const confidences = memberItems
          .filter((i) => i.pending_fields > 0)
          .map((i) => i.confidence);
        const confidence = confidences.length > 0 ? Math.min(...confidences) : first.confidence;
        const audioMatched = memberItems.every((i) => i.audio_matched);
        out.push({
          kind: 'release',
          id: key,
          trackIds: memberItems.map((i) => i.track.id),
          primary: first,
          items: memberItems,
          title: release.title,
          subtitle:
            release.subtitle ?? `${memberItems.length.toLocaleString('en-GB')} tracks in release`,
          pendingFields,
          confidence,
          audioMatched,
          provider: first.provider,
        });
      }
    }
    return out;
  });

  const activeEntry = $derived<QueueEntry | null>(
    entries[Math.min(selectedIndex, Math.max(0, entries.length - 1))] ?? null,
  );

  const thresholdPreview = $derived.by(() => {
    const minConf = thresholdPct / 100;
    let tracks = 0;
    let fields = 0;
    for (const item of items) {
      const matching = item.changeable_confidences.filter((c) => c >= minConf).length;
      if (matching > 0) {
        tracks += 1;
        fields += matching;
      }
    }
    return { tracks, fields };
  });

  const batchTargetIds = $derived(
    checkedTrackIds.length > 0 ? checkedTrackIds : items.map((item) => item.track.id),
  );

  const batchScopeLabel = $derived(
    checkedTrackIds.length > 0
      ? 'Selected in Prep'
      : (batchLabel ?? (scopeMode === 'proposals' ? 'Tracks in Prep queue' : 'All tracks in Prep')),
  );

  let loadInFlight = false;
  let loadQueued = false;
  let mutationsInFlight = 0;

  async function loadQueue(focusAfter = false) {
    if (mutationsInFlight > 0) {
      loadQueued = true;
      return;
    }
    if (loadInFlight) {
      loadQueued = true;
      return;
    }
    loadInFlight = true;
    try {
      do {
        loadQueued = false;
        if (mutationsInFlight > 0) break;
        const query = batchQuery;
        const proposalsOnly = query == null && scopeMode === 'proposals';
        try {
          const snapshot = await client.queue(query, proposalsOnly);
          if (mutationsInFlight > 0) break;
          items = snapshot.items;
          unwrittenTracks = snapshot.unwritten_tracks;
          lastOperation = snapshot.last_operation;
          loadError = null;
          if (selectedIndex >= entries.length) {
            selectedIndex = Math.max(0, entries.length - 1);
          }
          if (focusAfter) {
            await tick();
            focusQueueIfIdle();
          }
        } catch (err) {
          loadError = err instanceof Error ? err.message : String(err);
        } finally {
          loading = false;
        }
      } while (loadQueued && mutationsInFlight === 0);
    } finally {
      loadInFlight = false;
    }
  }

  function focusQueueIfIdle() {
    const active = document.activeElement;
    if (
      !active ||
      active === document.body ||
      active.closest('.sidebar') ||
      active.closest('.prep')
    ) {
      if (
        !(active instanceof HTMLInputElement) &&
        !(active instanceof HTMLSelectElement) &&
        !document.querySelector('dialog[open]')
      ) {
        queueListEl?.focus();
      }
    }
  }

  $effect(() => {
    void batchQuery;
    void scopeMode;
    void revision;
    untrack(() => {
      void loadQueue(true);
    });
  });

  // Prefetches the next queue item's diff and artwork so stepping through the queue never waits.
  $effect(() => {
    const nextEntry = entries[selectedIndex + 1];
    if (!nextEntry) return;
    const nextTrackId = nextEntry.primary.track.id;
    untrack(() => {
      void client.diff(nextTrackId).catch(() => {});
      void client.artwork(nextTrackId).catch(() => {});
    });
  });

  $effect(() => {
    if (editingField !== null && editInputEl) {
      editInputEl.focus();
      editInputEl.select();
    }
  });

  $effect(() => {
    if (urlOpen && urlInputEl) {
      urlInputEl.focus();
      urlInputEl.select();
    }
  });

  function selectEntry(index: number) {
    if (entries.length === 0) return;
    const clamped = Math.max(0, Math.min(entries.length - 1, index));
    selectedIndex = clamped;
    editingField = null;
    editError = null;
    const target = entries[clamped]?.primary.track;
    if (target) onfocustrack?.(target);
  }

  function toggleChecked(entry: QueueEntry) {
    const allIn = entry.trackIds.every((id) => checkedTrackIds.includes(id));
    if (allIn) {
      checkedTrackIds = checkedTrackIds.filter((id) => !entry.trackIds.includes(id));
    } else {
      const additions = entry.trackIds.filter((id) => !checkedTrackIds.includes(id));
      checkedTrackIds = [...checkedTrackIds, ...additions];
    }
  }

  function selectAllInQueue() {
    checkedTrackIds = items.map((item) => item.track.id);
  }

  function applyLocalDecision(trackIds: number[], action: 'accept' | 'skip', fields?: TagName[]) {
    const targetSet = new Set(trackIds);
    const fieldSet = fields && fields.length > 0 ? new Set(fields) : null;

    if (scopeMode === 'proposals' && !fieldSet && batchQuery == null) {
      items = items.filter((item) => !targetSet.has(item.track.id));
    } else {
      items = items.map((item) => {
        if (!targetSet.has(item.track.id)) return item;
        const nextFields = item.fields.map((field) => {
          if (!field.changes) return field;
          if (fieldSet && !fieldSet.has(field.field)) return field;
          if (!fieldSet && field.locked_by_user && action === 'accept') return field;
          if (action === 'accept') {
            return {
              ...field,
              current_value: field.proposed_value ?? field.current_value,
              current_provenance: 'user' as const,
              status: 'accepted' as const,
              locked_by_user: true,
              changes: false,
            };
          }
          return {
            ...field,
            status: 'skipped' as const,
            changes: false,
          };
        });
        const pendingFields = nextFields.filter((f) => f.changes).length;
        const changeable = nextFields
          .filter((f) => f.changes && !f.locked_by_user)
          .map((f) => f.confidence ?? 0);
        return {
          ...item,
          fields: nextFields,
          pending_fields: pendingFields,
          changeable_confidences: changeable,
        };
      });
    }
    if (selectedIndex >= entries.length) {
      selectedIndex = Math.max(0, entries.length - 1);
    }
  }

  async function acceptEntry(entry: QueueEntry | null = activeEntry, fields?: TagName[]) {
    if (!entry) return;
    const ids = [...entry.trackIds];
    const advancingFullEntry = !fields || fields.length === 0;
    const hadRemoved = scopeMode === 'proposals' && advancingFullEntry && batchQuery == null;

    applyLocalDecision(ids, 'accept', fields);
    if (advancingFullEntry) {
      unwrittenTracks += ids.length;
      if (!hadRemoved && selectedIndex + 1 < entries.length) {
        selectedIndex += 1;
      }
    }

    mutationsInFlight += 1;
    try {
      const report = await client.acceptItems(ids, fields ?? []);
      if (report.fields > 0) {
        statusNotice = `Staged ${report.fields.toLocaleString('en-GB')} ${
          report.fields === 1 ? 'field' : 'fields'
        } across ${report.tracks.toLocaleString('en-GB')} ${
          report.tracks === 1 ? 'track' : 'tracks'
        }`;
      }
    } catch (err) {
      statusNotice = err instanceof Error ? err.message : String(err);
    } finally {
      mutationsInFlight -= 1;
      if (mutationsInFlight === 0) void loadQueue();
    }
  }

  async function skipEntry(entry: QueueEntry | null = activeEntry, fields?: TagName[]) {
    if (!entry) return;
    const ids = [...entry.trackIds];
    const advancingFullEntry = !fields || fields.length === 0;
    const hadRemoved = scopeMode === 'proposals' && advancingFullEntry && batchQuery == null;

    applyLocalDecision(ids, 'skip', fields);
    if (advancingFullEntry && !hadRemoved && selectedIndex + 1 < entries.length) {
      selectedIndex += 1;
    }

    mutationsInFlight += 1;
    try {
      await client.skipItems(ids, fields ?? []);
    } catch (err) {
      statusNotice = err instanceof Error ? err.message : String(err);
    } finally {
      mutationsInFlight -= 1;
      if (mutationsInFlight === 0) void loadQueue();
    }
  }

  async function acceptAllAbove() {
    if (thresholdPreview.fields === 0) return;
    const minConf = thresholdPct / 100;
    try {
      const report = await client.acceptAboveThreshold(minConf);
      statusNotice = `Accepted ${report.fields.toLocaleString('en-GB')} ${
        report.fields === 1 ? 'field' : 'fields'
      } on ${report.tracks.toLocaleString('en-GB')} ${report.tracks === 1 ? 'track' : 'tracks'}`;
      await loadQueue(true);
    } catch (err) {
      statusNotice = err instanceof Error ? err.message : String(err);
    }
  }

  function startInlineEdit(field?: TagName) {
    const entry = activeEntry;
    if (!entry) return;
    const diffFields = entry.primary.fields;
    const target =
      field ??
      diffFields.find((f) => f.changes)?.field ??
      diffFields.find((f) => f.current_value == null)?.field ??
      'title';
    const found = diffFields.find((f) => f.field === target);
    editingField = target;
    editValue = found?.proposed_value ?? found?.current_value ?? '';
    editError = null;
  }

  function stepInlineField(delta: -1 | 1) {
    const entry = activeEntry;
    if (!entry || !editingField) return;
    const list = entry.primary.fields;
    const idx = list.findIndex((f) => f.field === editingField);
    if (idx < 0) return;
    const next = list[(idx + delta + list.length) % list.length];
    if (next) startInlineEdit(next.field);
  }

  async function saveInlineEdit() {
    const entry = activeEntry;
    const field = editingField;
    if (!entry || !field) return;
    const ids = [...entry.trackIds];
    const value = editValue;
    editError = null;
    mutationsInFlight += 1;
    try {
      await client.setUserTag(ids, [{ field, value }]);
      items = items.map((item) => {
        if (!ids.includes(item.track.id)) return item;
        const nextFields = item.fields.map((f) =>
          f.field === field
            ? {
                ...f,
                current_value: value.trim() || null,
                current_provenance: 'user' as const,
                locked_by_user: true,
                changes: false,
              }
            : f,
        );
        return {
          ...item,
          fields: nextFields,
          pending_fields: nextFields.filter((f) => f.changes).length,
        };
      });
      unwrittenTracks = Math.max(1, unwrittenTracks);
      editingField = null;
      statusNotice = `Saved ${FIELD_LABELS[field]}`;
      await tick();
      queueListEl?.focus();
    } catch (err) {
      editError = err instanceof Error ? err.message : String(err);
    } finally {
      mutationsInFlight -= 1;
      if (mutationsInFlight === 0) void loadQueue();
    }
  }

  function cancelInlineEdit() {
    editingField = null;
    editError = null;
    void tick().then(() => queueListEl?.focus());
  }

  async function submitReleaseUrl(event: SubmitEvent) {
    event.preventDefault();
    const entry = activeEntry;
    if (!entry || urlBusy || !urlValue.trim()) return;
    urlBusy = true;
    urlError = null;
    try {
      await client.identifyWithUrl(entry.primary.track.id, urlValue.trim());
      urlOpen = false;
      urlValue = '';
      await loadQueue(true);
    } catch (err) {
      urlError = err instanceof Error ? err.message : String(err);
    } finally {
      urlBusy = false;
    }
  }

  async function undoLastOperation() {
    const op = lastOperation;
    if (!op || !op.can_undo) return;
    try {
      const report = await client.undoOperation(op.id);
      statusNotice = `Undone · ${report.restored.toLocaleString('en-GB')} ${
        report.restored === 1 ? 'file' : 'files'
      } restored`;
      await loadQueue(true);
    } catch (err) {
      statusNotice = err instanceof Error ? err.message : String(err);
    }
  }

  function openBatchTools(tab: BatchToolTab = 'edit') {
    batchTab = tab;
    batchOpen = true;
  }

  function adjustThreshold(delta: number) {
    thresholdPct = Math.max(50, Math.min(100, thresholdPct + delta));
  }

  function onWindowKeyDown(event: KeyboardEvent) {
    if (event.altKey || document.querySelector('dialog[open], [role="menu"]')) return;
    const target = event.target;
    const inInput =
      target instanceof HTMLElement &&
      (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName));

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z' && !inInput) {
      if (lastOperation?.can_undo) {
        event.preventDefault();
        void undoLastOperation();
      }
      return;
    }

    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a' && !inInput) {
      event.preventDefault();
      selectAllInQueue();
      return;
    }

    if (inInput) return;

    if (event.shiftKey && event.key.toLowerCase() === 'a') {
      event.preventDefault();
      void acceptAllAbove();
      return;
    }

    if (event.ctrlKey || event.metaKey) return;

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        if (event.shiftKey && activeEntry) toggleChecked(activeEntry);
        selectEntry(selectedIndex + 1);
        break;
      case 'ArrowUp':
        event.preventDefault();
        if (event.shiftKey && activeEntry) toggleChecked(activeEntry);
        selectEntry(selectedIndex - 1);
        break;
      case 'Home':
        event.preventDefault();
        selectEntry(0);
        break;
      case 'End':
        event.preventDefault();
        selectEntry(entries.length - 1);
        break;
      case 'Enter':
        if (
          !(target instanceof HTMLElement) ||
          !target.closest('button, a[href], [role="button"]')
        ) {
          event.preventDefault();
          void acceptEntry();
        }
        break;
      case 's':
      case 'S':
        event.preventDefault();
        void skipEntry();
        break;
      case 'e':
      case 'E':
        event.preventDefault();
        startInlineEdit();
        break;
      case 'x':
      case 'X':
        if (activeEntry) {
          event.preventDefault();
          toggleChecked(activeEntry);
        }
        break;
      case 'g':
      case 'G':
        event.preventDefault();
        groupByRelease = !groupByRelease;
        selectedIndex = 0;
        break;
      case 'b':
      case 'B':
        event.preventDefault();
        openBatchTools('edit');
        break;
      case 'w':
      case 'W':
        event.preventDefault();
        writeOpen = true;
        break;
      case 'u':
      case 'U':
        event.preventDefault();
        urlOpen = !urlOpen;
        break;
      case '-':
        event.preventDefault();
        adjustThreshold(-5);
        break;
      case '=':
      case '+':
        event.preventDefault();
        adjustThreshold(5);
        break;
      case ' ':
        if (
          activeEntry &&
          (!(target instanceof HTMLElement) ||
            !target.closest('button, [role="button"], input, select'))
        ) {
          event.preventDefault();
          onpreview?.(activeEntry.primary.track);
        }
        break;
    }
  }
</script>

<svelte:window onkeydown={onWindowKeyDown} />

<section class="prep" aria-label="Prep">
  <div class="header">
    <div class="title-row">
      <div class="title-group">
        <h1 class="heading">Prep</h1>
        {#if !loading}
          <span class="count numeric">
            {entries.length.toLocaleString('en-GB')}
            {scopeMode === 'proposals' ? 'pending' : entries.length === 1 ? 'item' : 'items'}
          </span>
          {#if unwrittenTracks > 0}
            <span class="count dirty-count numeric">
              · {unwrittenTracks.toLocaleString('en-GB')} to write
            </span>
          {/if}
        {/if}
      </div>

      <div class="actions">
        <Button
          variant="quiet"
          onclick={() => void client.identifyOnline(items.map((item) => item.track.id))}
        >
          Identify online
        </Button>
        <Button variant="quiet" onclick={() => openBatchTools('edit')}>Batch tools</Button>
        {#if onrename}
          <Button
            variant="quiet"
            onclick={() => onrename({ kind: 'tracks', track_ids: batchTargetIds }, batchScopeLabel)}
          >
            Rename files
          </Button>
        {/if}
        <Button
          variant={unwrittenTracks > 0 ? 'primary' : 'quiet'}
          disabled={unwrittenTracks === 0 && items.length === 0}
          onclick={() => (writeOpen = true)}
        >
          {unwrittenTracks > 0
            ? `Write tags (${unwrittenTracks.toLocaleString('en-GB')})`
            : 'Write tags'}
        </Button>
      </div>
    </div>

    <div class="toolbar-slot">
      <div class="toolbar" role="toolbar" aria-label="Prep controls">
        <FilterPill
          label="Proposals"
          active={scopeMode === 'proposals' && batchQuery == null}
          onclick={() => {
            onclearbatch?.();
            scopeMode = 'proposals';
            selectedIndex = 0;
          }}
        />
        <FilterPill
          label="All"
          active={scopeMode === 'all' || batchQuery != null}
          onclick={() => {
            scopeMode = 'all';
            selectedIndex = 0;
          }}
        />
        <FilterPill
          label="By release"
          active={groupByRelease}
          onclick={() => {
            groupByRelease = !groupByRelease;
            selectedIndex = 0;
          }}
        />

        {#if batchQuery != null && batchLabel}
          <span class="bar-rule" aria-hidden="true"></span>
          <span class="batch-token" role="group" aria-label="Prep batch: {batchLabel}">
            <span class="token-text">{batchLabel}</span>
            <span class="token-count numeric">{items.length.toLocaleString('en-GB')}</span>
            <button
              type="button"
              class="token-remove"
              aria-label="Clear Prep batch {batchLabel}"
              onclick={() => {
                onclearbatch?.();
                scopeMode = 'proposals';
              }}
            >
              <Icon name="close" size={10} />
            </button>
          </span>
        {/if}

        <span class="bar-rule" aria-hidden="true"></span>

        <div class="threshold-group">
          <label class="threshold-label" for="prep-threshold">Threshold</label>
          <input
            id="prep-threshold"
            class="threshold-slider"
            type="range"
            min="50"
            max="100"
            step="5"
            aria-label="Confidence threshold"
            bind:value={thresholdPct}
          />
          <span class="threshold-value numeric">{thresholdPct}%</span>
          <Button
            size="sm"
            disabled={thresholdPreview.fields === 0}
            onclick={() => void acceptAllAbove()}
          >
            {#if thresholdPreview.fields > 0}
              Accept above {thresholdPct}% ({thresholdPreview.tracks.toLocaleString('en-GB')}
              {thresholdPreview.tracks === 1 ? 'track' : 'tracks'} ·
              {thresholdPreview.fields.toLocaleString('en-GB')}
              {thresholdPreview.fields === 1 ? 'field' : 'fields'})
            {:else}
              Accept above {thresholdPct}% (0)
            {/if}
          </Button>
        </div>

        {#if checkedTrackIds.length > 0}
          <span class="bar-rule" aria-hidden="true"></span>
          <span class="selection-readout numeric">{checkedTrackIds.length} ticked</span>
          <Button variant="quiet" size="sm" onclick={() => (checkedTrackIds = [])}>
            Clear ticks
          </Button>
        {/if}

        {#if statusNotice || lastOperation?.can_undo}
          <span class="bar-rule" aria-hidden="true"></span>
          <div class="status-group">
            {#if statusNotice}
              <span class="status-text" role="status">{statusNotice}</span>
            {/if}
            {#if lastOperation?.can_undo}
              <Button variant="quiet" size="sm" onclick={() => void undoLastOperation()}>
                Undo
              </Button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="stage">
    {#if loading}
      <div class="loading" aria-busy="true">
        <p class="visually-hidden" role="status">Reading the Prep queue</p>
        {#each { length: 8 }, row (row)}
          <span class="skeleton" style:width="{45 + ((row * 29) % 40)}%"></span>
        {/each}
      </div>
    {:else if loadError}
      <div class="state" role="alert">
        <h2>Prep could not be loaded</h2>
        <p>{loadError}</p>
        <Button onclick={() => void loadQueue(true)}>Try again</Button>
      </div>
    {:else if entries.length === 0}
      <div class="empty">
        <GridDots spacing={24}>
          <div class="empty-card">
            <h2>Nothing waiting in Prep</h2>
            <p>
              {#if unwrittenTracks > 0}
                All staged proposals are decided. {unwrittenTracks.toLocaleString('en-GB')}
                {unwrittenTracks === 1 ? 'track has' : 'tracks have'} tag changes ready to write to disk.
              {:else}
                Every track in the queue has been reviewed. Show all tracks to edit or run batch
                tools, or return to the library.
              {/if}
            </p>
            <div class="empty-actions">
              {#if unwrittenTracks > 0}
                <Button variant="primary" onclick={() => (writeOpen = true)}>
                  Write tags ({unwrittenTracks.toLocaleString('en-GB')})
                </Button>
              {/if}
              <Button onclick={() => (scopeMode = 'all')}>Show all tracks in Prep</Button>
              {#if onopenlibrary}
                <Button variant="quiet" onclick={onopenlibrary}>Go to Library</Button>
              {/if}
            </div>
          </div>
        </GridDots>
      </div>
    {:else}
      <div class="workspace-split">
        <div
          bind:this={queueListEl}
          class="queue-pane"
          role="listbox"
          tabindex="0"
          aria-label="Prep queue"
          aria-activedescendant={activeEntry ? `prep-entry-${activeEntry.id}` : undefined}
        >
          {#each entries as entry, index (entry.id)}
            {@const isSelected = index === selectedIndex}
            {@const isChecked = entry.trackIds.every((id) => checkedTrackIds.includes(id))}
            {@const thumbHash =
              entry.primary.artwork.proposed?.artwork ??
              entry.primary.artwork.artwork ??
              entry.primary.track.artwork}
            <div
              id="prep-entry-{entry.id}"
              role="option"
              aria-selected={isSelected}
              class={['queue-row', { selected: isSelected, checked: isChecked && !isSelected }]}
              onclick={() => selectEntry(index)}
              onkeydown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  selectEntry(index);
                  void acceptEntry(entry);
                }
              }}
              tabindex="-1"
            >
              <button
                type="button"
                class={['tick-box', { ticked: isChecked }]}
                aria-label="Select {entry.title}"
                aria-pressed={isChecked}
                onclick={(event) => {
                  event.stopPropagation();
                  toggleChecked(entry);
                }}
              >
                <span class={['tick-mark', { shown: isChecked }]}>
                  <Icon name="check" size="xs" />
                </span>
              </button>

              <Thumb
                size="sm"
                src={thumbHash ? artworkUrl(thumbHash) : undefined}
                seed={entry.primary.track.id}
                alt={entry.title}
              />

              <div class="queue-text">
                <span class="queue-title">{entry.title}</span>
                <span class="queue-subtitle">{entry.subtitle}</span>
              </div>

              <div class="queue-meta numeric">
                {#if entry.kind === 'release'}
                  <span class="meta-count">{entry.items.length} tracks</span>
                {/if}
                {#if entry.pendingFields > 0}
                  <span class="meta-fields">+{entry.pendingFields}</span>
                  <span class={['meta-conf', { low: entry.confidence < 0.8 }]}>
                    {Math.round(entry.confidence * 100)}%
                  </span>
                {:else if entry.primary.needs_url}
                  <span class="meta-conf low">URL</span>
                {:else}
                  <span class="meta-conf">Ready</span>
                {/if}
                {#if entry.audioMatched}
                  <span class="audio-tick" title="The audio matches">
                    <Icon name="check" size="xs" />
                  </span>
                {/if}
              </div>
            </div>
          {/each}
        </div>

        {#if activeEntry}
          {@const item = activeEntry.primary}
          {@const currentArt = item.artwork.artwork ?? item.track.artwork}
          {@const proposedArt = item.artwork.proposed}
          {@const isPlaying = previewTrackId === item.track.id && previewPlaying}
          <div class="diff-pane" role="region" aria-label="Staging diff">
            <FocusBrackets active>
              <div class="diff-card">
                <div class="item-header">
                  <div class="item-top">
                    <Thumb
                      size="md"
                      src={proposedArt?.artwork
                        ? artworkUrl(proposedArt.artwork)
                        : currentArt
                          ? artworkUrl(currentArt)
                          : undefined}
                      seed={item.track.id}
                      alt={activeEntry.title}
                    />
                    <div class="item-titles">
                      <h2 class="item-heading">{activeEntry.title}</h2>
                      <p class="item-artist">{activeEntry.subtitle}</p>
                      <p class="item-path numeric">
                        {item.track.format.toUpperCase()}
                        {#if item.track.audio?.sample_rate}
                          · {(item.track.audio.sample_rate / 1000).toFixed(1)} kHz
                        {/if}
                        {#if item.track.audio?.bit_depth}
                          · {item.track.audio.bit_depth}-bit
                        {/if}
                        · {item.track.location}
                      </p>
                    </div>

                    <div class="item-actions">
                      <Button
                        variant={activeEntry.pendingFields > 0 ? 'primary' : 'default'}
                        size="sm"
                        disabled={activeEntry.pendingFields === 0}
                        onclick={() => void acceptEntry(activeEntry)}
                      >
                        Accept (Enter)
                      </Button>
                      <Button
                        size="sm"
                        disabled={activeEntry.pendingFields === 0}
                        onclick={() => void skipEntry(activeEntry)}
                      >
                        Skip (S)
                      </Button>
                      <Button size="sm" onclick={() => startInlineEdit()}>Edit (E)</Button>
                      <Button variant="quiet" size="sm" onclick={() => openBatchTools('release')}>
                        Apply release
                      </Button>
                      {#if onpreview}
                        <Button
                          variant="quiet"
                          size="sm"
                          aria-label={isPlaying ? 'Pause preview' : 'Play preview'}
                          onclick={() => onpreview(item.track)}
                        >
                          <Icon name={isPlaying ? 'pause' : 'play'} size="xs" />
                        </Button>
                      {/if}
                    </div>
                  </div>

                  <div class="evidence-row">
                    {#if activeEntry.provider}
                      <Provenance
                        source="provider"
                        provider={activeEntry.provider}
                        confidence={activeEntry.confidence}
                      />
                    {:else}
                      <Provenance source="file" />
                    {/if}
                    {#if activeEntry.audioMatched}
                      <DataTag label="Audio matches" />
                    {/if}
                    <span class="proposed-summary">
                      {#if activeEntry.kind === 'release'}
                        Release group · {activeEntry.items.length} tracks ·
                        {activeEntry.pendingFields} proposed changes
                      {:else if activeEntry.pendingFields > 0}
                        Proposed action: update {activeEntry.pendingFields}
                        {activeEntry.pendingFields === 1 ? 'field' : 'fields'}
                      {:else if item.needs_url}
                        No confident match · paste a Discogs or Bandcamp URL
                      {:else}
                        All proposals decided · press E to edit any field
                      {/if}
                    </span>
                    <Button variant="quiet" size="xs" onclick={() => (urlOpen = !urlOpen)}>
                      {urlOpen ? 'Hide URL' : 'Paste release URL (U)'}
                    </Button>
                  </div>

                  {#if proposedArt}
                    <div class="cover-proposal">
                      <span class="cover-text">
                        Proposed release cover from {proposedArt.provider}
                      </span>
                      <div class="cover-btns">
                        <Button
                          size="xs"
                          onclick={() =>
                            void client.acceptArtwork(item.track.id).then(() => loadQueue())}
                        >
                          Use cover
                        </Button>
                        <Button
                          variant="quiet"
                          size="xs"
                          onclick={() =>
                            void client.skipArtwork(item.track.id).then(() => loadQueue())}
                        >
                          Skip cover
                        </Button>
                      </div>
                    </div>
                  {/if}

                  {#if urlOpen || item.needs_url}
                    <form class="url-form" onsubmit={submitReleaseUrl}>
                      <input
                        bind:this={urlInputEl}
                        type="url"
                        class="url-input"
                        aria-label="Release URL"
                        placeholder="https://www.discogs.com/release/... or Bandcamp album URL"
                        bind:value={urlValue}
                        disabled={urlBusy}
                      />
                      <Button size="sm" type="submit" disabled={urlBusy || !urlValue.trim()}>
                        Identify URL
                      </Button>
                      {#if urlError}
                        <p class="inline-error" role="alert">{urlError}</p>
                      {/if}
                    </form>
                  {/if}
                </div>

                <div class="diff-table-wrap">
                  <table class="diff-table" aria-label="Tag field comparison">
                    <thead>
                      <tr>
                        <th scope="col" class="col-field">Field</th>
                        <th scope="col" class="col-current">Current</th>
                        <th scope="col" class="col-proposed">Proposed</th>
                        <th scope="col" class="col-source">Source</th>
                        <th scope="col" class="col-action">Action</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each item.fields as field (field.field)}
                        {@const isMono = MONO_FIELDS.has(field.field)}
                        {@const isEditing = editingField === field.field}
                        <tr class={['diff-row', { changing: field.changes, editing: isEditing }]}>
                          <td class="col-field">
                            <MonoLabel text={FIELD_LABELS[field.field]} tone="secondary" />
                          </td>
                          <td class={['col-current', { numeric: isMono }]}>
                            {#if field.current_value != null}
                              <Provenance
                                source={field.current_provenance === 'user' ? 'user' : 'file'}
                              >
                                <span>{field.current_value}</span>
                              </Provenance>
                            {:else}
                              <span class="empty-val">Empty</span>
                            {/if}
                          </td>
                          <td class={['col-proposed', { numeric: isMono }]}>
                            {#if isEditing}
                              <form
                                class="inline-edit-form"
                                onsubmit={(event) => {
                                  event.preventDefault();
                                  void saveInlineEdit();
                                }}
                              >
                                <input
                                  bind:this={editInputEl}
                                  type="text"
                                  class="inline-edit-input"
                                  aria-label="Edit {FIELD_LABELS[field.field]}"
                                  bind:value={editValue}
                                  onkeydown={(event) => {
                                    if (event.key === 'Escape') {
                                      event.preventDefault();
                                      event.stopPropagation();
                                      cancelInlineEdit();
                                    } else if (event.key === 'ArrowDown') {
                                      event.preventDefault();
                                      event.stopPropagation();
                                      stepInlineField(1);
                                    } else if (event.key === 'ArrowUp') {
                                      event.preventDefault();
                                      event.stopPropagation();
                                      stepInlineField(-1);
                                    }
                                  }}
                                />
                              </form>
                            {:else if field.proposed_value != null}
                              <span class={['proposed-pill', { active: field.changes }]}>
                                {field.proposed_value}
                              </span>
                            {:else}
                              <span class="empty-val">—</span>
                            {/if}
                          </td>
                          <td class="col-source">
                            {#if field.locked_by_user}
                              <span class="locked-tag numeric">USER · LOCKED</span>
                            {:else if field.provider}
                              <Provenance
                                source="provider"
                                provider={field.provider}
                                confidence={field.confidence ?? undefined}
                              />
                            {:else if field.current_provenance}
                              <Provenance
                                source={field.current_provenance === 'user' ? 'user' : 'file'}
                              />
                            {:else}
                              <span class="empty-val">—</span>
                            {/if}
                          </td>
                          <td class="col-action">
                            {#if isEditing}
                              <div class="row-btns">
                                <Button
                                  size="xs"
                                  variant="primary"
                                  onclick={() => void saveInlineEdit()}
                                >
                                  Save
                                </Button>
                                <Button size="xs" variant="quiet" onclick={cancelInlineEdit}>
                                  Cancel
                                </Button>
                              </div>
                            {:else}
                              <div class="row-btns">
                                {#if field.proposed_value != null && (field.changes || field.locked_by_user)}
                                  <Button
                                    size="xs"
                                    onclick={() => void acceptEntry(activeEntry, [field.field])}
                                  >
                                    Accept
                                  </Button>
                                {/if}
                                <Button
                                  size="xs"
                                  variant="quiet"
                                  onclick={() => startInlineEdit(field.field)}
                                >
                                  Edit
                                </Button>
                              </div>
                            {/if}
                          </td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>

                {#if editError}
                  <p class="inline-error padded-error" role="alert">{editError}</p>
                {/if}
              </div>
            </FocusBrackets>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</section>

<BatchTagSheet
  bind:open={batchOpen}
  trackIds={batchTargetIds}
  sourceTrackId={activeEntry?.primary.track.id ?? null}
  sourceTrackTitle={activeEntry?.primary.track.title ?? null}
  scopeLabel={batchScopeLabel}
  initialTab={batchTab}
  client={{
    preview: (ids, tool) => client.previewBatchTag(ids, tool),
    apply: (ids, tool) => client.applyBatchTag(ids, tool),
  }}
  onapplied={(report) => {
    statusNotice = `Updated ${report.fields.toLocaleString('en-GB')} ${
      report.fields === 1 ? 'field' : 'fields'
    } across ${report.tracks.toLocaleString('en-GB')} ${report.tracks === 1 ? 'track' : 'tracks'}`;
    void loadQueue(true);
  }}
/>

<WriteTagsSheet
  bind:open={writeOpen}
  trackIds={checkedTrackIds}
  client={{
    preview: (ids) => client.previewTagWrites(ids),
    apply: (ids) => client.applyTagWrites(ids),
  }}
  onwritten={(report) => {
    statusNotice = `Wrote tags to ${report.written.toLocaleString('en-GB')} ${
      report.written === 1 ? 'file' : 'files'
    }`;
    void loadQueue(true);
  }}
/>

<style>
  .prep {
    container-type: inline-size;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
    max-width: var(--table-max-width);
    min-width: 0;
    min-height: 0;
  }

  .header {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-4);
    min-width: 0;
    padding: var(--space-4) var(--space-5) var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
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

  .dirty-count {
    color: var(--color-text);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    align-self: center;
    gap: var(--space-2);
  }

  .toolbar-slot {
    display: grid;
    min-width: 0;
  }

  .toolbar {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: var(--space-2-5);
    min-width: 0;
    height: var(--control-height-sm);
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .toolbar::-webkit-scrollbar {
    display: none;
  }

  .toolbar > :global(*) {
    flex: 0 0 auto;
  }

  .bar-rule {
    width: var(--border-width);
    height: var(--space-3-5);
    margin: 0 var(--space-1);
    background: var(--color-border);
  }

  .batch-token {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--control-height-sm);
    padding-left: var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    font-size: var(--text-sm);
  }

  .token-text {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .token-count {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .token-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 100%;
    padding: 0;
    border: 0;
    border-left: var(--border-width) solid var(--color-border);
    background: transparent;
    color: var(--color-text-2);
    cursor: pointer;
  }

  .token-remove:hover {
    background: var(--color-raised);
    color: var(--color-text);
  }

  .threshold-group {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .threshold-label {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .threshold-slider {
    width: 84px;
    accent-color: var(--color-text);
    cursor: pointer;
  }

  .threshold-value {
    min-width: 4ch;
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .selection-readout {
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .status-group {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .status-text {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .stage {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .workspace-split {
    display: grid;
    grid-template-columns: clamp(280px, 32%, 380px) minmax(0, 1fr);
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  @container (max-width: 860px) {
    .workspace-split {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: 180px minmax(0, 1fr);
    }

    .queue-pane {
      border-right: 0;
      border-bottom: var(--border-width) solid var(--color-border);
    }
  }

  .queue-pane {
    display: grid;
    align-content: start;
    gap: var(--space-1);
    min-width: 0;
    min-height: 0;
    padding: var(--space-3);
    overflow-y: auto;
    border-right: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }

  .queue-pane:focus-visible {
    outline: var(--focus-ring-width) solid var(--color-border-control);
    outline-offset: var(--focus-ring-offset);
  }

  .queue-row {
    position: relative;
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2-5);
    min-width: 0;
    height: 48px;
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    cursor: pointer;
    transition:
      background-color var(--duration-fast) var(--ease-out),
      border-color var(--duration-fast) var(--ease-out);
  }

  .queue-row:hover {
    background: var(--color-surface-2);
  }

  .queue-row.selected {
    border-color: var(--color-border-control);
    background: var(--color-raised);
  }

  .queue-row.selected::before,
  .queue-row.checked::before {
    position: absolute;
    top: var(--space-1-5);
    bottom: var(--space-1-5);
    left: calc(-1 * var(--border-width));
    width: var(--selection-bar-width);
    content: '';
  }

  .queue-row.selected::before {
    background: var(--color-accent);
  }

  .queue-row.checked {
    background: var(--color-surface-2);
  }

  .queue-row.checked::before {
    background: var(--color-border-control);
  }

  .tick-box {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    padding: 0;
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    background: transparent;
    color: var(--color-background);
    cursor: pointer;
  }

  .tick-box.ticked {
    border-color: var(--color-text);
    background: var(--color-text);
  }

  .tick-mark {
    display: inline-flex;
    opacity: 0;
  }

  .tick-mark.shown {
    opacity: 1;
  }

  .queue-text {
    display: grid;
    min-width: 0;
  }

  .queue-title {
    overflow: hidden;
    color: var(--color-text);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .queue-subtitle {
    overflow: hidden;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .queue-meta {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    font-size: var(--text-xs);
  }

  .meta-count,
  .meta-fields {
    color: var(--color-text-2);
  }

  .meta-conf {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .meta-conf.low {
    color: var(--color-muted);
  }

  .audio-tick {
    display: inline-flex;
    color: var(--color-text);
  }

  .diff-pane {
    min-width: 0;
    min-height: 0;
    padding: var(--space-3);
    overflow-y: auto;
  }

  .diff-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    min-width: 0;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
  }

  .item-header {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
    padding: var(--space-4);
    border-bottom: var(--border-width) solid var(--color-border);
  }

  .item-top {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: start;
    gap: var(--space-3-5);
    min-width: 0;
  }

  @container (max-width: 960px) {
    .item-top {
      grid-template-columns: auto minmax(0, 1fr);
    }

    .item-actions {
      grid-column: 1 / -1;
    }
  }

  .item-titles {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .item-heading {
    margin: 0;
    overflow: hidden;
    color: var(--color-text);
    font-family: var(--font-display);
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-snug);
    letter-spacing: var(--tracking-tight);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-artist {
    margin: 0;
    overflow: hidden;
    color: var(--color-text-2);
    font-size: var(--text-base);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-path {
    margin: 0;
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .evidence-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2-5);
  }

  .proposed-summary {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .cover-proposal {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .cover-text {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .cover-btns {
    display: inline-flex;
    gap: var(--space-2);
  }

  .url-form {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .url-input {
    flex: 1;
    min-width: 220px;
    height: var(--control-height);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-md);
  }

  .diff-table-wrap {
    min-width: 0;
    overflow-x: auto;
  }

  .diff-table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  .diff-table th {
    height: var(--table-header-height);
    padding: 0 var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
    color: var(--color-text-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    text-align: left;
  }

  .diff-table td {
    height: var(--row-height);
    padding: 0 var(--space-3);
    border-bottom: var(--border-width) solid var(--color-border);
    color: var(--color-text);
    font-size: var(--text-md);
    vertical-align: middle;
  }

  .diff-row:last-child td {
    border-bottom: 0;
  }

  .diff-row.changing {
    background: var(--color-surface-2);
  }

  .col-field {
    width: 112px;
  }

  .col-current {
    width: 28%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .col-proposed {
    width: 30%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .col-source {
    width: 136px;
  }

  .col-action {
    width: 108px;
  }

  .proposed-pill {
    display: inline-block;
    max-width: 100%;
    overflow: hidden;
    color: var(--color-text-2);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .proposed-pill.active {
    padding: var(--space-0-5) var(--space-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    background: var(--color-raised);
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }

  .empty-val {
    color: var(--color-muted);
  }

  .locked-tag {
    color: var(--color-text-2);
    font-size: var(--text-2xs);
    letter-spacing: var(--tracking-mono-wide);
  }

  .row-btns {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
  }

  .inline-edit-form {
    display: flex;
    width: 100%;
  }

  .inline-edit-input {
    width: 100%;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-xs);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-md);
  }

  .inline-error {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .padded-error {
    padding: var(--space-2) var(--space-3);
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
    padding: var(--space-3) var(--space-5) var(--space-5);
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
    max-width: 34rem;
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

  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .state {
    display: grid;
    justify-items: start;
    gap: var(--space-3);
    padding: var(--space-6) var(--space-5);
  }

  @media (forced-colors: active) {
    .queue-row.selected {
      border-color: Highlight;
    }

    .queue-row.selected::before {
      background: Highlight;
    }

    .queue-row.checked::before {
      background: CanvasText;
    }

    .tick-box.ticked {
      border-color: Highlight;
      background: Highlight;
      color: HighlightText;
      forced-color-adjust: none;
    }
  }
</style>
