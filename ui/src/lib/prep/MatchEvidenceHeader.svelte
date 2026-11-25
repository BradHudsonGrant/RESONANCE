<script lang="ts" module>
  const PROVIDER_LABELS: Record<string, string> = {
    acoustid: 'AcoustID',
    musicbrainz: 'MusicBrainz',
    discogs: 'Discogs',
    deezer: 'Deezer',
    itunes: 'Apple Music',
    bandcamp: 'Bandcamp',
    soundcloud: 'SoundCloud',
    youtube: 'YouTube',
    audd: 'AudD',
    filename: 'Filename',
    tags: 'File tags',
  };

  import type { MatchBasis } from '../ipc/bindings/MatchBasis';

  export function formatProviderName(id: string): string {
    return PROVIDER_LABELS[id] ?? PROVIDER_LABELS[id.toLowerCase()] ?? id;
  }

  export function formatBasisLabel(basis: MatchBasis): string {
    switch (basis) {
      case 'audio_fingerprint':
        return 'Audio fingerprint';
      case 'audio_preview':
        return 'Audio preview';
      case 'audio_recogniser':
        return 'Audio recogniser';
      case 'user_url':
        return 'Release link';
      case 'embedded_url':
        return 'Embedded link';
      case 'catalog_and_duration':
        return 'Catalogue no. & length';
      case 'tags_and_duration':
        return 'Tags & length';
      case 'tags_only':
        return 'Text match only';
      case 'local_tags':
        return 'Local tags';
    }
  }
</script>

<script lang="ts">
  import type { IdentifyStepOutcome } from '../ipc/bindings/IdentifyStepOutcome';
  import type { IdentifyStepReport } from '../ipc/bindings/IdentifyStepReport';
  import type { MatchEvidence } from '../ipc/bindings/MatchEvidence';
  import Button from '../design/Button.svelte';
  let {
    evidence,
    steps,
    onopenproviders,
  }: { evidence: MatchEvidence | null; steps: IdentifyStepReport[]; onopenproviders?: () => void } =
    $props();
  const checked = $derived(steps.filter((step) => step.provider !== null && step.step >= 3));

  function outcomeText(outcome: IdentifyStepOutcome) {
    switch (outcome.kind) {
      case 'ran':
        return outcome.candidates === 1
          ? '1 match'
          : outcome.candidates.toLocaleString('en-GB') + ' matches';
      case 'failed':
        return outcome.reason;
      case 'skipped_missing_key':
        return 'Key needed';
      case 'skipped_confident':
        return 'Earlier match used';
      case 'skipped_no_input':
        return 'Missing input';
      case 'needs_url':
        return 'Release link needed';
      case 'read':
        return 'Read tags';
    }
  }
</script>

{#if checked.length || evidence}
  <details>
    <summary>{evidence ? formatBasisLabel(evidence.basis) : 'Search details'}</summary>
    {#if evidence?.duration_delta_ms != null}
      <p>
        Length difference <span class="numeric"
          >{(evidence.duration_delta_ms / 1000).toFixed(1)} s</span
        >
      </p>
    {/if}
    <dl>
      {#each checked as step, index (index)}
        <div>
          <dt>{formatProviderName(step.provider!)}</dt>
          <dd>{outcomeText(step.outcome)}</dd>
        </div>
      {/each}
    </dl>
    {#if onopenproviders && checked.some((step) => step.outcome.kind === 'skipped_missing_key')}
      <div class="provider-action">
        <Button size="sm" onclick={onopenproviders}>Provider settings</Button>
      </div>
    {/if}
  </details>
{/if}

<style>
  details {
    font-size: var(--text-xs);
    color: var(--color-text-2);
  }
  summary {
    cursor: pointer;
    width: fit-content;
  }
  dl,
  p,
  .provider-action {
    margin: var(--space-2) 0 0;
  }
  dl {
    display: grid;
    gap: var(--space-1);
  }
  dl div {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
