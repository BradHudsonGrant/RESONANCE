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
  <form onsubmit={submit}>
    <p>Paste the path to a music folder. Its subfolders are included. Nothing in it is changed.</p>
    <TextInput label="Music folder" bind:value={folder} required disabled={running} />
    <div class="actions">
      <Button variant="primary" type="submit" disabled={running || !folder.trim()}>
        {running ? 'Scanning…' : 'Scan folder'}
      </Button>
      <Button onclick={() => (open = false)}>{running ? 'Hide' : 'Close'}</Button>
    </div>
    <div aria-live="polite" role="status">
      {#if running}<p>Reading the folder. You can hide this sheet while it runs.</p>{/if}
      {#if report}
        <p>
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
    {#if error}<p role="alert">{error}</p>{/if}
  </form>
</Sheet>

<style>
  form {
    display: grid;
    gap: var(--space-3);
  }

  .actions {
    display: flex;
    gap: var(--space-2);
  }

  .unreadable {
    max-height: 30vh;
    overflow: auto;
    overflow-wrap: anywhere;
    padding-left: var(--space-4);
  }
</style>
