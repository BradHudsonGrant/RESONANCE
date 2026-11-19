import { DEFAULT_TABLE_LAYOUT } from '../ipc/bindings/DefaultTableLayout';
import type { DescribedFilter } from '../ipc/bindings/DescribedFilter';
import type { FilterField } from '../ipc/bindings/FilterField';
import type { FilterTerm } from '../ipc/bindings/FilterTerm';
import type { QuickFilters } from '../ipc/bindings/QuickFilters';
import type { SmartView } from '../ipc/bindings/SmartView';
import type { LibraryTableLayout } from '../ipc/bindings/LibraryTableLayout';
import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import type { TableSortKey } from '../ipc/bindings/TableSortKey';
import type { TrackSortField } from '../ipc/bindings/TrackSortField';
import { FilterState, type FiltersClient } from './filters.svelte';
import { TableLayoutState } from './table-layout.svelte';
import { type ReadPage, TrackPages } from './track-pages.svelte';

export function fakeTrack(index: number): LibraryTrack {
  return {
    id: index + 1,
    title: `Track ${index}`,
    artist: 'Ada Marsh',
    album: null,
    album_artist: null,
    genre: 'House',
    year: '2024',
    track_number: null,
    label: null,
    comment: null,
    mix: null,
    subgenre: null,
    catno: null,
    composer: null,
    remixer: null,
    bpm: 124,
    key: { camelot: '8A', open_key: '1m', name: 'Am' },
    key_tag: 'Am',
    format: 'flac',
    favourite: false,
    location: `/music/${index}.flac`,
    size_bytes: 9_000_000,
    missing_since: null,
    artwork: null,
    added_ms: 1_700_000_000_000 + index,
    modified_ms: 1_700_000_000_000,
    audio: {
      duration_ms: 301_000,
      sample_rate: 44_100,
      channels: 2,
      bit_depth: 16,
      bitrate_kbps: 900,
    },
    collections: [],
  };
}

/** A library of `total` made-up tracks, and the offset of every page read from it. */
export function fakeLibrary(total: number) {
  const asked: number[] = [];
  const read: ReadPage = async (offset, limit) => {
    asked.push(offset);
    const count = Math.max(0, Math.min(limit, total - offset));
    return {
      total,
      library_total: total,
      offset,
      tracks: Array.from({ length: count }, (_, at) => fakeTrack(offset + at)),
    };
  };
  return { asked, pages: new TrackPages(read) };
}

/** Lets every page read that has started arrive. */
export function settled(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

const defaultFields: Record<string, TrackSortField> = {
  index: 'index',
  title: 'title',
  artist: 'artist',
  bpm: 'bpm',
  key: 'key',
  time: 'duration',
};

/**
 * A layout source for tests that answers header clicks the way the core does for one sort key:
 * up, then down, then library order.
 */
export function fakeTableLayout(sort: TableSortKey[] = []): TableLayoutState {
  let current: LibraryTableLayout = { ...DEFAULT_TABLE_LAYOUT, sort };
  return new TableLayoutState(
    async () => current,
    async (change) => {
      if (change.kind === 'sort') {
        const now = current.sort[0];
        const direction =
          now?.column !== change.column ? 'asc' : now.direction === 'asc' ? 'desc' : null;
        const field = defaultFields[change.column] ?? 'index';
        current = {
          ...current,
          sort: direction ? [{ column: change.column, field, direction }] : [],
        };
      }
      return current;
    },
  );
}

/** A few of the core's filter columns, as `filter_fields` lists them. */
export const FAKE_FILTER_FIELDS: FilterField[] = [
  {
    column: 'genre',
    label: 'Genre',
    noun: 'genre',
    group: 'release',
    kind: 'text',
    conditions: [
      'contains',
      'is',
      'is_not',
      'starts_with',
      'any_of',
      'none_of',
      'missing',
      'present',
    ],
    choices: [],
  },
  {
    column: 'bitrate',
    label: 'Bitrate',
    noun: 'bitrate',
    group: 'audio',
    kind: 'number',
    conditions: ['range', 'below', 'above', 'missing', 'present'],
    choices: [],
  },
  {
    column: 'key',
    label: 'Key',
    noun: 'key',
    group: 'track',
    kind: 'choice',
    conditions: ['any_of', 'none_of', 'missing', 'present'],
    choices: ['8A', '9A'].map((value) => ({ value, label: value })),
  },
  {
    column: 'artwork',
    label: 'Artwork',
    noun: 'artwork',
    group: 'release',
    kind: 'picture',
    conditions: ['missing', 'present'],
    choices: [],
  },
];

/** A term's words, roughly as the core puts them, for tests that do not run the core. */
function fakeLabel(term: FilterTerm): string {
  const [filter] = term.any_of;
  if (!filter) return '';
  const condition = filter.condition;
  if (condition.op === 'missing') return `No ${filter.column}`;
  if ('value' in condition) return `${filter.column} ${condition.op} ${condition.value}`;
  if ('values' in condition)
    return `${filter.column} ${condition.op} ${condition.values.join(', ')}`;
  return `${filter.column} ${condition.op}`;
}

export function described(term: FilterTerm): DescribedFilter {
  return { term, label: fakeLabel(term), key: JSON.stringify(term) };
}

export const MISSING_GENRE: FilterTerm = {
  any_of: [{ column: 'genre', condition: { op: 'missing' } }],
};

/** Quick filters kept in memory the way the core keeps them, and every call made. */
export function fakeFilters() {
  const calls: { name: string; arg?: unknown }[] = [];
  let quick: QuickFilters = {
    pinned: [],
    recommended: [described(MISSING_GENRE)],
    recommend: true,
  };
  let views: SmartView[] = [];
  const client: FiltersClient = {
    fields: async () => FAKE_FILTER_FIELDS,
    summaries: async (query) => {
      calls.push({ name: 'summaries', arg: query });
      const terms = query.filters ?? [];
      return terms.map((term, at) => ({
        label: fakeLabel(term),
        key: JSON.stringify(term),
        count: terms.length > 1 ? at + 1 : null,
      }));
    },
    parse: async (text) => {
      calls.push({ name: 'parse', arg: text });
      if (text.trim() === 'missing genre') return described(MISSING_GENRE);
      throw new Error('That is not a filter.');
    },
    recordUse: async (term) => {
      calls.push({ name: 'recordUse', arg: term });
      return quick;
    },
    quickFilters: async () => quick,
    changeQuickFilters: async (change) => {
      calls.push({ name: 'change', arg: change });
      if (change.kind === 'pin') {
        quick = {
          ...quick,
          pinned: [...quick.pinned, described(change.term)],
          recommended: quick.recommended.filter(
            (item) => JSON.stringify(item.term) !== JSON.stringify(change.term),
          ),
        };
      } else if (change.kind === 'remove') {
        const key = JSON.stringify(change.term);
        quick = {
          ...quick,
          pinned: quick.pinned.filter((item) => item.key !== key),
          recommended: quick.recommended.filter((item) => item.key !== key),
        };
      } else if (change.kind === 'set_recommend') {
        quick = { ...quick, recommend: change.on, recommended: change.on ? quick.recommended : [] };
      }
      return quick;
    },
    smartViews: async () => views,
    saveView: async (name, query) => {
      calls.push({ name: 'saveView', arg: { name, query } });
      views = [...views, { id: views.length + 1, name, query }];
      return views;
    },
    renameView: async () => views,
    deleteView: async () => views,
  };
  return { calls, filters: new FilterState(client) };
}
