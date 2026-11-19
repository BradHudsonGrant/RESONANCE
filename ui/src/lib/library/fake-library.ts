import { DEFAULT_TABLE_LAYOUT } from '../ipc/bindings/DefaultTableLayout';
import type { LibraryTableLayout } from '../ipc/bindings/LibraryTableLayout';
import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import type { TableSortKey } from '../ipc/bindings/TableSortKey';
import type { TrackSortField } from '../ipc/bindings/TrackSortField';
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
