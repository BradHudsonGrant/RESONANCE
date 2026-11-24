<script lang="ts">
  import { untrack } from 'svelte';
  import Button from '../design/Button.svelte';
  import Icon from '../design/Icon.svelte';
  import Thumb from '../design/Thumb.svelte';
  import { artworkUrl } from '../ipc/artwork';
  import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
  import type { PrepItem } from '../ipc/bindings/PrepItem';
  import type { StagedField } from '../ipc/bindings/StagedField';
  import type { TagName } from '../ipc/bindings/TagName';
  import MatchEvidenceHeader, { formatProviderName } from './MatchEvidenceHeader.svelte';
  import {
    LIBRARY_ONLY_FIELDS,
    MISSING_LABELS,
    basisSentence,
    candidateTitle,
    lengthDifference,
    localSource,
    percent,
    releaseLine,
    resolveActiveCoverIndex,
    reviewableFields,
    selectedCandidate,
  } from './prep-view';

  interface Props {
    item: PrepItem;
    busy: boolean;
    searching: boolean;
    linkBusy: boolean;
    compact: boolean;
    chosenCover: number | undefined;
    canUndo: boolean;
    onaccept: (fields: TagName[], includeArtwork: boolean) => void;
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
    onaccept,
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

  let url = $state('');
  let urlFor = -1;
  $effect.pre(() => {
    const id = item.track.id;
    untrack(() => {
      if (id !== urlFor) {
        urlFor = id;
        url = '';
      }
    });
  });

  const match = $derived(selectedCandidate(item));
  const suggested = $derived(reviewableFields(item));
  const cover = $derived(item.artwork.proposed);
  const covers = $derived(item.candidates.filter((candidate) => candidate.has_artwork));
  const activeCover = $derived(resolveActiveCoverIndex(item, chosenCover, covers));
  const total = $derived(suggested.length + (cover ? 1 : 0));
  const source = $derived(match ? null : localSource(item));
  const needsKey = $derived(item.steps.some((step) => step.outcome.kind === 'skipped_missing_key'));
  const evidence = $derived(match?.evidence ?? item.evidence);

  const heroArt = $derived(cover?.artwork ?? item.artwork.artwork ?? match?.artwork_hash ?? null);
  const heroCaption = $derived(
    cover
      ? 'Suggested cover · ' + formatProviderName(cover.provider)
      : item.artwork.artwork
        ? 'Cover in the file'
        : match?.artwork_hash
          ? 'Cover of this match'
          : 'No cover',
  );

  const verdict = $derived.by(() => {
    if (match && evidence) {
      return [
        basisSentence(evidence.basis),
        lengthDifference(evidence.duration_delta_ms),
        [...new Set(evidence.providers.map(formatProviderName))].join(', '),
      ]
        .filter(Boolean)
        .join(' · ');
    }
    if (source) return 'Suggested from the ' + source.toLowerCase() + '. Not searched online yet';
    if (item.stage === 'not_searched') return 'Not searched online yet';
    if (item.stage === 'no_match') {
      return needsKey
        ? 'No online match. Some providers were skipped because they need a key'
        : 'No online match. Paste the release page below, or edit the tags';
    }
    return total ? '' : 'Every tag is up to date';
  });

  const fileRelease = $derived(
    releaseLine({
      album: currentOf('album'),
      label: currentOf('label'),
      catno: currentOf('catno'),
      year: Number(currentOf('year')) || null,
    }),
  );

  function currentOf(name: TagName) {
    return item.fields.find((field) => field.field === name)?.current_value ?? null;
  }
  function isSuggested(field: StagedField) {
    return field.changes && !field.locked_by_user;
  }

  /** Accepts every suggestion on this track. */
  export function acceptAll() {
    if (busy || !total) return;
    onaccept(
      suggested.map((field) => field.field),
      !!cover,
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
  <div class="body">
    {#if compact}
      <span class="back-to-tracks"><Button size="sm" onclick={onback}>Back to tracks</Button></span>
    {/if}
    <header class="hero">
      <figure>
        <Thumb
          seed={item.track.id}
          size="xl"
          src={heroArt ? artworkUrl(heroArt) : null}
          alt={heroCaption}
        />
        <figcaption>{heroCaption}</figcaption>
      </figure>
      <div class="identity">
        <h2 title={item.track.title}>{item.track.title}</h2>
        <p class="artist">{item.track.artist || 'No artist'}</p>
        {#if fileRelease}<p class="meta">{fileRelease}</p>{/if}
        {#if verdict}
          <p class="verdict">
            {#if match}<span class="numeric confidence">{percent(match.confidence)}</span>{/if}
            <span>{verdict}</span>
          </p>
        {/if}
        <div class="hero-actions">
          {#if onpreview}
            <Button size="sm" title="Preview (Space)" onclick={onpreview}>
              <Icon name="play" size="xs" />Preview
            </Button>
          {/if}
          {#if item.stage === 'not_searched' || item.stage === 'no_match' || source}
            <Button size="sm" disabled={searching} title="Search online (F)" onclick={onsearch}>
              <Icon name="search" size="xs" />{item.stage === 'no_match'
                ? 'Search again'
                : 'Find a match'}
            </Button>
          {/if}
          {#if needsKey && onopenproviders}
            <Button size="sm" onclick={onopenproviders}>Provider settings</Button>
          {/if}
        </div>
      </div>
    </header>

    {#if item.candidates.length}
      <section class="group" aria-labelledby="prep-matches">
        <h3 id="prep-matches">
          Matches found <span class="numeric">{item.candidates.length.toLocaleString('en-GB')}</span
          >
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
                <Thumb
                  seed={candidate.index + item.track.id}
                  size="sm"
                  src={candidate.artwork_hash ? artworkUrl(candidate.artwork_hash) : null}
                  alt=""
                />
                <span class="match-text">
                  <span class="name">{candidateTitle(candidate)}</span>
                  <span class="meta"
                    >{[releaseLine(candidate), formatProviderName(candidate.provider)]
                      .filter(Boolean)
                      .join(' · ')}</span
                  >
                </span>
                <span class="match-side">
                  <span class="numeric">{percent(candidate.confidence)}</span>
                  <span class="in-use" class:shown={active}>In use</span>
                </span>
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
    </form>

    <section class="group" aria-labelledby="prep-tags">
      <div class="group-head">
        <h3 id="prep-tags">
          Tags
          {#if total}<span class="numeric">{total.toLocaleString('en-GB')} suggested</span>{/if}
        </h3>
        {#if total}
          <Button size="sm" disabled={busy} title="Accept all (Enter)" onclick={acceptAll}
            >Accept all</Button
          >
        {/if}
      </div>
      <table class="tags">
        <thead>
          <tr>
            <th scope="col">Tag</th>
            <th scope="col">In the file</th>
            <th scope="col">Suggested</th>
            <th scope="col" aria-label="Action"></th>
          </tr>
        </thead>
        <tbody>
          <tr class:suggested={!!cover}>
            <th scope="row">Cover</th>
            <td>
              {#if item.artwork.artwork}
                <Thumb
                  seed={item.track.id}
                  size="sm"
                  src={artworkUrl(item.artwork.artwork)}
                  alt="Cover in the file"
                />
              {:else}<span class="missing">Missing</span>{/if}
            </td>
            <td>
              {#if covers.length > 1 && cover}
                <span class="covers">
                  {#each covers as candidate (candidate.index)}
                    <button
                      type="button"
                      class="cover-choice"
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
                </span>
              {:else if cover}
                <Thumb
                  seed={item.track.id + 1}
                  size="sm"
                  src={artworkUrl(cover.artwork)}
                  alt="Suggested cover"
                />
              {/if}
            </td>
            <td class="action">
              {#if cover}
                <Button
                  size="sm"
                  disabled={busy}
                  aria-label="Accept cover"
                  onclick={() => onaccept([], true)}>Accept</Button
                >
              {/if}
            </td>
          </tr>
          {#each item.fields as field (field.field)}
            {@const offered = isSuggested(field)}
            <tr class:suggested={offered}>
              <th scope="row"
                >{MISSING_LABELS[field.field]}{#if LIBRARY_ONLY_FIELDS.has(field.field)}<small
                    title="Kept in the library, not the file">Library</small
                  >{/if}</th
              >
              <td class:old={offered} title={field.current_value ?? ''}>
                {#if field.current_value}{field.current_value}{:else}<span class="missing"
                    >Missing</span
                  >{/if}
              </td>
              <td title={field.proposed_value ?? ''}>
                {#if offered}{field.proposed_value || 'Clear'}{:else if field.changes}<span
                    class="kept">{field.proposed_value}</span
                  >{/if}
              </td>
              <td class="action">
                {#if offered}
                  <Button
                    size="sm"
                    disabled={busy}
                    aria-label={'Accept ' + MISSING_LABELS[field.field]}
                    onclick={() => onaccept([field.field], false)}>Accept</Button
                  >
                {:else if field.changes}
                  <span class="kept" title="You set this tag, so suggestions never change it"
                    >Yours</span
                  >
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>

    <div class="details">
      <MatchEvidenceHeader evidence={item.evidence} steps={item.steps} {onopenproviders} />
    </div>
  </div>

  <footer class="actions">
    {#if total}
      <Button
        variant="primary"
        size="sm"
        disabled={busy}
        title="Accept all (Enter)"
        onclick={acceptAll}
      >
        {total === 1 ? 'Accept 1 change' : 'Accept ' + total.toLocaleString('en-GB') + ' changes'}
      </Button>
      <Button size="sm" disabled={busy} title="Skip the suggestions (S)" onclick={onskip}
        >Skip</Button
      >
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
    grid-template-rows: minmax(0, 1fr) auto;
    container-type: inline-size;
    background: var(--color-background);
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
  p,
  figure {
    margin: 0;
  }
  .hero {
    display: grid;
    grid-template-columns: var(--thumb-xl) minmax(0, 1fr);
    gap: var(--space-5);
    align-items: start;
  }
  figure {
    display: grid;
    gap: var(--space-2);
  }
  figcaption {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }
  .identity {
    min-width: 0;
    display: grid;
    gap: var(--space-1-5);
    align-content: start;
  }
  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }
  .artist {
    font-size: var(--text-base);
    color: var(--color-text-2);
  }
  .meta {
    min-width: 0;
    font-size: var(--text-sm);
    color: var(--color-text-2);
    overflow-wrap: anywhere;
  }
  .verdict {
    display: flex;
    align-items: baseline;
    gap: var(--space-2-5);
    margin-top: var(--space-2);
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .confidence {
    font-size: var(--text-lg);
    color: var(--color-text);
  }
  .hero-actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2-5);
    margin-top: var(--space-3);
  }
  .hero-actions :global(button),
  .actions :global(button) {
    gap: var(--space-1-5);
  }
  .group {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }
  .group-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
  h3 {
    margin: 0;
    display: flex;
    gap: var(--space-2);
    align-items: baseline;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--color-text-2);
  }
  h3 .numeric {
    color: var(--color-muted);
    font-weight: var(--weight-regular);
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .matches li + li {
    border-top: var(--border-width) solid var(--color-border);
  }
  .match {
    width: 100%;
    min-width: 0;
    display: grid;
    grid-template-columns: var(--control-height-keybadge) var(--thumb-sm) minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2);
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
  .cover-choice:focus-visible {
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
  .name {
    font-size: var(--text-md);
  }
  .match-side {
    display: grid;
    justify-items: end;
    gap: var(--space-1);
    font-size: var(--text-sm);
    color: var(--color-text-2);
  }
  .in-use {
    font-size: var(--text-xs);
    color: var(--color-text);
    opacity: 0;
  }
  .in-use.shown {
    opacity: 1;
  }
  .link {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2-5);
    font-size: var(--text-sm);
  }
  .link label {
    color: var(--color-text-2);
  }
  input[type='url'] {
    min-width: 0;
    height: var(--control-height-sm);
    font: inherit;
    color: var(--color-text);
    background: var(--color-surface-2);
    border: var(--border-width) solid var(--color-border-control);
    border-radius: var(--radius-sm);
    padding-inline: var(--space-2-5);
  }
  .tags {
    width: 100%;
    border-spacing: 0;
    table-layout: fixed;
    font-size: var(--text-sm);
  }
  .tags th,
  .tags td {
    height: var(--row-height);
    padding: var(--space-1) var(--space-2-5);
    text-align: left;
    vertical-align: middle;
    border-bottom: var(--border-width) solid var(--color-border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tags thead th {
    height: var(--table-header-height);
    font-weight: var(--weight-regular);
    color: var(--color-muted);
  }
  .tags thead th:first-child {
    width: 9rem;
  }
  .tags thead th:last-child {
    width: 6rem;
  }
  .tags tbody th {
    font-weight: var(--weight-regular);
    color: var(--color-text-2);
  }
  .tags tbody th small {
    margin-left: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }
  .tags tr.suggested {
    background: var(--color-surface);
  }
  .tags tr.suggested td:nth-child(3) {
    color: var(--color-text);
    font-weight: var(--weight-medium);
  }
  .old,
  .missing,
  .kept {
    color: var(--color-muted);
  }
  .action {
    text-align: right;
  }
  .covers {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }
  .tags td:has(.covers) {
    white-space: normal;
    padding-block: var(--space-2);
  }
  .cover-choice {
    padding: 0;
    border: var(--border-width) solid transparent;
    border-radius: var(--radius-md);
    background: transparent;
    cursor: pointer;
    line-height: 0;
  }
  .cover-choice[aria-pressed='true'] {
    border-color: var(--color-text);
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
  @container (max-width: 520px) {
    .hero {
      grid-template-columns: minmax(0, 1fr);
    }
    .tags th:nth-child(2),
    .tags td:nth-child(2) {
      display: none;
    }
    .link {
      grid-template-columns: minmax(0, 1fr) auto;
    }
    .link label {
      grid-column: 1 / -1;
    }
  }
  @media (forced-colors: active) {
    .match[aria-pressed='true'],
    .cover-choice[aria-pressed='true'] {
      outline: 2px solid Highlight;
    }
    .tags tr.suggested {
      outline: 1px solid CanvasText;
    }
  }
</style>
