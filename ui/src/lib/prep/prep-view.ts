import type { AcceptedTagWriteReport } from '../ipc/bindings/AcceptedTagWriteReport';
import type { PrepItem } from '../ipc/bindings/PrepItem';
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

export function missingNames(item: PrepItem): string {
  const names = new Set<string>(
    item.fields.filter((field) => !field.current_value?.trim()).map((field) => field.field),
  );
  for (const field of ['album_artist', 'composer', 'remixer'] as const) {
    if (!item.fields.some((entry) => entry.field === field) && !item.track[field]?.trim()) {
      names.add(field);
    }
  }
  return MISSING_FIELDS.filter((field) =>
    field === 'artwork' ? !item.artwork.artwork : names.has(field),
  )
    .map((field) => MISSING_LABELS[field])
    .join(', ');
}

export function uniqueProviders(providers: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  for (const provider of providers) {
    if (provider) seen.add(formatProviderName(provider));
  }
  return [...seen];
}

export function resolveActiveCoverIndex(
  item: PrepItem,
  chosenCover: number | undefined,
  covers: readonly PrepItem['candidates'][number][],
): number | null {
  const cover = item.artwork.proposed;
  if (!cover) return null;
  if (chosenCover !== undefined && covers.some((candidate) => candidate.index === chosenCover)) {
    return chosenCover;
  }
  const selected = item.candidates.find((candidate) => candidate.index === item.selected_candidate);
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

export function formatSaveNotice(
  report: Pick<AcceptedTagWriteReport, 'written' | 'library_fields' | 'reverted_fields' | 'failed'>,
): string {
  const parts: string[] = [];
  if (report.written > 0) {
    parts.push(
      `Saved to ${report.written.toLocaleString('en-GB')} ${
        report.written === 1 ? 'file' : 'files'
      }`,
    );
  }
  if (report.library_fields > 0) {
    parts.push(
      `${report.library_fields.toLocaleString('en-GB')} ${
        report.library_fields === 1 ? 'field' : 'fields'
      } saved to library`,
    );
  }
  if (report.reverted_fields > 0) {
    parts.push(
      `${report.reverted_fields.toLocaleString('en-GB')} ${
        report.reverted_fields === 1 ? 'tag' : 'tags'
      } reverted to file`,
    );
  }
  if (parts.length > 0) return parts.join(' · ');
  return report.failed.length ? 'Save failed' : 'No pending changes';
}

export function formatWriteNotice(report: Pick<TagWriteReport, 'written' | 'failed'>): string {
  if (report.written > 0) {
    return `Wrote tags to ${report.written.toLocaleString('en-GB')} ${
      report.written === 1 ? 'file' : 'files'
    }`;
  }
  if (report.failed.length > 0) {
    return `No tags were written (${report.failed.length.toLocaleString('en-GB')} ${
      report.failed.length === 1 ? 'file failed' : 'files failed'
    })`;
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
