<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { AppliedExternalChanges } from '../ipc/bindings/AppliedExternalChanges';
  import type { ExternalChangePreview } from '../ipc/bindings/ExternalChangePreview';

  export interface PlaylistChangesClient {
    changes: () => Promise<ExternalChangePreview[]>;
    apply: (libraryId: number) => Promise<AppliedExternalChanges>;
  }

  interface Props {
    open: boolean;
    client: PlaylistChangesClient;
    /** After a playlist is written, so the count in the sidebar follows. */
    onapplied?: () => void;
  }

  let { open = $bindable(false), client, onapplied }: Props = $props();

  let previews = $state<ExternalChangePreview[]>([]);
  let loading = $state(false);
  let applying = $state<number | null>(null);
  let outcome = $state<string | null>(null);
  let failure = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      previews = await client.changes();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (!open) return;
    outcome = null;
    failure = null;
    void load();
  });

  function lines(value: number): string {
    return value === 1 ? '1 line' : `${value.toLocaleString('en-GB')} lines`;
  }

  function files(value: number): string {
    return value === 1 ? '1 file' : `${value.toLocaleString('en-GB')} files`;
  }

  function countsLine(preview: ExternalChangePreview): string {
    const parts = [`${files(preview.changes)} moved`, `${lines(preview.edits.length)} to change`];
    if (preview.not_found > 0) parts.push(`${files(preview.not_found)} no longer in the playlist`);
    return parts.join(', ');
  }

  async function apply(preview: ExternalChangePreview) {
    if (applying !== null) return;
    applying = preview.library_id;
    failure = null;
    outcome = null;
    try {
      const applied = await client.apply(preview.library_id);
      outcome =
        applied.edits > 0
          ? `Updated ${preview.name}: ${lines(applied.edits)} changed, backed up first`
          : `${preview.name} already names every file`;
      onapplied?.();
      await load();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      applying = null;
    }
  }
</script>

<Sheet bind:open title="Update playlists">
  <div class="changes-body">
    <p class="note">
      Renames and replaces moved files these playlists name. Each playlist is copied to the backups
      folder in the data folder before it is written, then read back to check it.
    </p>
    {#if previews.length === 0 && !loading}
      <p class="note">Every playlist names its files where they are.</p>
    {/if}
    {#each previews as preview (preview.library_id)}
      <ModularDivider />
      <section class="playlist" aria-label={preview.name}>
        <div class="playlist-head">
          <div class="playlist-text">
            <h3 class="playlist-name">{preview.name}</h3>
            <span class="playlist-path">{preview.location}</span>
          </div>
          <Button
            size="sm"
            disabled={applying !== null || preview.problem !== null}
            onclick={() => void apply(preview)}
          >
            {applying === preview.library_id ? 'Updating…' : `Update ${preview.name}`}
          </Button>
        </div>
        {#if preview.problem}
          <p class="error" role="alert">{preview.problem}</p>
        {:else}
          <p class="counts">{countsLine(preview)}</p>
          {#if preview.edits.length > 0}
            <ol class="edits" aria-label="Changes to {preview.name}">
              {#each preview.edits as edit (edit.line)}
                <li class="edit">
                  <span class="line numeric">{edit.line}</span>
                  <span class="before">{edit.before}</span>
                  <span class="after">{edit.after}</span>
                </li>
              {/each}
            </ol>
          {/if}
        {/if}
      </section>
    {/each}
    {#if outcome}
      <p class="outcome" role="status">{outcome}</p>
    {/if}
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
  </div>

  {#snippet actions()}
    <Button variant="quiet" disabled={applying !== null} onclick={() => (open = false)}
      >Close</Button
    >
  {/snippet}
</Sheet>

<style>
  .changes-body {
    display: grid;
    gap: var(--space-3-5);
    min-width: 0;
  }

  .note,
  .counts,
  .outcome {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .outcome {
    color: var(--color-text);
  }

  .playlist {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  .playlist-head {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .playlist-text {
    display: grid;
    gap: var(--space-0-5);
    min-width: 0;
  }

  .playlist-name {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-base);
    font-weight: var(--weight-medium);
  }

  .playlist-path {
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .edits {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .edit {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    column-gap: var(--space-3);
    row-gap: var(--space-0-5);
    min-width: 0;
    font-size: var(--text-sm);
  }

  .line {
    grid-row: span 2;
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .before,
  .after {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .before {
    color: var(--color-muted);
  }

  .after {
    color: var(--color-text);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }
</style>
