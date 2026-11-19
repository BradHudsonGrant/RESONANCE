<script lang="ts">
  import { untrack } from 'svelte';

  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { JournalOperation } from '../ipc/bindings/JournalOperation';
  import type { RenameReport } from '../ipc/bindings/RenameReport';
  import type { RenameScope } from '../ipc/bindings/RenameScope';
  import type { UndoReport } from '../ipc/bindings/UndoReport';
  import {
    filenameTemplate,
    LivePreview,
    renameSummary,
    type RenamePreviewClient,
  } from './rename-template.svelte';
  import TemplateField from './TemplateField.svelte';

  export interface RenameClient extends RenamePreviewClient {
    apply: (scope: RenameScope, template: string) => Promise<RenameReport>;
    recentOperations: () => Promise<JournalOperation[]>;
    undo: (operationId: number) => Promise<UndoReport>;
  }

  interface Props {
    open: boolean;
    scope: RenameScope;
    /** Where the files come from: "Selected tracks", a collection's name, "Whole library". */
    scopeLabel: string;
    client: RenameClient;
    onrenamed?: () => void;
  }

  let { open = $bindable(false), scope, scopeLabel, client, onrenamed }: Props = $props();

  const rowLimit = 50;
  const live = new LivePreview(() => client, rowLimit);

  let template = $state(filenameTemplate.current);
  let applying = $state(false);
  let outcome = $state<string | null>(null);
  let failure = $state<string | null>(null);
  let lastRename = $state<JournalOperation | null>(null);
  let undoing = $state(false);

  function count(value: number): string {
    return value.toLocaleString('en-GB');
  }

  function files(value: number): string {
    return value === 1 ? '1 file' : `${count(value)} files`;
  }

  async function loadLastRename() {
    try {
      const recent = await client.recentOperations();
      lastRename =
        recent.find((operation) => operation.kind === 'rename_files' && operation.can_undo) ?? null;
    } catch {
      lastRename = null;
    }
  }

  $effect(() => {
    if (!open) return;
    untrack(() => {
      template = filenameTemplate.current;
      outcome = null;
      failure = null;
      void loadLastRename();
    });
  });

  $effect(() => {
    if (open) live.request(scope, template);
  });

  const preview = $derived(live.preview);
  const ready = $derived(live.error ? 0 : (preview?.ready ?? 0));
  const blocked = $derived(
    preview
      ? preview.collision + preview.missing_fields + preview.too_long + preview.file_missing
      : 0,
  );

  const countsLine = $derived.by(() => {
    if (!preview) return '';
    const parts = [`${count(preview.ready)} ready`, `${count(preview.unchanged)} unchanged`];
    if (blocked > 0) parts.push(`${count(blocked)} cannot be renamed`);
    return parts.join(' · ');
  });

  async function apply() {
    if (applying || ready === 0) return;
    applying = true;
    failure = null;
    outcome = null;
    try {
      const report = await client.apply(scope, template);
      outcome = report.queued
        ? `Renaming ${files(preview?.total ?? 0)} in Activity. Undo it here once it has finished.`
        : renameSummary(report);
      onrenamed?.();
      live.request(scope, template);
      await loadLastRename();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      applying = false;
    }
  }

  async function undo() {
    if (!lastRename || undoing) return;
    undoing = true;
    failure = null;
    try {
      const report = await client.undo(lastRename.id);
      outcome = `Undone. ${files(report.restored)} have their old names back.`;
      onrenamed?.();
      live.request(scope, template);
      await loadLastRename();
    } catch (error) {
      failure = error instanceof Error ? error.message : String(error);
    } finally {
      undoing = false;
    }
  }
</script>

<Sheet bind:open title="Rename files">
  <div class="rename-body">
    <div class="scope">
      <MonoLabel text="Scope" tone="secondary" />
      <p class="scope-line">
        <span class="scope-name">{scopeLabel}</span>
        {#if preview}
          <span class="numeric">{files(preview.total)}</span>
        {/if}
      </p>
      <p class="note">
        Each file is renamed from its tags inside its own folder. Nothing changes until you rename,
        and the whole batch can be undone.
      </p>
    </div>

    <ModularDivider />

    <TemplateField
      bind:value={template}
      {preview}
      error={live.error}
      previewLabel="Preview of new file names"
      disabled={applying}
    />

    {#if preview && !live.error}
      <p class="counts numeric" aria-live="polite">
        {countsLine}
      </p>
      {#if blocked > 0}
        <p class="note">
          A name another file already has is never overwritten. Files without the tags the template
          needs keep their names.
        </p>
      {/if}
    {/if}

    {#if outcome}
      <p class="outcome" role="status">{outcome}</p>
    {/if}
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {/if}
  </div>

  {#snippet actions()}
    {#if lastRename}
      <Button variant="quiet" disabled={undoing || applying} onclick={() => void undo()}>
        {undoing ? 'Undoing…' : `Undo: ${lastRename.summary}`}
      </Button>
    {/if}
    <Button variant="quiet" disabled={applying} onclick={() => (open = false)}>Close</Button>
    <Button variant="primary" disabled={applying || ready === 0} onclick={() => void apply()}>
      {applying ? 'Renaming…' : `Rename ${files(ready)}`}
    </Button>
  {/snippet}
</Sheet>

<style>
  .rename-body {
    display: grid;
    gap: var(--space-3-5);
    min-width: 0;
  }

  .scope {
    display: grid;
    gap: var(--space-1);
  }

  .scope-line {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
    margin: 0;
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }

  .scope-name {
    color: var(--color-text);
    font-family: var(--font-display);
    font-size: var(--text-base);
    font-weight: var(--weight-semibold);
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

  .error {
    margin: 0;
    padding-left: var(--space-2-5);
    border-left: var(--selection-bar-width) solid var(--color-accent);
    color: var(--color-text);
    font-size: var(--text-sm);
  }
</style>
