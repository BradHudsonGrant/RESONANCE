<script lang="ts">
  import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
  import { formatProviderName } from './MatchEvidenceHeader.svelte';

  let {
    candidates,
    selected,
    busy,
    onselect,
  }: {
    candidates: MatchCandidateSummary[];
    selected: number;
    busy: boolean;
    onselect: (index: number) => void;
  } = $props();

  function matchLabel(candidate: MatchCandidateSummary) {
    const release = [
      candidate.artist,
      candidate.title,
      candidate.mix,
      candidate.album,
      candidate.catno,
    ]
      .filter(Boolean)
      .join(' · ');
    return [
      release,
      formatProviderName(candidate.provider),
      Math.round(candidate.confidence * 100) + '%',
    ]
      .filter(Boolean)
      .join(' · ');
  }
</script>

{#if candidates.length > 1}
  <label class="match">
    Match
    <select
      aria-label="Choose match"
      value={selected}
      disabled={busy}
      onchange={(event) => onselect(Number(event.currentTarget.value))}
    >
      {#each candidates as candidate (candidate.index)}
        <option value={candidate.index}>{matchLabel(candidate)}</option>
      {/each}
    </select>
  </label>
{:else if candidates[0]}
  <p title={matchLabel(candidates[0])}>{matchLabel(candidates[0])}</p>
{/if}

<style>
  .match {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .match,
  p {
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  select {
    min-width: 0;
    max-width: 100%;
    flex: 1;
    height: var(--control-height-sm);
    padding-inline: var(--space-2-5);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    background: var(--color-surface-2);
    color: var(--color-text);
    font: inherit;
  }
  p {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
