import type { FilterCondition } from '../ipc/bindings/FilterCondition';
import type { FilterField } from '../ipc/bindings/FilterField';
import type { FilterTerm } from '../ipc/bindings/FilterTerm';
import type { LibraryTableLayout } from '../ipc/bindings/LibraryTableLayout';
import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import type { TableColumn } from '../ipc/bindings/TableColumn';
import { duration, fileSize, sampleRate, tempo } from './format';
import { keyCode } from './key-notation.svelte';

/** How a column's cells are set. Which columns exist, and their widths, come from the core. */
export interface CellLook {
  numeric?: boolean;
  mono?: boolean;
  tone?: 'primary' | 'secondary' | 'muted';
  weight?: 'regular' | 'medium';
}

/** Tick box (30) + Status (22) + # (36) before the data columns, and Actions (56) after. */
export const LEADING_PX = 88;
export const ACTIONS_PX = 56;

/** Container width below which Title and Artist narrow their floor while keeping every column. */
export const COMPACT_TABLE_BELOW_PX = 600;

const COMPACT_FLOOR_PX: Record<string, number> = { title: 96, artist: 68 };
const SHARE: Record<string, number> = { title: 1.8, artist: 1.3 };

const looks: Record<string, CellLook> = {
  title: { weight: 'medium' },
  key: { mono: true },
  catno: { mono: true, tone: 'secondary' },
  format: { mono: true, tone: 'secondary' },
};

export function cellLook(column: TableColumn): CellLook {
  return looks[column.id] ?? { numeric: column.numeric, tone: 'secondary' };
}

function date(ms: number): string {
  if (ms <= 0) return '';
  const day = new Date(ms);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${day.getFullYear()}-${pad(day.getMonth() + 1)}-${pad(day.getDate())}`;
}

function folder(location: string): string {
  const cut = Math.max(location.lastIndexOf('/'), location.lastIndexOf('\\'));
  return cut > 0 ? location.slice(0, cut) : '';
}

function channels(count: number | null | undefined): string {
  if (count == null) return '';
  if (count === 1) return 'Mono';
  if (count === 2) return 'Stereo';
  return String(count);
}

const text: Record<string, (track: LibraryTrack) => string> = {
  title: (track) => track.title,
  artist: (track) => track.artist ?? '',
  bpm: (track) => tempo(track.bpm),
  key: (track) => (track.key ? keyCode(track.key) : (track.key_tag ?? '')),
  time: (track) => duration(track.audio?.duration_ms),
  composer: (track) => track.composer ?? '',
  remixer: (track) => track.remixer ?? '',
  mix: (track) => track.mix ?? '',
  comment: (track) => track.comment ?? '',
  album: (track) => track.album ?? '',
  genre: (track) => track.genre ?? '',
  label: (track) => track.label ?? '',
  catno: (track) => track.catno ?? '',
  year: (track) => track.year ?? '',
  artwork: () => '',
  format: (track) => track.format.toUpperCase(),
  bitrate: (track) => (track.audio?.bitrate_kbps ? `${track.audio.bitrate_kbps} kbps` : ''),
  sample_rate: (track) => (track.audio?.sample_rate ? sampleRate(track.audio.sample_rate) : ''),
  bit_depth: (track) => (track.audio?.bit_depth ? `${track.audio.bit_depth}-bit` : ''),
  channels: (track) => channels(track.audio?.channels),
  quality: (track) => track.quality?.label ?? '',
  file_size: (track) => fileSize(track.size_bytes),
  added: (track) => date(track.added_ms),
  modified: (track) => date(track.modified_ms),
  path: (track) => track.location,
  folder: (track) => folder(track.location),
  'tag:album_artist': (track) => track.album_artist ?? '',
  'tag:track_number': (track) => track.track_number ?? '',
  'tag:subgenre': (track) => track.subgenre ?? '',
};

/** The value a cell shows, as plain text. A column this build cannot read shows nothing. */
export function cellText(column: TableColumn, track: LibraryTrack): string {
  return text[column.id]?.(track) ?? '';
}

/** A cell's value as a filter compares it, for the columns whose values can be picked out. */
const raw: Record<string, (track: LibraryTrack) => string | null | undefined> = {
  title: (track) => track.title,
  artist: (track) => track.artist,
  composer: (track) => track.composer,
  remixer: (track) => track.remixer,
  mix: (track) => track.mix,
  comment: (track) => track.comment,
  album: (track) => track.album,
  genre: (track) => track.genre,
  label: (track) => track.label,
  catno: (track) => track.catno,
  key: (track) => track.key?.camelot,
  format: (track) => track.format,
  quality: (track) => track.quality?.kind,
  'tag:album_artist': (track) => track.album_artist,
  'tag:track_number': (track) => track.track_number,
  'tag:subgenre': (track) => track.subgenre,
};

/** Whether a cell has a value for "is missing", by what it shows. */
const has: Record<string, (track: LibraryTrack) => boolean> = {
  bpm: (track) => track.bpm != null,
  time: (track) => track.audio?.duration_ms != null,
  year: (track) => Boolean(track.year),
  artwork: (track) => Boolean(track.artwork),
  bitrate: (track) => track.audio?.bitrate_kbps != null,
  sample_rate: (track) => track.audio?.sample_rate != null,
  bit_depth: (track) => track.audio?.bit_depth != null,
  channels: (track) => track.audio?.channels != null,
};

/** The filters a right click on a cell offers: this value, not this value, or no value. */
export function cellFilters(
  field: FilterField,
  track: LibraryTrack,
): { label: string; term: FilterTerm }[] {
  const one = (condition: FilterCondition): FilterTerm => ({
    any_of: [{ column: field.column, condition }],
  });
  const value = raw[field.column]?.(track)?.trim();
  const choice = field.kind === 'choice';
  const offers: { label: string; term: FilterTerm }[] = [];
  if (value && field.conditions.includes(choice ? 'any_of' : 'is')) {
    offers.push(
      {
        label: `Only tracks with this ${field.noun}`,
        term: one(choice ? { op: 'any_of', values: [value] } : { op: 'is', value }),
      },
      {
        label: `Hide this ${field.noun}`,
        term: one(choice ? { op: 'none_of', values: [value] } : { op: 'is_not', value }),
      },
    );
  }
  const present = value ? true : (has[field.column]?.(track) ?? false);
  if (!present && field.conditions.includes('missing')) {
    offers.push({ label: `Only tracks with no ${field.noun}`, term: one({ op: 'missing' }) });
  }
  if (present && field.kind === 'picture') {
    offers.push({ label: `Only tracks with ${field.noun}`, term: one({ op: 'present' }) });
  }
  return offers;
}

/** The width a column takes on its own: the user's, else its default, else its floor. */
export function columnWidth(column: TableColumn, live?: number, compact = false): number {
  const floor = compact
    ? (COMPACT_FLOOR_PX[column.id] ?? column.floor_width_px)
    : column.floor_width_px;
  return live ?? column.width_px ?? column.default_width_px ?? floor;
}

function track(column: TableColumn, compact: boolean, live?: number): string {
  const set = live ?? column.width_px;
  if (set != null) return `${set}px`;
  if (column.default_width_px != null) return `${column.default_width_px}px`;
  const floor = compact
    ? (COMPACT_FLOOR_PX[column.id] ?? column.floor_width_px)
    : column.floor_width_px;
  return `minmax(${floor}px, ${SHARE[column.id] ?? 1}fr)`;
}

export function gridTemplate(
  columns: TableColumn[],
  compact: boolean,
  live: Record<string, number> = {},
): string {
  const tracks = columns.map((column) => track(column, compact, live[column.id]));
  return `30px 22px 36px ${tracks.join(' ')} ${ACTIONS_PX}px`;
}

export function tableWidth(
  columns: TableColumn[],
  live: Record<string, number> = {},
  compact = false,
): number {
  return columns.reduce(
    (sum, column) => sum + columnWidth(column, live[column.id], compact),
    LEADING_PX + ACTIONS_PX,
  );
}

/**
 * The columns that fit in `width`, giving way in the core's drop order: the last picked first.
 * The default columns never drop, so a table narrower than them scrolls instead.
 */
export function fitColumns(
  layout: LibraryTableLayout,
  width: number,
  live: Record<string, number> = {},
): { shown: TableColumn[]; hidden: TableColumn[] } {
  let shown = layout.columns;
  const hidden: TableColumn[] = [];
  if (width <= 0) return { shown, hidden };
  for (const id of layout.drop_order) {
    if (tableWidth(shown, live) <= width) break;
    const column = shown.find((candidate) => candidate.id === id);
    if (!column) continue;
    hidden.push(column);
    shown = shown.filter((candidate) => candidate.id !== id);
  }
  return { shown, hidden };
}
