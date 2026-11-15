<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
  import type { RelinkCandidate } from '../ipc/bindings/RelinkCandidate';
  import type { RelinkFileSummary } from '../ipc/bindings/RelinkFileSummary';
  import type { RelinkMatchEvidence } from '../ipc/bindings/RelinkMatchEvidence';
  import type { RelinkPreview } from '../ipc/bindings/RelinkPreview';
  import { duration, fileSize, sampleRate } from './format';

  export interface RelinkClient {
    candidates: (trackId: number, searchFolder?: string | null) => Promise<RelinkCandidate[]>;
    preview: (trackId: number, candidatePath: string) => Promise<RelinkPreview>;
    apply: (trackId: number, candidatePath: string, force: boolean) => Promise<LibraryTrack>;
    pickAudioFile?: () => Promise<string | null>;
    pickFolder?: () => Promise<string | null>;
  }

  interface Props {
    open: boolean;
    track: LibraryTrack | null;
    client: RelinkClient;
    onrelinked?: (updated: LibraryTrack) => void;
  }

  let { open = $bindable(false), track, client, onrelinked }: Props = $props();

  let candidates = $state<RelinkCandidate[]>([]);
  let loadingCandidates = $state(false);
  let searchFolder = $state('');
  let searchedFolder = $state<string | null>(null);
  let manualPath = $state('');
  let inspecting = $state(false);
  let preview = $state<RelinkPreview | null>(null);
  let confirmMismatch = $state(false);
  let applying = $state(false);
  let errorMsg = $state<string | null>(null);

  let activeSession = 0;

  function evidenceText(evidence: RelinkMatchEvidence): string {
    switch (evidence.kind) {
      case 'exact_bytes':
        return 'Exact file copy';
      case 'exact_audio':
        return 'Identical decoded audio';
      case 'same_recording':
        return `Same recording · ${Math.round(evidence.similarity * 100)}% match`;
      case 'different_recording':
        return evidence.similarity != null
          ? `Different audio · ${Math.round(evidence.similarity * 100)}% match`
          : 'Different audio';
    }
  }

  function summaryFacts(summary: RelinkFileSummary): string {
    const parts = [summary.format.toUpperCase()];
    if (summary.sample_rate) parts.push(sampleRate(summary.sample_rate));
    if (summary.bit_depth) parts.push(`${summary.bit_depth}-bit`);
    else if (summary.bitrate_kbps) parts.push(`${summary.bitrate_kbps} kbps`);
    const len = duration(summary.duration_ms);
    if (len) parts.push(len);
    parts.push(fileSize(summary.size_bytes));
    return parts.join(' · ');
  }

  async function loadCandidates(trackId: number, folder: string | null, session: number) {
    loadingCandidates = true;
    errorMsg = null;
    try {
      const found = await client.candidates(trackId, folder);
      if (session !== activeSession) return;
      candidates = found;
      searchedFolder = folder;
    } catch (failure) {
      if (session !== activeSession) return;
      errorMsg = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (session === activeSession) {
        loadingCandidates = false;
      }
    }
  }

  $effect(() => {
    const currentOpen = open;
    const targetId = track?.id ?? null;
    if (!currentOpen || targetId === null) {
      activeSession += 1;
      untrack(() => {
        candidates = [];
        loadingCandidates = false;
        searchFolder = '';
        searchedFolder = null;
        manualPath = '';
        inspecting = false;
        preview = null;
        confirmMismatch = false;
        applying = false;
        errorMsg = null;
      });
      return;
    }

    const session = ++activeSession;
    untrack(() => {
      candidates = [];
      searchFolder = '';
      searchedFolder = null;
      manualPath = '';
      preview = null;
      confirmMismatch = false;
      errorMsg = null;
      void loadCandidates(targetId, null, session);
    });
  });

  async function submitFolderSearch(event: SubmitEvent) {
    event.preventDefault();
    if (!track || loadingCandidates || applying) return;
    const trimmed = searchFolder.trim();
    if (!trimmed) return;
    await loadCandidates(track.id, trimmed, activeSession);
  }

  async function browseFolder() {
    if (!track || !client.pickFolder || loadingCandidates || applying) return;
    try {
      const picked = await client.pickFolder();
      if (!picked) return;
      searchFolder = picked;
      await loadCandidates(track.id, picked, activeSession);
    } catch (failure) {
      errorMsg = failure instanceof Error ? failure.message : String(failure);
    }
  }

  async function inspectCandidate(path: string) {
    if (!track || inspecting || applying) return;
    const trimmed = path.trim();
    if (!trimmed) return;
    const session = activeSession;
    inspecting = true;
    errorMsg = null;
    confirmMismatch = false;
    try {
      const result = await client.preview(track.id, trimmed);
      if (session !== activeSession) return;
      preview = result;
    } catch (failure) {
      if (session !== activeSession) return;
      preview = null;
      errorMsg = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (session === activeSession) {
        inspecting = false;
      }
    }
  }

  async function submitInspect(event: SubmitEvent) {
    event.preventDefault();
    await inspectCandidate(manualPath);
  }

  async function browseFile() {
    if (!track || !client.pickAudioFile || inspecting || applying) return;
    try {
      const picked = await client.pickAudioFile();
      if (!picked) return;
      manualPath = picked;
      await inspectCandidate(picked);
    } catch (failure) {
      errorMsg = failure instanceof Error ? failure.message : String(failure);
    }
  }

  function onManualPathInput() {
    if (preview && manualPath.trim() !== preview.candidate.path) {
      preview = null;
      confirmMismatch = false;
    }
  }

  async function applyCandidate(candidatePath: string, force: boolean) {
    if (!track || applying) return;
    const session = activeSession;
    applying = true;
    errorMsg = null;
    try {
      const updated = await client.apply(track.id, candidatePath, force);
      if (session !== activeSession) return;
      onrelinked?.(updated);
      open = false;
    } catch (failure) {
      if (session !== activeSession) return;
      errorMsg = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (session === activeSession) {
        applying = false;
      }
    }
  }
</script>

<Sheet bind:open title="Relink missing file">
  {#if track}
    <div class="relink-body">
      <div class="target-summary">
        <div class="target-head">
          <span class="target-title">{track.title}</span>
          <span class="target-artist">{track.artist ?? 'No artist tag'}</span>
        </div>
        <p class="path numeric">{track.location}</p>
      </div>

      <ModularDivider />

      <section class="section" aria-label="Matching copies">
        <div class="section-head">
          <MonoLabel text="Matching copies" tone="secondary" />
          {#if !loadingCandidates && candidates.length > 0}
            <span class="count numeric">
              {candidates.length === 1 ? '1 match' : `${candidates.length} matches`}
            </span>
          {/if}
        </div>

        {#if loadingCandidates}
          <p class="status-note" role="status">Searching for matching audio…</p>
        {:else if candidates.length > 0}
          <ul class="candidate-list">
            {#each candidates as item (item.candidate_path)}
              <li class="candidate-row">
                <div class="candidate-info">
                  <div class="candidate-top">
                    <span class="evidence">{evidenceText(item.evidence)}</span>
                    <span class="facts numeric">{summaryFacts(item.file)}</span>
                  </div>
                  <p class="path numeric">{item.candidate_path}</p>
                </div>
                <Button
                  size="sm"
                  variant="primary"
                  disabled={applying || inspecting}
                  aria-label="Relink to {item.candidate_path}"
                  onclick={() => void applyCandidate(item.candidate_path, false)}
                >
                  Relink
                </Button>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="status-note">
            No matching copies found in your library{searchedFolder ? ' or selected folder' : ''}.
          </p>
        {/if}

        <form class="path-row" onsubmit={submitFolderSearch}>
          <input
            type="text"
            class="path-input numeric"
            aria-label="Folder to search for matching copies"
            placeholder="Search another folder…"
            bind:value={searchFolder}
            disabled={loadingCandidates || applying}
          />
          {#if client.pickFolder}
            <Button
              size="sm"
              disabled={loadingCandidates || applying}
              onclick={() => void browseFolder()}
            >
              Browse folder…
            </Button>
          {/if}
          <Button
            type="submit"
            size="sm"
            disabled={loadingCandidates || applying || !searchFolder.trim()}
          >
            Search folder
          </Button>
        </form>
      </section>

      <ModularDivider />

      <section class="section" aria-label="Locate file">
        <MonoLabel text="Locate file" tone="secondary" />
        <p class="section-copy">
          Choose a replacement audio file directly if it moved outside your watched folders or was
          re-encoded.
        </p>

        <form class="path-row" onsubmit={submitInspect}>
          <input
            type="text"
            class="path-input numeric"
            aria-label="Replacement file path"
            placeholder="Path to replacement audio file"
            bind:value={manualPath}
            oninput={onManualPathInput}
            disabled={inspecting || applying}
          />
          {#if client.pickAudioFile}
            <Button size="sm" disabled={inspecting || applying} onclick={() => void browseFile()}>
              Browse file…
            </Button>
          {/if}
          <Button type="submit" size="sm" disabled={inspecting || applying || !manualPath.trim()}>
            {inspecting ? 'Inspecting…' : 'Inspect'}
          </Button>
        </form>

        {#if preview}
          <div class="preview-panel">
            <div class="preview-evidence">
              <Icon name={preview.requires_confirmation ? 'missing' : 'check'} size="xs" />
              <span>{evidenceText(preview.evidence)}</span>
            </div>

            <div class="comparison" aria-label="Before and after file comparison">
              <div class="compare-card">
                <MonoLabel text="Missing file" size="xs" tone="muted" />
                <span class="facts numeric">{summaryFacts(preview.current)}</span>
                <p class="path numeric">{preview.current.path}</p>
              </div>
              <div class="compare-card">
                <MonoLabel text="Selected file" size="xs" tone="muted" />
                <span class="facts numeric">{summaryFacts(preview.candidate)}</span>
                <p class="path numeric">{preview.candidate.path}</p>
              </div>
            </div>

            {#if preview.merges_existing_track_id != null}
              <p class="merge-note">
                This file is already in your library as a separate entry. Relinking merges it into
                this track and keeps your collections and favourite state.
              </p>
            {/if}

            {#if preview.requires_confirmation}
              <div class="mismatch-warning" role="alert">
                <p class="mismatch-text">
                  The selected file does not match the missing track's audio fingerprint or length.
                  Confirm below if you intend to replace this track's audio.
                </p>
                <label class="confirm-check">
                  <input type="checkbox" bind:checked={confirmMismatch} disabled={applying} />
                  <span>Replace this track with different audio anyway</span>
                </label>
              </div>
            {/if}
          </div>
        {/if}
      </section>

      {#if errorMsg}
        <p class="error" role="alert">{errorMsg}</p>
      {/if}
    </div>
  {/if}

  {#snippet actions()}
    <Button variant="quiet" disabled={applying} onclick={() => (open = false)}>Close</Button>
    {#if preview}
      <Button
        variant="primary"
        disabled={applying || (preview.requires_confirmation && !confirmMismatch)}
        onclick={() =>
          preview && void applyCandidate(preview.candidate.path, preview.requires_confirmation)}
      >
        {applying ? 'Relinking…' : 'Relink selected file'}
      </Button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .relink-body {
    display: grid;
    gap: var(--space-3-5);
  }

  .target-summary {
    display: grid;
    gap: var(--space-1);
  }

  .target-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
  }

  .target-title {
    color: var(--color-text);
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: var(--weight-semibold);
  }

  .target-artist {
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .section {
    display: grid;
    gap: var(--space-2-5);
  }

  .section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .count {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .section-copy,
  .status-note,
  .merge-note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .candidate-list {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .candidate-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2-5) var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .candidate-info {
    display: grid;
    flex: 1;
    gap: var(--space-1);
    min-width: 0;
  }

  .candidate-top {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
  }

  .evidence {
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .facts {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .path {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: var(--leading-normal);
    word-break: break-all;
  }

  .path-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .path-input {
    flex: 1 1 14rem;
    min-width: 0;
    height: var(--control-height-sm);
    padding: 0 var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .path-input:focus-visible {
    border-color: var(--color-text);
  }

  .preview-panel {
    display: grid;
    gap: var(--space-2-5);
    padding: var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .preview-evidence {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1-5);
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .comparison {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2-5);
  }

  @media (width < 560px) {
    .comparison {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .compare-card {
    display: grid;
    align-content: start;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-2-5);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .mismatch-warning {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-2-5) var(--space-3);
    border: var(--border-width) solid var(--color-border-control);
    border-left-width: var(--selection-bar-width);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .mismatch-text {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-xs);
    line-height: var(--leading-normal);
  }

  .confirm-check {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .confirm-check input {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    margin: 0;
    accent-color: var(--color-text);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  @media (forced-colors: active) {
    .candidate-row,
    .preview-panel,
    .compare-card,
    .mismatch-warning {
      border-color: CanvasText;
    }
  }
</style>
