<script lang="ts">
  import Button from '../design/Button.svelte';
  import ModularDivider from '../design/ModularDivider.svelte';
  import MonoLabel from '../design/MonoLabel.svelte';
  import Sheet from '../design/Sheet.svelte';
  import type { ActivityJob } from '../ipc/bindings/ActivityJob';
  import type { ActivitySnapshot } from '../ipc/bindings/ActivitySnapshot';

  export interface ActivityClient {
    snapshot: () => Promise<ActivitySnapshot>;
    cancel: (job_id: number) => Promise<ActivitySnapshot>;
    pause: (job_id: number) => Promise<ActivitySnapshot>;
    resume: (job_id: number) => Promise<ActivitySnapshot>;
    retry: (job_id: number) => Promise<ActivitySnapshot>;
    clearFinished: () => Promise<ActivitySnapshot>;
  }

  interface Props {
    open: boolean;
    snapshot?: ActivitySnapshot;
    client?: ActivityClient;
    onsnapshot?: (next: ActivitySnapshot) => void;
  }

  let {
    open = $bindable(false),
    snapshot = { active: [], recent: [], mix_backoff_active: false },
    client,
    onsnapshot,
  }: Props = $props();

  let busyJobId = $state<number | null>(null);
  let clearing = $state(false);
  let actionError = $state('');

  $effect(() => {
    if (!open || !client) return;
    actionError = '';
    void client
      .snapshot()
      .then((next) => {
        if (next) onsnapshot?.(next);
      })
      .catch(() => {});
  });

  function activeStatusLabel(job: ActivityJob): string {
    if (job.paused) return 'Paused';
    if (job.paused_for_mix) return 'Paused · Mix live';
    if (job.status === 'running') return 'Running';
    return 'Queued';
  }

  function finishedStatusLabel(job: ActivityJob): string {
    if (job.status === 'completed') return 'Completed';
    if (job.status === 'failed') return 'Failed';
    return 'Cancelled';
  }

  function countReadout(job: ActivityJob): string | null {
    if (job.total_items !== null) {
      return `${job.done_items.toLocaleString()} / ${job.total_items.toLocaleString()}`;
    }
    if (job.done_items > 0) {
      return job.done_items.toLocaleString();
    }
    return null;
  }

  function progressPercent(job: ActivityJob): number {
    if (job.total_items === null || job.total_items <= 0) return 0;
    const clamped = Math.min(Math.max(job.done_items, 0), job.total_items);
    return Math.round((clamped / job.total_items) * 100);
  }

  async function cancelJob(jobId: number) {
    if (!client || busyJobId !== null) return;
    busyJobId = jobId;
    actionError = '';
    try {
      const next = await client.cancel(jobId);
      if (next) onsnapshot?.(next);
    } catch (failure) {
      actionError = failure instanceof Error ? failure.message : String(failure);
      await client
        .snapshot()
        .then((next) => {
          if (next) onsnapshot?.(next);
        })
        .catch(() => {});
    } finally {
      busyJobId = null;
    }
  }

  async function togglePause(job: ActivityJob) {
    if (!client || busyJobId !== null) return;
    busyJobId = job.id;
    actionError = '';
    try {
      const next = job.paused ? await client.resume(job.id) : await client.pause(job.id);
      if (next) onsnapshot?.(next);
    } catch (failure) {
      actionError = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busyJobId = null;
    }
  }

  async function retryJob(jobId: number) {
    if (!client || busyJobId !== null) return;
    busyJobId = jobId;
    actionError = '';
    try {
      const next = await client.retry(jobId);
      if (next) onsnapshot?.(next);
    } catch (failure) {
      actionError = failure instanceof Error ? failure.message : String(failure);
      await client
        .snapshot()
        .then((next) => {
          if (next) onsnapshot?.(next);
        })
        .catch(() => {});
    } finally {
      busyJobId = null;
    }
  }

  async function clearFinished() {
    if (!client || clearing) return;
    clearing = true;
    actionError = '';
    try {
      const next = await client.clearFinished();
      if (next) onsnapshot?.(next);
    } catch (failure) {
      actionError = failure instanceof Error ? failure.message : String(failure);
    } finally {
      clearing = false;
    }
  }
</script>

<Sheet bind:open variant="drawer" title="Activity">
  <div class="activity-content">
    {#if actionError}
      <p class="alert" role="alert">{actionError}</p>
    {/if}

    {#if snapshot.mix_backoff_active && snapshot.active.some((job) => job.paused_for_mix)}
      <p class="backoff-note" role="status">
        Background audio identification is paused while a Mix deck is playing.
      </p>
    {/if}

    <section class="section">
      <div class="section-head">
        <MonoLabel
          text={snapshot.active.length > 0 ? `Active · ${snapshot.active.length}` : 'Active'}
          tone="secondary"
        />
      </div>

      {#if snapshot.active.length === 0}
        <p class="empty">No active jobs.</p>
      {:else}
        <ul class="job-list" aria-label="Active jobs">
          {#each snapshot.active as job (job.id)}
            {@const readout = countReadout(job)}
            {@const percent = progressPercent(job)}
            <li class="job-card" aria-label={job.label}>
              <div class="job-header">
                <span class="job-label">{job.label}</span>
                <div class="job-actions">
                  <Button
                    size="xs"
                    disabled={busyJobId === job.id || !client}
                    aria-label={`${job.paused ? 'Resume' : 'Pause'} ${job.label}`}
                    onclick={() => togglePause(job)}
                  >
                    {job.paused ? 'Resume' : 'Pause'}
                  </Button>
                  <Button
                    size="xs"
                    disabled={busyJobId === job.id || !client}
                    onclick={() => cancelJob(job.id)}
                  >
                    Cancel
                  </Button>
                </div>
              </div>
              <div class="job-meta">
                <span class="status">{activeStatusLabel(job)}</span>
                {#if readout}
                  <span class="counts">{readout}</span>
                {/if}
              </div>
              <div
                class="progress-track"
                role="progressbar"
                aria-label={`${job.label} progress`}
                aria-valuemin={0}
                aria-valuemax={job.total_items ?? 100}
                aria-valuenow={job.total_items !== null
                  ? Math.min(job.done_items, job.total_items)
                  : 0}
              >
                <div class="progress-fill" style:width={`${percent}%`}></div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <ModularDivider />

    <section class="section">
      <div class="section-head">
        <MonoLabel
          text="Recent"
          tone="secondary"
          actionLabel={snapshot.recent.length > 0 && client ? 'Clear finished' : undefined}
          onAction={snapshot.recent.length > 0 && client ? clearFinished : undefined}
        />
      </div>

      {#if snapshot.recent.length === 0}
        <p class="empty">No recent jobs.</p>
      {:else}
        <ul class="job-list" aria-label="Recent jobs">
          {#each snapshot.recent as job (job.id)}
            {@const readout = countReadout(job)}
            <li class="job-card" aria-label={job.label}>
              <div class="job-header">
                <span class="job-label">{job.label}</span>
                {#if job.status === 'failed' || job.status === 'cancelled'}
                  <Button
                    size="xs"
                    disabled={busyJobId === job.id || !client}
                    onclick={() => retryJob(job.id)}
                  >
                    Retry
                  </Button>
                {/if}
              </div>
              <div class="job-meta">
                <span class="status">{finishedStatusLabel(job)}</span>
                {#if readout}
                  <span class="counts">{readout}</span>
                {/if}
              </div>
              {#if job.error_message}
                <p class="job-error">{job.error_message}</p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>

  {#snippet actions()}
    <Button variant="quiet" onclick={() => (open = false)}>Done</Button>
  {/snippet}
</Sheet>

<style>
  .activity-content {
    display: grid;
    gap: var(--space-4);
  }

  .section {
    display: grid;
    gap: var(--space-2);
  }

  .section-head {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .alert,
  .backoff-note,
  .empty,
  .job-error {
    margin: 0;
  }

  .alert,
  .backoff-note {
    padding: var(--space-2) var(--space-2-5);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text);
    font-size: var(--text-xs);
    line-height: var(--leading-compact);
  }

  .empty {
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .job-list {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .job-card {
    display: grid;
    gap: var(--space-1-5);
    min-width: 0;
    padding: var(--space-2-5) var(--space-3);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface-2);
  }

  .job-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    min-width: 0;
  }

  .job-actions {
    display: flex;
    flex: none;
    gap: var(--space-1-5);
  }

  .job-label {
    min-width: 0;
    overflow: hidden;
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-compact);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .job-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    color: var(--color-text-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: var(--leading-mono);
  }

  .progress-track {
    width: 100%;
    height: var(--trait-bar-height);
    overflow: hidden;
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-xs);
    background: var(--color-surface);
  }

  .progress-fill {
    height: 100%;
    background: var(--color-text);
    transition: width var(--duration-fast) var(--ease-out);
  }

  .job-error {
    color: var(--color-text);
    font-size: var(--text-xs);
    line-height: var(--leading-compact);
    overflow-wrap: anywhere;
  }

  @media (forced-colors: active) {
    .progress-fill {
      background: Highlight;
      forced-color-adjust: none;
    }
  }
</style>
