<script lang="ts">
  import Button from '../design/Button.svelte';
  import type { CopyFile } from '../ipc/bindings/CopyFile';
  import type { TrackCopies } from '../ipc/bindings/TrackCopies';
  import { audioSummary, duration } from './format';
  import { copyGroup } from './copies';

  interface Props {
    copies: TrackCopies;
    /** Keeps one copy and moves the others to the Recycle Bin. Without it the list only shows. */
    onkeep?: (assetId: number) => void;
    busy?: boolean;
  }

  let { copies, onkeep, busy = false }: Props = $props();

  const group = $derived(copyGroup(copies));
  let confirming = $state<number | null>(null);

  $effect(() => {
    if (confirming !== null && !group.some((file) => file.asset_id === confirming)) {
      confirming = null;
    }
  });

  function facts(file: CopyFile): string {
    const parts = [audioSummary(file.format, file.audio)];
    if (file.audio.duration_ms) parts.push(duration(file.audio.duration_ms));
    return parts.join(' · ');
  }

  function question(others: number): string {
    const count = others.toLocaleString('en-GB');
    return `Keep this copy and move ${count} ${others === 1 ? 'other' : 'others'} to the Recycle Bin?`;
  }

  function relation(file: CopyFile): string {
    if (file.similarity === null) return file.relation;
    return `${file.relation} · ${Math.round(file.similarity * 100)}% match`;
  }
</script>

{#if group.length > 1}
  <section class="block" aria-labelledby="copies-heading">
    <h4 id="copies-heading">
      Copies <span class="count numeric">{group.length.toLocaleString('en-GB')}</span>
    </h4>
    <ul class="files">
      {#each group as file (file.asset_id)}
        {@const best = file.asset_id === copies.keep_asset_id}
        <li class="file">
          <span class="head">
            <span class="relation">{relation(file)}</span>
            {#if best}<span class="note">Best copy</span>{/if}
            {#if file.flagged}<span class="note">Quality flag</span>{/if}
            {#if file.missing}<span class="note">Missing</span>{/if}
          </span>
          <span class="facts numeric">{facts(file)}</span>
          <span class="location" title={file.location}>{file.location}</span>
          {#if onkeep && !file.missing}
            {#if confirming === file.asset_id}
              <span class="confirm" role="group" aria-label="Confirm keeping this copy">
                <span class="question">{question(group.length - 1)}</span>
                <span class="actions">
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={busy}
                    onclick={() => {
                      confirming = null;
                      onkeep(file.asset_id);
                    }}>Move to the bin</Button
                  >
                  <Button size="sm" onclick={() => (confirming = null)}>Cancel</Button>
                </span>
              </span>
            {:else}
              <span class="actions">
                <Button
                  size="sm"
                  variant={best ? 'default' : 'quiet'}
                  disabled={busy}
                  onclick={() => (confirming = file.asset_id)}>Keep this copy</Button
                >
              </span>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if copies.versions.length}
  <section class="block" aria-labelledby="versions-heading">
    <h4 id="versions-heading">
      Versions <span class="count numeric">{copies.versions.length.toLocaleString('en-GB')}</span>
    </h4>
    <ul class="files">
      {#each copies.versions as file (file.asset_id)}
        <li class="file">
          <span class="head">
            <span class="title" title={file.title}>{file.title}</span>
            <span class="note">{file.relation}</span>
          </span>
          <span class="facts numeric">{facts(file)}</span>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .block {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  h4 {
    margin: 0;
    color: var(--color-text);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .count {
    margin-left: var(--space-1);
    color: var(--color-muted);
    font-weight: var(--weight-regular);
  }

  .files {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .file {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-2);
    min-width: 0;
  }

  .relation,
  .title {
    color: var(--color-text);
    font-size: var(--text-sm);
  }

  .title {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .note,
  .facts,
  .location,
  .question {
    color: var(--color-text-2);
    font-size: var(--text-xs);
  }

  .location {
    min-width: 0;
    overflow: hidden;
    color: var(--color-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .confirm {
    display: grid;
    gap: var(--space-2);
    padding-top: var(--space-1);
  }

  .question {
    line-height: var(--leading-normal);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding-top: var(--space-1);
  }
</style>
