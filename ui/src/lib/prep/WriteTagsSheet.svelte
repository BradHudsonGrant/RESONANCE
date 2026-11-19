<script lang="ts">
  import { tick, untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { FileTagWrite } from '../ipc/bindings/FileTagWrite';
  import type { TagName } from '../ipc/bindings/TagName';
  import type { TagWriteReport } from '../ipc/bindings/TagWriteReport';

  export interface WriteTagsClient {
    preview: (trackIds: number[]) => Promise<FileTagWrite[]>;
    apply: (trackIds: number[]) => Promise<TagWriteReport>;
  }

  interface Props {
    open: boolean;
    trackIds?: number[];
    client: WriteTagsClient;
    onwritten?: (report: TagWriteReport) => void;
  }

  let { open = $bindable(false), trackIds = [], client, onwritten }: Props = $props();

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

  let files = $state<FileTagWrite[]>([]);
  let loading = $state(false);
  let applying = $state(false);
  let error = $state<string | null>(null);
  let formEl = $state<HTMLFormElement | undefined>();

  const readyFiles = $derived(files.filter((file) => file.blocked == null));
  const blockedFiles = $derived(files.filter((file) => file.blocked != null));

  function fileCount(count: number): string {
    return count === 1 ? '1 file' : `${count.toLocaleString('en-GB')} files`;
  }

  function shortPath(path: string): string {
    const parts = path.split(/[/\\]/);
    return parts.slice(-2).join('/');
  }

  async function loadPreview(ids: number[]) {
    loading = true;
    error = null;
    try {
      files = await client.preview(ids);
      loading = false;
      await tick();
      const submitBtn = document.querySelector<HTMLButtonElement>('#write-tags-submit');
      submitBtn?.focus();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      files = [];
      loading = false;
    }
  }

  $effect(() => {
    if (!open) return;
    const ids = [...trackIds];
    untrack(() => {
      void loadPreview(ids);
    });
  });

  async function applyWrites() {
    if (applying || loading || readyFiles.length === 0) return;
    applying = true;
    error = null;
    try {
      const report = await client.apply(trackIds);
      onwritten?.(report);
      if (report.failed.length === 0) {
        open = false;
      } else {
        await loadPreview(trackIds);
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      applying = false;
    }
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    await applyWrites();
  }

  function onDialogKeyDown(event: KeyboardEvent) {
    if (!open || event.key !== 'Enter' || loading || applying || readyFiles.length === 0) return;
    const target = event.target;
    if (target instanceof HTMLButtonElement && target.id !== 'write-tags-submit') {
      return;
    }
    event.preventDefault();
    void applyWrites();
  }
</script>

<svelte:window onkeydown={onDialogKeyDown} />

<Sheet bind:open title="Write tags to files">
  <form bind:this={formEl} id="write-tags-form" class="write-body" onsubmit={submit}>
    <div class="summary">
      <MonoLabel text="Pending file writes" tone="secondary" />
      <p class="summary-line numeric" aria-live="polite">
        {#if loading}
          Checking files…
        {:else if files.length === 0}
          No unwritten tag changes waiting on disk
        {:else}
          {fileCount(readyFiles.length)} ready
          {#if blockedFiles.length > 0}
            · {fileCount(blockedFiles.length)} blocked
          {/if}
        {/if}
      </p>
      <p class="note">
        Tags are written into a checked copy in each file's own format (ID3v2.3, Vorbis comments or
        MP4 atoms) and verified by decoded audio hash before replacing the file. The whole batch can
        be undone with Ctrl+Z.
      </p>
    </div>

    <ModularDivider />

    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}

    {#if files.length > 0}
      <div class="file-list" role="list" aria-label="Files to write">
        {#each files.slice(0, 50) as file (file.track_id)}
          <div class="file-row" role="listitem">
            <div class="file-head">
              <span class="file-path">{shortPath(file.path)}</span>
              {#if file.blocked}
                <span class="file-status blocked">{file.blocked}</span>
              {:else}
                <span class="file-status numeric">
                  {file.fields.length}
                  {file.fields.length === 1 ? 'field' : 'fields'}{file.cover ? ' + cover' : ''}
                </span>
              {/if}
            </div>
            {#if file.fields.length > 0}
              <div class="field-changes">
                {#each file.fields as change (change.field)}
                  <span class="change-item">
                    <span class="change-name">{FIELD_LABELS[change.field]}:</span>
                    <span class="change-from">{change.from ?? 'Empty'}</span>
                    <span class="change-arrow" aria-hidden="true">→</span>
                    <span class="change-to">{change.to ?? 'Empty'}</span>
                  </span>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </form>

  {#snippet actions()}
    <Button onclick={() => (open = false)} disabled={applying}>Cancel</Button>
    <Button
      id="write-tags-submit"
      variant="primary"
      type="submit"
      form="write-tags-form"
      disabled={loading || applying || readyFiles.length === 0}
    >
      {readyFiles.length > 0
        ? `Write tags to ${fileCount(readyFiles.length)}`
        : 'Write tags to files'}
    </Button>
  {/snippet}
</Sheet>

<style>
  .write-body {
    display: grid;
    gap: var(--space-3);
  }

  .summary {
    display: grid;
    gap: var(--space-1-5);
  }

  .summary-line {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }

  .note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .error {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .file-list {
    display: grid;
    gap: var(--space-2);
    max-height: 280px;
    padding-right: var(--space-1);
    overflow-y: auto;
  }

  .file-row {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-2-5);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
  }

  .file-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    min-width: 0;
  }

  .file-path {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-status {
    flex-shrink: 0;
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .file-status.blocked {
    color: var(--color-text);
  }

  .field-changes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1-5) var(--space-3);
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .change-item {
    display: inline-flex;
    align-items: baseline;
    gap: var(--space-1);
  }

  .change-name {
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .change-from {
    color: var(--color-text-2);
  }

  .change-to {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }
</style>
