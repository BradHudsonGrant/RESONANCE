<script lang="ts">
  import Button from '../design/Button.svelte';
  import Sheet from '../design/Sheet.svelte';
  import TextInput from '../design/TextInput.svelte';
  import type { ScanReport } from '../ipc/bindings/ScanReport';

  interface Props {
    open: boolean;
    scan: (folder: string) => Promise<ScanReport>;
    onscanned: () => void;
  }

  let { open = $bindable(), scan, onscanned }: Props = $props();
  const formId = $props.id();
  let folder = $state('');
  let running = $state(false);
  let report = $state<ScanReport>();
  let error = $state('');

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (running || !folder.trim()) return;
    running = true;
    report = undefined;
    error = '';
    try {
      report = await scan(folder);
      onscanned();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      running = false;
    }
  }
</script>

<Sheet bind:open title="Scan a music folder">
  <form id={formId} onsubmit={submit}>
    <p class="intro">
      Paste the path to a music folder. Its subfolders are included. Nothing in it is changed.
    </p>
    <TextInput label="Music folder" bind:value={folder} required disabled={running} />
    <div class="feedback" aria-live="polite" role="status">
      {#if running}<p>Reading the folder. You can hide this sheet while it runs.</p>{/if}
      {#if report}
        <p class="numeric">
          {report.found} files: {report.added} added, {report.changed} changed,
          {report.unchanged} unchanged.
        </p>
        {#if report.unreadable_count > 0}
          <p>{report.unreadable_count} files or folders could not be read.</p>
          <ul class="unreadable">
            {#each report.unreadable as file, index (index)}
              <li><span class="numeric">{file.path}</span>: {file.reason}</li>
            {/each}
          </ul>
          {#if report.unreadable_count > report.unreadable.length}
            <p>Showing the first {report.unreadable.length} problems.</p>
          {/if}
        {/if}
      {/if}
    </div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>

  {#snippet actions()}
    <Button variant="quiet" onclick={() => (open = false)}>{running ? 'Hide' : 'Close'}</Button>
    <Button variant="primary" type="submit" form={formId} disabled={running || !folder.trim()}>
      {running ? 'Scanning…' : 'Scan folder'}
    </Button>
  {/snippet}
</Sheet>

<style>
  form {
    display: grid;
    gap: var(--space-3);
  }

  .intro,
  .feedback p,
  .error {
    margin: 0;
  }

  .feedback {
    display: grid;
    gap: var(--space-2);
    font-size: var(--text-sm);
  }

  .error {
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .unreadable {
    max-height: 30vh;
    margin: 0;
    padding-left: var(--space-4);
    overflow: auto;
    overflow-wrap: anywhere;
  }
</style>
