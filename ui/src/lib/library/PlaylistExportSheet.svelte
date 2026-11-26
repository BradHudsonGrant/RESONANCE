<script lang="ts">
  import Button from '../design/Button.svelte';
  import Sheet from '../design/Sheet.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { ExportedPlaylist } from '../ipc/bindings/ExportedPlaylist';
  import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';

  export interface PlaylistExportClient {
    exportCollection: (collectionId: number, path: string) => Promise<ExportedPlaylist>;
    /** Only the desktop app has a native dialog; the browser UI keeps the typed path. */
    pickPlaylistSavePath?: (folder: string | null, name: string) => Promise<string | null>;
  }

  interface Props {
    open: boolean;
    collection: LibraryCollection | null;
    /** Where the file goes unless the user names another place: a folder the library knows. */
    folder: string | null;
    client: PlaylistExportClient;
  }

  let { open = $bindable(false), collection, folder, client }: Props = $props();

  let path = $state('');
  let busy = $state(false);
  let outcome = $state<string | null>(null);
  let failure = $state<string | null>(null);

  const suggestedName = $derived(
    collection ? `${collection.name.replace(/[\\/:*?"<>|]/g, ' ')}.m3u8` : '',
  );

  $effect(() => {
    if (!open || !collection) return;
    outcome = null;
    failure = null;
    const separator = folder?.includes('\\') ? '\\' : '/';
    path = folder ? `${folder.replace(/[\\/]+$/, '')}${separator}${suggestedName}` : suggestedName;
  });

  async function browse() {
    if (busy || !collection || !client.pickPlaylistSavePath) return;
    failure = null;
    try {
      const picked = await client.pickPlaylistSavePath(folder, suggestedName);
      if (picked) {
        path = picked;
        outcome = null;
      }
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    }
  }

  async function save() {
    if (busy || !collection || path.trim() === '') return;
    busy = true;
    failure = null;
    outcome = null;
    try {
      const written = await client.exportCollection(collection.id, path);
      const tracks =
        written.tracks === 1 ? '1 track' : `${written.tracks.toLocaleString('en-GB')} tracks`;
      outcome = `Saved ${tracks} to ${written.path}`;
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }
</script>

<Sheet bind:open title="Export to M3U8">
  <form
    class="export-body"
    id="playlist-export"
    onsubmit={(event) => {
      event.preventDefault();
      void save();
    }}
  >
    <p class="note">
      {collection?.name ?? 'The collection'} is saved as an extended M3U8 playlist naming each file by
      its full path. An existing file is never overwritten.
    </p>
    <div class="path-row">
      <TextInput
        label="Save as"
        bind:value={path}
        disabled={busy}
        spellcheck="false"
        autocomplete="off"
      />
      {#if client.pickPlaylistSavePath}
        <Button disabled={busy} onclick={() => void browse()}>Browse…</Button>
      {/if}
    </div>
    {#if outcome}
      <p class="outcome" role="status">{outcome}</p>
    {/if}
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
  </form>

  {#snippet actions()}
    <Button variant="quiet" disabled={busy} onclick={() => (open = false)}>
      {outcome ? 'Close' : 'Cancel'}
    </Button>
    <Button
      variant="primary"
      type="submit"
      form="playlist-export"
      disabled={busy || path.trim() === '' || outcome !== null}
    >
      {busy ? 'Saving…' : 'Save'}
    </Button>
  {/snippet}
</Sheet>

<style>
  .export-body {
    display: grid;
    gap: var(--space-3-5);
    min-width: 0;
    margin: 0;
  }

  .path-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: end;
    gap: var(--space-2);
    min-width: 0;
  }

  .note,
  .outcome {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
    overflow-wrap: anywhere;
  }

  .outcome {
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
