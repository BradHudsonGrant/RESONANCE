<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { JournalOperation } from '../ipc/bindings/JournalOperation';
  import type { OperationStateName } from '../ipc/bindings/OperationStateName';
  import type { UndoReport } from '../ipc/bindings/UndoReport';

  export interface HistoryClient {
    recentOperations: () => Promise<JournalOperation[]>;
    undo: (operationId: number) => Promise<UndoReport>;
  }

  interface Props {
    open: boolean;
    client: HistoryClient;
    onundone?: () => void;
  }

  let { open = $bindable(false), client, onundone }: Props = $props();

  let operations = $state<JournalOperation[] | null>(null);
  let undoing = $state<number | null>(null);
  let failure = $state<string | null>(null);

  const states: Record<OperationStateName, string> = {
    pending: 'Waiting',
    executing: 'Running',
    applied: 'Done',
    failed: 'Failed',
    undone: 'Undone',
    rolled_back: 'Put back',
  };

  const when = new Intl.DateTimeFormat('en-GB', {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  });

  async function load() {
    try {
      operations = await client.recentOperations();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
      operations = [];
    }
  }

  $effect(() => {
    if (!open) return;
    untrack(() => {
      failure = null;
      operations = null;
      void load();
    });
  });

  async function undo(operation: JournalOperation) {
    if (undoing !== null) return;
    undoing = operation.id;
    failure = null;
    try {
      await client.undo(operation.id);
      onundone?.();
      await load();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      undoing = null;
    }
  }
</script>

<Sheet bind:open title="History">
  <div class="history-body">
    <p class="note">
      Every change to files on disk, newest first. Undo puts it back, files from the bin included.
    </p>
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
    {#if operations === null}
      <p class="note" role="status">Reading the history</p>
    {:else if operations.length === 0}
      <p class="note">No changes to files yet</p>
    {:else}
      <ul class="operations" aria-label="Changes to files">
        {#each operations as operation (operation.id)}
          <li class="operation">
            <div class="text">
              <span class="summary">{operation.summary}</span>
              <span class="meta">
                <span class="numeric">{when.format(operation.created_ms)}</span>
                <span>{states[operation.state]}</span>
              </span>
              {#if operation.recovery}
                <span class="recovery">{operation.recovery}</span>
              {/if}
            </div>
            {#if operation.can_undo}
              <Button
                size="sm"
                disabled={undoing !== null}
                aria-label={`Undo ${operation.summary}`}
                onclick={() => void undo(operation)}
              >
                {undoing === operation.id ? 'Undoing…' : 'Undo'}
              </Button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  {#snippet actions()}
    <Button variant="quiet" onclick={() => (open = false)}>Close</Button>
  {/snippet}
</Sheet>

<style>
  .history-body {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .note,
  .recovery {
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .operations {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .operation {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    padding: var(--space-2-5) 0;
    border-top: 1px solid var(--color-border);
  }

  .text {
    display: grid;
    flex: 1;
    gap: var(--space-1);
    min-width: 0;
  }

  .summary {
    overflow: hidden;
    color: var(--color-text);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    gap: var(--space-2-5);
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }
</style>
