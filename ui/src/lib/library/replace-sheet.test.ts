// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { FfmpegStatus } from '../ipc/bindings/FfmpegStatus';
import type { ReplaceComparison } from '../ipc/bindings/ReplaceComparison';
import type { ReplaceFileFacts } from '../ipc/bindings/ReplaceFileFacts';
import type { ReplaceResult } from '../ipc/bindings/ReplaceResult';
import { fakeTrack } from './fake-library';
import Inspector from './Inspector.svelte';
import Library from './Library.svelte';
import ReplaceSheet, { type ReplaceClient } from './ReplaceSheet.svelte';
import { TrackPages } from './track-pages.svelte';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.open = false;
};
HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

const mp3: ReplaceFileFacts = {
  path: 'C:/music/night drive 128.mp3',
  format: 'mp3',
  bitrate_kbps: 128,
  sample_rate: 44_100,
  bit_depth: null,
  duration_ms: 75_000,
  integrated_lufs: -9.84,
  true_peak_dbtp: 0.21,
  spectral_cutoff_hz: 16_050,
  flags: [],
};

const flac: ReplaceFileFacts = {
  ...mp3,
  path: 'C:/incoming/Night Drive.flac',
  format: 'flac',
  bitrate_kbps: 912,
  bit_depth: 16,
  spectral_cutoff_hz: null,
};

const better: ReplaceComparison = {
  track_id: 3,
  old: mp3,
  new: flac,
  relation: 'Another encode',
  fingerprint_similarity: 0.94,
  offset_ms: 47.8,
  tempo_ratio: 1,
  verdict: 'same_recording_and_better',
  verdict_label: 'Same recording and better',
  default_choice: 'replace',
  replace_name: 'Test Artist - Night Drive.flac',
  separate_name: 'Night Drive.flac',
  cues_to_carry: 3,
};

const replaced: ReplaceResult = {
  track_id: 3,
  operation: {
    id: 9,
    kind: 'replace_file',
    state: 'applied',
    summary: 'Replaced 1 file',
    can_undo: true,
    created_ms: Date.UTC(2026, 10, 25, 9, 30),
    applied_ms: Date.UTC(2026, 10, 25, 9, 30),
    undone_ms: null,
    recovery: null,
  },
};

const found: FfmpegStatus = {
  program: 'C:/tools/ffmpeg.exe',
  version: 'ffmpeg version 8.1',
  presets: [
    { preset: 'mp3_cbr_320', label: 'MP3 320 kbps CBR, true peak held under -1 dBTP' },
    { preset: 'flac_24', label: 'FLAC 24-bit lossless' },
  ],
};

function client(overrides: Partial<ReplaceClient> = {}): ReplaceClient {
  return {
    compare: vi.fn(() => Promise.resolve(better)),
    apply: vi.fn(() => Promise.resolve(replaced)),
    ffmpegStatus: vi.fn(() => Promise.resolve(found)),
    convert: vi.fn(() => Promise.resolve({ path: 'D:/data/converted/x.mp3', similarity: 0.99 })),
    repair: vi.fn(() => Promise.resolve({ path: 'D:/data/converted/x.m4a', similarity: 1 })),
    ...overrides,
  };
}

describe('ReplaceSheet', () => {
  it('compares a dropped file and replaces the track with it', async () => {
    const replace = client();
    const onreplaced = vi.fn();
    render(ReplaceSheet, {
      open: true,
      track: { ...fakeTrack(3), id: 3 },
      path: 'C:/incoming/Night Drive.flac',
      client: replace,
      onreplaced,
    });

    const sheet = screen.getByRole('dialog', { name: 'Replace file' });
    expect(await within(sheet).findByText('Same recording and better')).toBeTruthy();
    expect(replace.compare).toHaveBeenCalledWith(
      3,
      'C:/incoming/Night Drive.flac',
      expect.any(String),
    );
    const table = within(sheet).getByRole('table');
    expect(within(table).getByText('128 kbps')).toBeTruthy();
    expect(within(table).getByText('912 kbps')).toBeTruthy();
    expect(within(table).getByText('16.05 kHz')).toBeTruthy();
    expect(within(sheet).getByText('47.80 ms')).toBeTruthy();
    expect(within(sheet).getByText('94%')).toBeTruthy();
    expect(within(sheet).getByText('Test Artist - Night Drive.flac')).toBeTruthy();
    expect(within(sheet).getByText(/3 cues move onto the new file/)).toBeTruthy();

    await fireEvent.click(within(sheet).getByRole('button', { name: 'Replace' }));
    await waitFor(() => expect(onreplaced).toHaveBeenCalledWith(replaced));
    expect(replace.apply).toHaveBeenCalledWith(
      3,
      'C:/incoming/Night Drive.flac',
      expect.any(String),
      'replace',
    );
  });

  it('defaults a different version to adding a separate track', async () => {
    const version: ReplaceComparison = {
      ...better,
      relation: 'Different length',
      verdict: 'different_version',
      verdict_label: 'Different version',
      default_choice: 'add_separate_track',
      offset_ms: null,
      tempo_ratio: null,
    };
    const replace = client({ compare: vi.fn(() => Promise.resolve(version)) });
    render(ReplaceSheet, {
      open: true,
      track: { ...fakeTrack(3), id: 3 },
      client: replace,
    });

    const sheet = screen.getByRole('dialog', { name: 'Replace file' });
    await fireEvent.input(within(sheet).getByRole('textbox', { name: 'New file path' }), {
      target: { value: 'C:/incoming/Night Drive (Radio Edit).flac' },
    });
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Compare' }));
    expect(await within(sheet).findByText('Different version')).toBeTruthy();
    expect(within(sheet).getByText('Not lined up')).toBeTruthy();
    expect(within(sheet).getByRole('tab', { name: 'Add as a separate track' })).toBeTruthy();

    await fireEvent.click(within(sheet).getByRole('button', { name: 'Add track' }));
    await waitFor(() =>
      expect(replace.apply).toHaveBeenCalledWith(
        3,
        'C:/incoming/Night Drive (Radio Edit).flac',
        expect.any(String),
        'add_separate_track',
      ),
    );
  });

  it('converts the track file and compares the result', async () => {
    const replace = client();
    render(ReplaceSheet, { open: true, track: { ...fakeTrack(3), id: 3 }, client: replace });

    const sheet = screen.getByRole('dialog', { name: 'Replace file' });
    await fireEvent.click(await within(sheet).findByRole('button', { name: 'Convert' }));
    await waitFor(() => expect(replace.convert).toHaveBeenCalledWith(3, 'mp3_cbr_320'));
    await waitFor(() =>
      expect(replace.compare).toHaveBeenCalledWith(
        3,
        'D:/data/converted/x.mp3',
        expect.any(String),
      ),
    );
  });

  it('says when FFmpeg is missing', async () => {
    const replace = client({
      ffmpegStatus: vi.fn(() => Promise.resolve({ program: null, version: null, presets: [] })),
    });
    render(ReplaceSheet, { open: true, track: { ...fakeTrack(3), id: 3 }, client: replace });

    expect(await screen.findByText(/Convert and Repair need FFmpeg/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Convert' })).toBeNull();
  });
});

describe('Replace file entry points', () => {
  it('opens from the row menu and the Inspector, but not for a missing file', async () => {
    const present = fakeTrack(1);
    const missing = { ...fakeTrack(2), missing_since: 1_760_000_000_000 };
    const pages = new TrackPages(() =>
      Promise.resolve({ total: 2, library_total: 2, offset: 0, tracks: [present, missing] }),
    );
    await pages.load();
    const onreplace = vi.fn();
    const library = render(Library, {
      pages,
      onscan: vi.fn(),
      onreplace,
      loadFacets: vi.fn().mockResolvedValue({
        bpm_min: null,
        bpm_max: null,
        year_min: null,
        year_max: null,
        labels: [],
        missing_count: 1,
      }),
    });

    await fireEvent.click(screen.getByRole('button', { name: `Actions for ${present.title}` }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Replace file…' }));
    expect(onreplace).toHaveBeenCalledWith(expect.objectContaining({ id: present.id }));

    await fireEvent.click(screen.getByRole('button', { name: `Actions for ${missing.title}` }));
    expect(screen.queryByRole('menuitem', { name: 'Replace file…' })).toBeNull();
    library.unmount();

    render(Inspector, { track: present, onclose: vi.fn(), onreplace });
    await fireEvent.click(screen.getByRole('button', { name: 'Replace file…' }));
    expect(onreplace).toHaveBeenCalledTimes(2);
  });
});
