<script lang="ts">
  import { untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import Thumb from '../design/Thumb.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
  import type { PrepItem } from '../ipc/bindings/PrepItem';
  import type { TagName } from '../ipc/bindings/TagName';
  import MatchEvidenceHeader, { formatProviderName } from './MatchEvidenceHeader.svelte';
  import {
    LIBRARY_ONLY_FIELDS,
    MISSING_LABELS,
    basisSentence,
    candidateTitle,
    keptFields,
    lengthDifference,
    localSource,
    percent,
    releaseLine,
    resolveActiveCoverIndex,
    reviewableFields,
    selectedCandidate,
    stillMissing,
  } from './prep-view';

  interface Props {
    item: PrepItem;
    busy: boolean;
    searching: boolean;
    linkBusy: boolean;
    compact: boolean;
    chosenCover: number | undefined;
    canUndo: boolean;
    onsave: (fields: TagName[], includeArtwork: boolean, skipRest: boolean) => void;
    onskip: () => void;
    onchoose: (index: number) => void;
    oncover: (index: number) => void;
    onlookup: (url: string) => Promise<boolean>;
    onsearch: () => void;
    onedit: () => void;
    onwrite: () => void;
    onrename?: () => void;
    onpreview?: () => void;
    onback: () => void;
    onundo: () => void;
    onopenproviders?: () => void;
  }
  let {
    item,
    busy,
    searching,
    linkBusy,
    compact,
    chosenCover,
    canUndo,
    onsave,
    onskip,
    onchoose,
    oncover,
    onlookup,
    onsearch,
    onedit,
    onwrite,
    onrename,
    onpreview,
    onback,
    onundo,
    onopenproviders,
  }: Props = $props();

  const excluded = new SvelteSet<string>();
  let excludedFor = '';
  let url = $state('');
  $effect.pre(() => {
    const key = item.track.id + ':' + item.review_token;
    untrack(() => {
      if (key === excludedFor) return;
      if (!excludedFor.startsWith(item.track.id + ':')) url = '';
      excludedFor = key;
      excluded.clear();
    });
  });

  const match = $derived(selectedCandidate(item));
  const fields = $derived(reviewableFields(item));
  const kept = $derived(keptFields(item));
  const cover = $derived(item.artwork.proposed);
  const covers = $derived(item.candidates.filter((candidate) => candidate.has_artwork));
  const activeCover = $derived(resolveActiveCoverIndex(item, chosenCover, covers));
  const chosenFields = $derived(fields.filter((field) => !excluded.has(field.field)));
  const includeArtwork = $derived(!!cover && !excluded.has('artwork'));
  const total = $derived(fields.length + (cover ? 1 : 0));
  const chosen = $derived(chosenFields.length + (includeArtwork ? 1 : 0));
  const missing = $derived(stillMissing(item));
  const source = $derived(match ? null : localSource(item));
  const needsKey = $derived(item.steps.some((step) => step.outcome.kind === 'skipped_missing_key'));
  const evidence = $derived(match?.evidence ?? item.evidence);
  const evidenceText = $derived(
    evidence
      ? [
          basisSentence(evidence.basis),
          lengthDifference(evidence.duration_delta_ms),
          [...new Set(evidence.providers.map(formatProviderName))].join(', '),
        ]
          .filter(Boolean)
          .join(' · ')
      : '',
  );

  const currentValue = (name: TagName) =>
    item.fields.find((field) => field.field === name)?.current_value ?? null;
  const fileRelease = $derived(
    releaseLine({
      album: currentValue('album'),
      label: currentValue('label'),
      catno: currentValue('catno'),
      year: Number(currentValue('year')) || null,
    }),
  );

  function toggle(name: string) {
    if (excluded.has(name)) excluded.delete(name);
    else excluded.add(name);
  }

  /** Saves the ticked changes and sets the rest aside, so the track leaves the review list. */
  export function save() {
    if (busy || !total) return;
    onsave(
      chosenFields.map((field) => field.field),
      includeArtwork,
      chosen < total,
    );
  }

  async function lookup(event: SubmitEvent) {
    event.preventDefault();
    const link = url.trim();
    if (!link || linkBusy) return;
    if (await onlookup(link)) url = '';
  }

  function coverLabel(candidate: MatchCandidateSummary) {
    return (
      'Use cover from ' +
      formatProviderName(candidate.artwork_provider ?? candidate.provider) +
      (candidate.album ? ', ' + candidate.album : '')
    );
  }
</script>

<section class="review" aria-label="Review" aria-busy={busy}>
  <header class="head">
    {#if compact}
      <span class="back-to-tracks"><Button size="sm" onclick={onback}>Back to tracks</Button></span>
    {/if}
    <div class="heading">
      <h2 title={item.track.title}>{item.track.title}</h2>
      {#if item.track.artist}<p>{item.track.artist}</p>{/if}
    </div>
    {#if onpreview}
      <Button
        variant="icon"
        size="sm"
        ariaLabel="Preview"
        title="Preview (Space)"
        onclick={onpreview}
      >
        <Icon name="play" size="xs" />
      </Button>
    {/if}
  </header>

  <div class="body">
    {#if match}
      <div class="match-block">
        <div class="compare">
          <div class="side">
            <span class="side-label">In the file</span>
            <Thumb
              seed={item.track.id}
              size="md"
              src={item.artwork.artwork ? artworkUrl(item.artwork.artwork) : null}
              alt="Current cover"
            />
            <p class="name">{item.track.title}</p>
            <p class="meta">{item.track.artist || 'No artist'}</p>
            {#if fileRelease}<p class="meta">{fileRelease}</p>{/if}
          </div>
          <span class="arrow" aria-hidden="true"><Icon name="arrow-right" size="sm" /></span>
          <div class="side">
            <span class="side-label">Suggested</span>
            <Thumb
              seed={item.track.id + 1}
              size="md"
              src={cover
                ? artworkUrl(cover.artwork)
                : match.artwork_hash
                  ? artworkUrl(match.artwork_hash)
                  : null}
              alt="Suggested cover"
            />
            <p class="name">{candidateTitle(match)}</p>
            {#if releaseLine(match)}<p class="meta">{releaseLine(match)}</p>{/if}
          </div>
        </div>
        {#if evidence}
          <p class="evidence">
            <span class="numeric confidence">{percent(match.confidence)}</span>
            <span>{evidenceText}</span>
          </p>
        {/if}
      </div>
    {:else if item.stage === 'not_searched' || source}
      <div class="state">
        <p class="state-title">
          {source ? 'Suggested from the ' + source.toLowerCase() : 'Not searched online yet'}
        </p>
        <p class="meta">
          Search online to compare the audio against release databases and fill in the rest
        </p>
        <Button size="sm" disabled={searching} onclick={onsearch}>
          <Icon name="search" size="xs" />Find a match
        </Button>
      </div>
    {:else if item.stage === 'no_match'}
      <div class="state">
        <p class="state-title">No online match</p>
        <p class="meta">
          {needsKey
            ? 'Some providers were skipped because they need a key'
            : 'Paste the release page below, or edit the tags by hand'}
        </p>
        <div class="state-actions">
          <Button size="sm" disabled={searching} onclick={onsearch}>Search again</Button>
          {#if needsKey && onopenproviders}
            <Button size="sm" onclick={onopenproviders}>Provider settings</Button>
          {/if}
        </div>
      </div>
    {/if}

    {#if total}
      <section class="group" aria-labelledby="prep-changes">
        <h3 id="prep-changes">
          Changes <span class="numeric"
            >{chosen.toLocaleString('en-GB')} / {total.toLocaleString('en-GB')}</span
          >
        </h3>
        <ul class="changes">
          {#each fields as field (field.field)}
            <li class="change" class:off={excluded.has(field.field)}>
              <input
                id={'prep-change-' + field.field}
                type="checkbox"
                aria-label={MISSING_LABELS[field.field]}
                checked={!excluded.has(field.field)}
                onchange={() => toggle(field.field)}
              />
              <label class="field" for={'prep-change-' + field.field}
                >{MISSING_LABELS[field.field]}{#if LIBRARY_ONLY_FIELDS.has(field.field)}<small
                    >Library</small
                  >{/if}</label
              >
              <span class="old" title={field.current_value ?? ''}>{field.current_value || '—'}</span
              >
              <span class="new" title={field.proposed_value ?? ''}
                >{field.proposed_value || '—'}</span
              >
            </li>
          {/each}
          {#if cover}
            <li class="change artwork" class:off={excluded.has('artwork')}>
              <input
                id="prep-change-artwork"
                type="checkbox"
                aria-label="Artwork"
                checked={!excluded.has('artwork')}
                onchange={() => toggle('artwork')}
              />
              <label class="field" for="prep-change-artwork">Artwork</label>
              <span class="old">
                {#if item.artwork.artwork}
                  <Thumb
                    seed={item.track.id}
                    size="sm"
                    src={artworkUrl(item.artwork.artwork)}
                    alt="Current cover"
                  />
                {:else}—{/if}
              </span>
              <span class="new covers">
                {#if covers.length > 1}
                  {#each covers as candidate (candidate.index)}
                    <button
                      type="button"
                      class="cover"
                      aria-pressed={candidate.index === activeCover}
                      aria-label={coverLabel(candidate)}
                      title={coverLabel(candidate)}
                      disabled={busy}
                      onclick={() => oncover(candidate.index)}
                    >
                      <Thumb
                        seed={candidate.index}
                        size="sm"
                        src={candidate.artwork_hash ? artworkUrl(candidate.artwork_hash) : null}
                        alt=""
                      />
                    </button>
                  {/each}
                {:else}
                  <Thumb
                    seed={item.track.id + 1}
                    size="sm"
                    src={artworkUrl(cover.artwork)}
                    alt="Suggested cover"
                  />
                {/if}
              </span>
            </li>
          {/if}
        </ul>
        {#if kept.length}
          <p class="note">
            Kept your own {kept
              .map((field) => MISSING_LABELS[field.field].toLowerCase())
              .join(', ')}
          </p>
        {/if}
      </section>
    {:else if match || item.stage === 'done'}
      <p class="note">Nothing to change. The file already has these tags</p>
    {/if}

    {#if item.candidates.length > 1}
      <section class="group" aria-labelledby="prep-matches">
        <h3 id="prep-matches">
          Matches <span class="numeric">{item.candidates.length.toLocaleString('en-GB')}</span>
        </h3>
        <ol class="matches">
          {#each item.candidates as candidate, position (candidate.index)}
            {@const active = candidate.index === item.selected_candidate}
            <li>
              <button
                type="button"
                class="match"
                aria-pressed={active}
                disabled={busy}
                title={position < 9 ? 'Use this match (' + (position + 1) + ')' : 'Use this match'}
                onclick={() => onchoose(candidate.index)}
              >
                <span class="keycap numeric" aria-hidden="true"
                  >{position < 9 ? position + 1 : ''}</span
                >
                <span class="match-text">
                  <span class="name">{candidateTitle(candidate)}</span>
                  <span class="meta"
                    >{[releaseLine(candidate), formatProviderName(candidate.provider)]
                      .filter(Boolean)
                      .join(' · ')}</span
                  >
                </span>
                <span class="numeric match-score">{percent(candidate.confidence)}</span>
                <span class="tick" class:shown={active} aria-hidden="true"
                  ><Icon name="check" size="xs" /></span
                >
              </button>
            </li>
          {/each}
        </ol>
      </section>
    {/if}

    <form class="link" onsubmit={lookup}>
      <label for="prep-release-link">
        {item.candidates.length ? 'Not the right release?' : 'Know the release?'}
      </label>
      <div class="link-row">
        <input
          id="prep-release-link"
          type="url"
          bind:value={url}
          placeholder="Paste a release link"
          autocomplete="off"
        />
        <Button type="submit" size="sm" disabled={busy || linkBusy || !url.trim()}
          >{linkBusy ? 'Looking up' : 'Look up'}</Button
        >
      </div>
    </form>

    {#if missing.length}
      <p class="note">Still missing {missing.join(', ').toLowerCase()}</p>
    {/if}

    <div class="details">
      <MatchEvidenceHeader evidence={item.evidence} steps={item.steps} {onopenproviders} />
    </div>
  </div>

  <footer class="actions">
    {#if total}
      <Button variant="primary" size="sm" disabled={busy} title="Save (Enter)" onclick={save}>
        {chosen === 0
          ? 'Skip all changes'
          : chosen === 1
            ? 'Save 1 change'
            : 'Save ' + chosen.toLocaleString('en-GB') + ' changes'}
      </Button>
      <Button size="sm" disabled={busy} title="Skip (S)" onclick={onskip}>Skip</Button>
    {/if}
    <span class="spacer"></span>
    {#if canUndo}
      <Button
        size="sm"
        ariaLabel="Undo save"
        title="Undo (Ctrl+Z)"
        disabled={busy}
        onclick={onundo}
      >
        <Icon name="undo" size="xs" />Undo
      </Button>
    {/if}
    <Button size="sm" title="Edit tags (E)" onclick={onedit}>Edit tags</Button>
    {#if item.has_unwritten}
      <Button size="sm" title="Write edits to the file (W)" onclick={onwrite}>Write edits</Button>
    {/if}
    {#if onrename}
      <Button size="sm" onclick={onrename}>Rename file</Button>
    {/if}
  </footer>
</section>

<style>
  .review {
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    container-type: inline-size;
    background: var(--color-background);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    padding: var(--space-3) var(--space-5);
    border-bottom: var(--border-width) solid var(--color-border);
  }
  .heading {
    flex: 1;
    min-width: 0;
  }
  h2,
  .heading p {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  h2 {
    font-size: var(--text-lg);
    font-weight: var(--weight-medium);
  }
  .heading p {
    margin-top: var(--space-1);
    color: var(--color-text-2);
    font-size: var(--text-sm);
  }
  .body {
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-5);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  p {
    margin: 0;
  }
  .match-block {
    display: grid;
    gap: var(--space-4);
  }
  .compare {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
  }
  .side {
    min-width: 0;
    display: grid;
    gap: var(--space-1-5);
    justify-items: start;
  }
  .side-label,
  h3 {
    color: var(--color-text-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .name {
    min-width: 0;
    max-width: 100%;
    font-size: var(--text-md);
    overflow-wrap: anywhere;
  }
  .meta {
    min-width: 0;
    max-width: 100%;
    color: var(--color-text-2);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .arrow {
    align-self: center;
    color: var(--color-muted);
  }
  .evidence {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .confidence {
    font-size: var(--text-lg);
    color: var(--color-text);
  }
  .state {
    display: grid;
    gap: var(--space-2);
    justify-items: start;
  }
  .state-title {
    font-size: var(--text-md);
  }
  .state :global(button) {
    margin-top: var(--space-2);
  }
  .state-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2-5);
  }
  .group {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }
  h3 {
    margin: 0;
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
  }
  h3 .numeric {
    color: var(--color-muted);
    font-weight: var(--weight-regular);
  }
  ol,
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .matches li + li,
  .changes li + li {
    border-top: var(--border-width) solid var(--color-border);
  }
  .match {
    width: 100%;
    min-width: 0;
    display: grid;
    grid-template-columns: var(--control-height-keybadge) minmax(0, 1fr) auto var(--space-4);
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-2);
    font: inherit;
    text-align: left;
    color: var(--color-text);
    background: transparent;
    border: 0;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background-color var(--duration-fast) var(--ease-out);
  }
  .match:hover:not(:disabled) {
    background: var(--color-surface);
  }
  .match[aria-pressed='true'] {
    background: var(--color-surface-2);
  }
  .match:focus-visible,
  .cover:focus-visible {
    outline: var(--focus-ring-width) solid var(--color-text);
    outline-offset: 1px;
  }
  .keycap {
    display: inline-grid;
    place-items: center;
    height: var(--control-height-keybadge);
    font-size: var(--text-xs);
    color: var(--color-text-2);
    border: var(--border-width) solid var(--color-border);
    border-radius: var(--radius-xs);
  }
  .keycap:empty {
    border-color: transparent;
  }
  .match-text {
    min-width: 0;
    display: grid;
    gap: var(--space-1);
  }
  .match-text .name,
  .match-text .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .match-score {
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .tick {
    opacity: 0;
  }
  .tick.shown {
    opacity: 1;
  }
  .link {
    display: grid;
    gap: var(--space-2);
  }
  .link label {
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .link-row {
    display: flex;
    gap: var(--space-2);
    min-width: 0;
  }
  input[type='url'] {
    flex: 1;
    min-width: 0;
    height: var(--control-height-sm);
    font: inherit;
    font-size: var(--text-sm);
    color: var(--color-text);
    background: var(--color-surface-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    padding-inline: var(--space-2-5);
  }
  .change {
    display: grid;
    grid-template-columns: var(--checkbox-size) minmax(0, 8rem) minmax(0, 1fr) minmax(0, 1.4fr);
    align-items: center;
    gap: var(--space-3);
    min-height: var(--row-height);
    padding-block: var(--space-1);
    font-size: var(--text-sm);
  }
  .change input {
    width: var(--checkbox-size);
    height: var(--checkbox-size);
    margin: 0;
    accent-color: var(--color-text);
    cursor: pointer;
  }
  .field {
    color: var(--color-text-2);
    cursor: pointer;
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    min-width: 0;
  }
  .field small {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }
  .old,
  .new {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .old {
    color: var(--color-muted);
  }
  .change.off .new {
    color: var(--color-muted);
    text-decoration: line-through;
  }
  .covers {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    white-space: normal;
  }
  .cover {
    padding: 0;
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    cursor: pointer;
    line-height: 0;
  }
  .cover[aria-pressed='true'] {
    border-color: var(--color-text);
  }
  .note {
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .details {
    margin-top: auto;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2-5);
    padding: var(--space-3) var(--space-5);
    border-top: var(--border-width) solid var(--color-border);
    background: var(--color-surface);
  }
  .spacer {
    flex: 1;
  }
  .actions :global(button) {
    gap: var(--space-1-5);
  }
  @container (max-width: 440px) {
    .match-block {
      display: grid;
      gap: var(--space-4);
    }
    .compare {
      grid-template-columns: minmax(0, 1fr);
    }
    .arrow {
      display: none;
    }
    .change {
      grid-template-columns: var(--checkbox-size) minmax(0, 1fr);
    }
    .old {
      display: none;
    }
    .new {
      grid-column: 2;
    }
  }
  @media (forced-colors: active) {
    .match[aria-pressed='true'],
    .cover[aria-pressed='true'] {
      outline: 2px solid Highlight;
    }
  }
</style>
