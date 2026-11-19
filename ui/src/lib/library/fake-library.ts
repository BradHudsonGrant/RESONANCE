import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
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
    bpm: 124,
    key: { camelot: '8A', open_key: '1m', name: 'Am' },
    key_tag: 'Am',
    format: 'flac',
    favourite: false,
    location: `/music/${index}.flac`,
    size_bytes: 9_000_000,
    missing_since: null,
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
