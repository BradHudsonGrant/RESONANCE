<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { FoundPlaylist } from '../ipc/bindings/FoundPlaylist';
  import type { PlaylistMirror } from '../ipc/bindings/PlaylistMirror';

  export interface PlaylistImportClient {
    find: () => Promise<FoundPlaylist[]>;
    importPlaylist: (path: string) => Promise<PlaylistMirror>;
  }

  interface Props {
    open: boolean;
    client: PlaylistImportClient;
    onimported?: (mirror: PlaylistMirror) => void;
  }

  let { open = $bindable(false), client, onimported }: Props = $props();

  let path = $state('');
  let found = $state<FoundPlaylist[]>([]);
  let busy = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
    if (!open) return;
    failure = null;
    void client.find().then(
      (list) => (found = list),
      () => (found = []),
    );
  });

  async function importFrom(target: string) {
    if (busy || target.trim() === '') return;
    busy = true;
    failure = null;
    try {
      const mirror = await client.importPlaylist(target);
      path = '';
      open = false;
      onimported?.(mirror);
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
    }
  }
</script>

<Sheet bind:open title="Import playlist">
  <form
    class="import-body"
    id="playlist-import"
    onsubmit={(event) => {
      event.preventDefault();
      void importFrom(path);
    }}
  >
    <p class="note">
      An M3U or M3U8 playlist shows as a collection that follows the file. Resonance never changes
      the file unless you update it after a rename or replace.
    </p>
    <TextInput
      label="Playlist file"
      placeholder="Full path to an .m3u or .m3u8 file"
      bind:value={path}
      disabled={busy}
      spellcheck="false"
      autocomplete="off"
    />
    {#if found.length > 0}
      <ModularDivider />
      <div class="found">
        <MonoLabel text="In your music folders" tone="secondary" />
        <div class="found-list" role="list">
          {#each found as playlist (playlist.path)}
            <div class="found-row" role="listitem">
              <div class="found-text">
                <span class="found-name">{playlist.name}</span>
                <span class="found-path">{playlist.path}</span>
              </div>
              {#if playlist.imported}
                <span class="found-state">Imported</span>
              {:else}
                <Button
                  size="sm"
                  disabled={busy}
                  aria-label="Import {playlist.name}"
                  onclick={() => void importFrom(playlist.path)}
                >
                  Import
                </Button>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
  </form>

  {#snippet actions()}
    <Button variant="quiet" disabled={busy} onclick={() => (open = false)}>Cancel</Button>
    <Button
      variant="primary"
      type="submit"
      form="playlist-import"
      disabled={busy || path.trim() === ''}
    >
      {busy ? 'Importing…' : 'Import'}
    </Button>
  {/snippet}
</Sheet>

<style>
  .import-body {
    display: grid;
    gap: var(--space-3-5);
    min-width: 0;
    margin: 0;
  }

  .note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .found {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  .found-list {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  .found-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .found-text {
    display: grid;
    gap: var(--space-0-5);
    min-width: 0;
  }

  .found-name {
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .found-path {
    overflow: hidden;
    color: var(--color-muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .found-state {
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }
</style>
