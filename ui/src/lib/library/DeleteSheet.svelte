<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { DeleteReport } from '../ipc/bindings/DeleteReport';

  export interface DeleteClient {
    deleteTracks: (trackIds: number[], forGood: boolean) => Promise<DeleteReport>;
  }

  interface Props {
    open: boolean;
    trackIds: number[];
    client: DeleteClient;
    ondeleted?: () => void;
  }

  let { open = $bindable(false), trackIds, client, ondeleted }: Props = $props();

  let working = $state(false);
  let noBin = $state<string | null>(null);
  let failure = $state<string | null>(null);

  const tracks = $derived(
    trackIds.length === 1 ? '1 track' : `${trackIds.length.toLocaleString('en-GB')} tracks`,
  );

  $effect(() => {
    if (!open) return;
    untrack(() => {
      noBin = null;
      failure = null;
    });
  });

  async function remove(forGood: boolean) {
    if (working || trackIds.length === 0) return;
    working = true;
    failure = null;
    try {
      const report = await client.deleteTracks(trackIds, forGood);
      if (report.no_bin !== null) {
        noBin = report.no_bin;
        return;
      }
      open = false;
      ondeleted?.();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      working = false;
    }
  }
</script>

<Sheet bind:open title={noBin === null ? `Move ${tracks} to the bin` : 'Delete for good'}>
  <div class="delete-body">
    {#if noBin === null}
      <p class="note">
        Their files go to the Recycle Bin, or the Trash, and leave the library. Collections,
        favourites and tags are kept, and History can put everything back.
      </p>
    {:else}
      <p class="note" role="alert">{noBin}</p>
      <p class="note">
        Nothing was moved. These files can only be deleted for good, and that cannot be undone.
      </p>
    {/if}
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
  </div>

  {#snippet actions()}
    <Button variant="quiet" disabled={working} onclick={() => (open = false)}>Cancel</Button>
    {#if noBin === null}
      <Button variant="primary" disabled={working} onclick={() => void remove(false)}>
        {working ? 'Moving…' : 'Move to bin'}
      </Button>
    {:else}
      <Button variant="danger" disabled={working} onclick={() => void remove(true)}>
        {working ? 'Deleting…' : 'Delete for good'}
      </Button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .delete-body {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .note {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }
</style>
