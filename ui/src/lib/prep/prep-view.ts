import type { AcceptedTagWriteReport } from '../ipc/bindings/AcceptedTagWriteReport';
import type { MatchBasis } from '../ipc/bindings/MatchBasis';
import type { MatchCandidateSummary } from '../ipc/bindings/MatchCandidateSummary';
import type { PrepItem } from '../ipc/bindings/PrepItem';
import type { PrepStageCounts } from '../ipc/bindings/PrepStageCounts';
import type { PrepStageName } from '../ipc/bindings/PrepStageName';
import type { StagedField } from '../ipc/bindings/StagedField';
import type { TagName } from '../ipc/bindings/TagName';
import type { TagWriteReport } from '../ipc/bindings/TagWriteReport';
import type { TrackIdentification } from '../ipc/bindings/TrackIdentification';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';
import { formatProviderName } from './MatchEvidenceHeader.svelte';

export type MissingField = TagName | 'artwork';

export const MISSING_LABELS: Record<MissingField, string> = {
  artist: 'Artist',
  title: 'Title',
  artwork: 'Artwork',
  genre: 'Genre',
  album: 'Album',
  mix: 'Mix',
  label: 'Label',
  catno: 'Catalogue number',
  year: 'Year',
  bpm: 'BPM',
  key: 'Key',
  track_number: 'Track number',
  subgenre: 'Subgenre',
  album_artist: 'Album artist',
  composer: 'Composer',
  remixer: 'Remixer',
  comment: 'Comment',
};

export const MISSING_FIELDS = Object.keys(MISSING_LABELS) as MissingField[];

/** The tags a DJ notices are missing. BPM and key come from analysis, so they are left out. */
const CORE_FIELDS: MissingField[] = ['artist', 'title', 'artwork', 'genre', 'label', 'year'];

/** Fields the common tag formats have no frame for. Prep shows only what a file carries, so the
 * mix appears as part of the title instead. */
const LIBRARY_ONLY_FIELDS: ReadonlySet<TagName> = new Set(['mix', 'subgenre']);

/** The tags a file can hold, in the order staging shows them. */
export function fileFields(item: PrepItem): StagedField[] {
  return item.fields.filter((field) => !LIBRARY_ONLY_FIELDS.has(field.field));
}

const STAGE_WORDS: Record<PrepStageName, string> = {
  review: 'to review',
  not_searched: 'not searched',
  no_match: 'with no match',
  done: 'done',
};

/** How the view's tracks stand, in a few words: "36 to review · 395 not searched". */
export function stageSummary(counts: PrepStageCounts): string {
  return (['review', 'not_searched', 'no_match', 'done'] as const)
    .filter((stage) => counts[stage] > 0)
    .map((stage) => counts[stage].toLocaleString('en-GB') + ' ' + STAGE_WORDS[stage])
    .join(' · ');
}
export function createSelectionSet(): Set<number> {
  return new Set<number>();
}

export function pruneSelection(selected: Set<number>, validIds: readonly number[]): boolean {
  const valid = new Set(validIds);
  let changed = false;
  for (const id of selected) {
    if (!valid.has(id)) {
      selected.delete(id);
      changed = true;
    }
  }
  return changed;
}

export function intersectSelection(selected: Set<number>, validIds: readonly number[]): number[] {
  const valid = new Set(validIds);
  const kept: number[] = [];
  for (const id of selected) {
    if (valid.has(id)) kept.push(id);
  }
  return kept;
}

export function buildMissingQuery(
  batchQuery: TrackQuery | null,
  missing: MissingField | '',
): TrackQuery | null {
  if (!missing) return batchQuery;
  const column = ['album_artist', 'subgenre', 'track_number'].includes(missing)
    ? `tag:${missing}`
    : missing;
  return {
    ...batchQuery,
    filters: [
      ...(batchQuery?.filters ?? []),
      { any_of: [{ column, condition: { op: 'missing' } }] },
    ],
  };
}

/** The list's columns, the tags DJ stores and libraries lead with. */
export type ListColumn = 'title' | 'genre' | 'label' | 'year' | 'status';
export interface ListSort {
  column: ListColumn;
  direction: 'asc' | 'desc';
}

export const LIST_COLUMNS: ReadonlyArray<{ id: ListColumn; label: string }> = [
  { id: 'title', label: 'Track' },
  { id: 'genre', label: 'Genre' },
  { id: 'label', label: 'Label' },
  { id: 'year', label: 'Year' },
  { id: 'status', label: 'Status' },
];

/** Stages in the order a status sort lists them, work first. */
export const STAGE_ORDER: readonly PrepStageName[] = ['review', 'not_searched', 'no_match', 'done'];

/** Ascending, then descending, then back to the order Prep chose. */
export function nextListSort(sort: ListSort | null, column: ListColumn): ListSort | null {
  if (sort?.column !== column) return { column, direction: 'asc' };
  return sort.direction === 'asc' ? { column, direction: 'desc' } : null;
}

/** The query for a sort; a status sort orders by stage, which Prep does itself. */
export function withListSort(query: TrackQuery | null, sort: ListSort | null): TrackQuery | null {
  if (!sort || sort.column === 'status') return query;
  return {
    ...query,
    sort: sort.column,
    direction: sort.direction,
    then_sort: sort.column === 'title' ? 'artist' : 'title',
    then_direction: 'asc',
  };
}

export function withFindText(query: TrackQuery | null, text: string): TrackQuery | null {
  if (!text) return query;
  return { ...query, text: [query?.text, text].filter(Boolean).join(' ') };
}

/** Core tags the track will still lack after its suggestions are saved. */
export function stillMissing(item: PrepItem): string[] {
  return CORE_FIELDS.filter((name) => {
    if (name === 'artwork') return !item.artwork.artwork && !item.artwork.proposed;
    const field = item.fields.find((entry) => entry.field === name);
    const value = field ? field.current_value || field.proposed_value : item.track[name];
    return !String(value ?? '').trim();
  }).map((name) => MISSING_LABELS[name]);
}

/** Core tags the track lacks right now, in the short words a list row has room for. */
export function missingNow(item: PrepItem): string[] {
  return CORE_FIELDS.filter((name) => {
    if (name === 'artwork') return !item.artwork.artwork;
    const field = item.fields.find((entry) => entry.field === name);
    const value = field ? field.current_value : item.track[name];
    return !String(value ?? '').trim();
  }).map((name) => (name === 'artwork' ? 'cover' : MISSING_LABELS[name].toLowerCase()));
}

/** Suggestions the user can still take: changing, and not a value the user set by hand. */
export function reviewableFields(item: PrepItem): StagedField[] {
  return fileFields(item).filter((field) => field.changes && !field.locked_by_user);
}

/** Suggestions held back because the user typed that field. */
export function keptFields(item: PrepItem): StagedField[] {
  return fileFields(item).filter((field) => field.changes && field.locked_by_user);
}

export function percent(confidence: number): string {
  return Math.round(confidence * 100) + '%';
}

export function selectedCandidate(item: PrepItem): MatchCandidateSummary | null {
  return item.candidates.find((candidate) => candidate.index === item.selected_candidate) ?? null;
}

export function releaseLine(
  parts: Pick<MatchCandidateSummary, 'label' | 'catno' | 'year'> & { album?: string | null },
): string {
  return [parts.album, parts.label, parts.catno, parts.year?.toString()]
    .filter((part) => part && String(part).trim())
    .join(' · ');
}

export function candidateTitle(candidate: MatchCandidateSummary): string {
  const title = candidate.mix ? `${candidate.title ?? ''} (${candidate.mix})` : candidate.title;
  return [candidate.artist, title].filter(Boolean).join(' – ') || 'Untitled release';
}

/** How the match was made, in the words a DJ would use to decide whether to trust it. */
export function basisSentence(basis: MatchBasis): string {
  switch (basis) {
    case 'audio_fingerprint':
      return 'The audio fingerprint matches';
    case 'audio_preview':
      return 'The audio matches the store preview';
    case 'audio_recogniser':
      return 'Recognised from the audio';
    case 'user_url':
      return 'From the release link you gave';
    case 'embedded_url':
      return 'From a link inside the file';
    case 'catalog_and_duration':
      return 'Catalogue number and length match';
    case 'tags_and_duration':
      return 'Names and length match';
    case 'tags_only':
      return 'Only the names match. Listen before saving';
    case 'local_tags':
      return 'Read from the file';
  }
}

export function lengthDifference(ms: number | null): string {
  if (ms === null) return '';
  const seconds = Math.abs(ms) / 1000;
  if (seconds < 1) return 'Same length';
  return `${seconds.toFixed(1)} s ${ms > 0 ? 'longer' : 'shorter'}`;
}

/** Where a track's suggestions came from when no online match backs them. */
export function localSource(item: PrepItem): string | null {
  const providers = new Set(
    reviewableFields(item)
      .map((field) => field.provider)
      .filter((provider): provider is string => !!provider),
  );
  if (!providers.size) return null;
  return [...providers].map(formatProviderName).join(', ');
}

export interface RowStatus {
  /** One plain phrase for where the track stands. */
  primary: string;
  /** What is waiting, in a few words, or empty. */
  secondary: string;
}

function count(n: number, one: string, many: string): string {
  return n.toLocaleString('en-GB') + ' ' + (n === 1 ? one : many);
}

export function rowStatus(item: PrepItem): RowStatus {
  const waiting = reviewableFields(item).length + (item.artwork.proposed ? 1 : 0);
  // Without an online match, waiting suggestions are tidy-ups read from the file itself.
  const source = localSource(item);
  const fixes = waiting
    ? count(waiting, 'fix', 'fixes') + (source ? ' from ' + source.toLowerCase() : '')
    : '';
  switch (item.stage) {
    case 'review': {
      const match = selectedCandidate(item);
      return {
        primary: match ? (match.audio_matched ? 'Audio match' : 'Name match') : 'Match',
        secondary:
          (match ? percent(match.confidence) + ' · ' : '') +
          count(waiting, 'to accept', 'to accept'),
      };
    }
    case 'not_searched':
      return { primary: 'Not searched', secondary: fixes };
    case 'no_match':
      return {
        primary: 'No match',
        secondary: fixes || (item.needs_url ? 'Paste a release link' : ''),
      };
    case 'done':
      return {
        primary: item.has_unwritten ? 'Edits not written' : 'Done',
        secondary: '',
      };
  }
}
export function resolveActiveCoverIndex(
  item: PrepItem,
  chosenCover: number | undefined,
  covers: readonly MatchCandidateSummary[],
): number | null {
  const cover = item.artwork.proposed;
  if (!cover) return null;
  if (chosenCover !== undefined && covers.some((candidate) => candidate.index === chosenCover)) {
    return chosenCover;
  }
  const selected = selectedCandidate(item);
  if (selected?.has_artwork && selected.artwork_hash === cover.artwork) {
    return selected.index;
  }
  const exactHash = covers.find((candidate) => candidate.artwork_hash === cover.artwork);
  if (exactHash) return exactHash.index;
  const coverProvider = formatProviderName(cover.provider);
  if (
    selected?.has_artwork &&
    formatProviderName(selected.artwork_provider ?? selected.provider) === coverProvider
  ) {
    return selected.index;
  }
  return (
    covers.find(
      (candidate) =>
        formatProviderName(candidate.artwork_provider ?? candidate.provider) === coverProvider,
    )?.index ?? null
  );
}

function plural(count: number, one: string, many: string): string {
  return `${count.toLocaleString('en-GB')} ${count === 1 ? one : many}`;
}

export function formatSaveNotice(
  report: Pick<AcceptedTagWriteReport, 'written' | 'library_fields' | 'reverted_fields' | 'failed'>,
): string {
  const parts: string[] = [];
  if (report.written > 0) parts.push(`Saved to ${plural(report.written, 'file', 'files')}`);
  if (report.library_fields > 0) {
    parts.push(`${plural(report.library_fields, 'field', 'fields')} saved to library`);
  }
  if (report.reverted_fields > 0) {
    parts.push(`${plural(report.reverted_fields, 'tag', 'tags')} reverted to file`);
  }
  if (parts.length > 0) return parts.join(' · ');
  return report.failed.length ? 'Save failed' : 'No pending changes';
}

export function formatWriteNotice(report: Pick<TagWriteReport, 'written' | 'failed'>): string {
  if (report.written > 0) return `Wrote tags to ${plural(report.written, 'file', 'files')}`;
  if (report.failed.length > 0) {
    return `No tags were written (${plural(report.failed.length, 'file failed', 'files failed')})`;
  }
  return '';
}

export function formatUrlLookupNotice(result: TrackIdentification): string {
  if (result.best) {
    return `Loaded match from ${formatProviderName(result.best.provider)}`;
  }
  for (let i = result.steps.length - 1; i >= 0; i--) {
    const step = result.steps[i];
    if (step?.outcome.kind === 'failed') {
      return step.outcome.reason || 'Lookup failed at that release link';
    }
  }
  return 'No match found at that release link';
}
