import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import { duration, tempo } from './format';

export interface Column {
  key: string;
  label: string;
  /** Set in the number face and to the right, so the digits line up down the column. */
  numeric: boolean;
  /** The narrowest the column goes, in rem. */
  min: number;
  /** Its share of any width left over. A column with none stays at its narrowest. */
  grow: number;
  text: (track: LibraryTrack) => string;
}

export const columns: Column[] = [
  { key: 'title', label: 'Title', numeric: false, min: 12, grow: 3, text: (track) => track.title },
  {
    key: 'artist',
    label: 'Artist',
    numeric: false,
    min: 9,
    grow: 2,
    text: (track) => track.artist ?? '',
  },
  { key: 'bpm', label: 'BPM', numeric: true, min: 4.5, grow: 0, text: (track) => tempo(track.bpm) },
  {
    key: 'key',
    label: 'Key',
    numeric: true,
    min: 3.5,
    grow: 0,
    text: (track) => track.key?.camelot ?? '',
  },
  {
    key: 'time',
    label: 'Time',
    numeric: true,
    min: 4.5,
    grow: 0,
    text: (track) => duration(track.audio?.duration_ms),
  },
  {
    key: 'genre',
    label: 'Genre',
    numeric: false,
    min: 7,
    grow: 1,
    text: (track) => track.genre ?? '',
  },
  {
    key: 'album',
    label: 'Album',
    numeric: false,
    min: 8,
    grow: 1.5,
    text: (track) => track.album ?? '',
  },
  {
    key: 'format',
    label: 'Format',
    numeric: false,
    min: 4.5,
    grow: 0,
    text: (track) => track.format.toUpperCase(),
  },
];

export const columnWidths = columns
  .map(({ min, grow }) => (grow > 0 ? `minmax(${min}rem, ${grow}fr)` : `${min}rem`))
  .join(' ');

/** The row's selection border and the columns at their narrowest. Past this the table scrolls. */
export const tableWidth = `calc(2px + ${columns.reduce((sum, { min }) => sum + min, 0)}rem)`;
