import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import { duration, tempo } from './format';

export interface DataColumn {
  key: 'title' | 'artist' | 'bpm' | 'key' | 'time';
  label: string;
  track: string;
  numeric?: boolean;
  mono?: boolean;
  tone?: 'primary' | 'secondary' | 'muted';
  weight?: 'regular' | 'medium';
  text: (track: LibraryTrack) => string;
}

/** Container-width drop thresholds for M5 analysis columns (docs/design/library.md). */
export const ENERGY_HIDE_BELOW_PX = 820;
export const WAVEFORM_NARROW_BELOW_PX = 740;
export const WAVEFORM_HIDE_BELOW_PX = 640;

/** Container width below which the row index and row-actions columns fold away. */
export const COMPACT_TABLE_BELOW_PX = 580;

export const dataColumns: DataColumn[] = [
  {
    key: 'title',
    label: 'Title',
    track: 'minmax(150px, 1.8fr)',
    weight: 'medium',
    text: (track) => track.title,
  },
  {
    key: 'artist',
    label: 'Artist',
    track: 'minmax(120px, 1.3fr)',
    tone: 'secondary',
    text: (track) => track.artist ?? '',
  },
  {
    key: 'bpm',
    label: 'BPM',
    track: '58px',
    numeric: true,
    text: (track) => tempo(track.bpm),
  },
  {
    key: 'key',
    label: 'Key',
    track: '52px',
    mono: true,
    text: (track) => track.key?.camelot ?? track.key_tag ?? '',
  },
  {
    key: 'time',
    label: 'Time',
    track: '56px',
    numeric: true,
    tone: 'secondary',
    text: (track) => duration(track.audio?.duration_ms),
  },
];

/** Tick box (30) + Status (22) + # (36) + data columns + Actions (32). */
export const columnWidths = `30px 22px 36px ${dataColumns.map((col) => col.track).join(' ')} 32px`;

/** Below COMPACT_TABLE_BELOW_PX, # and Actions fold away so narrow workspaces never scroll sideways. */
export const compactColumnWidths = [
  '30px',
  '22px',
  'minmax(136px, 1.8fr)',
  'minmax(108px, 1.3fr)',
  '58px',
  '52px',
  '56px',
].join(' ');

export const tableMinWidth = '462px';
