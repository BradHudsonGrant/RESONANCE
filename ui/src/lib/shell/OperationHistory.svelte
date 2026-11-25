<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import type { JournalOperation } from '../ipc/bindings/JournalOperation';
  import type { OperationStateName } from '../ipc/bindings/OperationStateName';
  import type { UndoReport } from '../ipc/bindings/UndoReport';

  export interface HistoryClient {
    recentOperations: () => Promise<JournalOperation[]>;
    undo: (operationId: number) => Promise<UndoReport>;
  }

  interface Props {
    /** Read again each time the sheet opens. */
    open: boolean;
    client: HistoryClient;
    /** Raised by Ctrl+Z: each new value undoes the newest change that can be undone. */
    undoRequest?: number;
    onundone?: () => void;
  }

  let { open, client, undoRequest = 0, onundone }: Props = $props();

  let operations = $state<JournalOperation[] | null>(null);
  let undoing = $state<number | null>(null);
  let failure = $state<string | null>(null);
  // Requests count up from 0, so one already set when Activity first mounts is still to do.
  let handledRequest = 0;
  // Ctrl+Z pressed again while an undo runs (the bin can take seconds) is kept, not dropped.
  let queued = 0;

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

  async function load(): Promise<JournalOperation[]> {
    try {
      operations = await client.recentOperations();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
      operations = [];
    }
    return operations;
  }

  async function undo(operation: JournalOperation) {
    if (undoing !== null) return;
    undoing = operation.id;
    failure = null;
    try {
      await client.undo(operation.id);
      onundone?.();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      undoing = null;
    }
    await load();
    if (queued > 0) {
      queued -= 1;
      await undoNewest();
    }
  }

  async function undoNewest() {
    if (undoing !== null) {
      queued += 1;
      return;
    }
    const latest = (await load()).find((operation) => operation.can_undo);
    if (latest) await undo(latest);
    else failure = 'Nothing to undo';
  }

  $effect(() => {
    if (!open) return;
    const request = undoRequest;
    untrack(() => {
      failure = null;
      if (request !== handledRequest) {
        handledRequest = request;
        void undoNewest();
      } else {
        void load();
      }
    });
  });
</script>

<section class="section">
  <MonoLabel text="Changes to files" tone="secondary" />
  {#if failure}
    <p class="alert" role="alert">{failure}</p>
  {/if}
  {#if operations === null}
    <p class="empty" role="status">Reading the history</p>
  {:else if operations.length === 0}
    <p class="empty">No changes to files yet</p>
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
              size="xs"
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
</section>

<style>
  .section {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  .empty,
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

  .alert {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }
</style>
