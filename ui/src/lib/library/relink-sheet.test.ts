// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import type { RelinkCandidate } from '../ipc/bindings/RelinkCandidate';
import type { RelinkPreview } from '../ipc/bindings/RelinkPreview';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';
import { fakeTrack } from './fake-library';
import Inspector from './Inspector.svelte';
import Library from './Library.svelte';
import RelinkSheet, { type RelinkClient } from './RelinkSheet.svelte';
import { TrackPages } from './track-pages.svelte';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

function missingTrack(index = 0): LibraryTrack {
  return {
    ...fakeTrack(index),
    missing_since: 1_760_000_000_000,
    favourite: true,
    collections: [{ id: 1, name: 'Warm Up' }],
  };
}

describe('Missing track indicators and RelinkSheet', () => {
  it('flags missing tracks in the table, row menu, filter drawer and inspector without enabling playback', async () => {
    const queries: TrackQuery[] = [];
    const missing = missingTrack(0);
    const present = fakeTrack(1);
    const pages = new TrackPages((_offset, _limit, query) => {
      if (query) queries.push(query);
      const tracks = query?.missing_only ? [missing] : [missing, present];
      return Promise.resolve({
        total: tracks.length,
        library_total: 2,
        offset: 0,
        tracks,
      });
    });
    await pages.load();

    const onrelink = vi.fn();
    const onpreview = vi.fn();
    const loadFacets = vi.fn().mockResolvedValue({
      bpm_min: 124,
      bpm_max: 124,
      year_min: 2024,
      year_max: 2024,
      labels: [],
      missing_count: 1,
    });

    const { unmount } = render(Library, {
      pages,
      onscan: vi.fn(),
      onrelink,
      onpreview,
      loadFacets,
    });

    const statusBtn = screen.getByRole('button', {
      name: `Missing file for ${missing.title} — Relink`,
    });
    await fireEvent.click(statusBtn);
    expect(onrelink).toHaveBeenCalledWith(expect.objectContaining({ id: missing.id }));
    expect(onpreview).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole('button', { name: `Actions for ${missing.title}` }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Relink missing file…' }));
    expect(onrelink).toHaveBeenCalledTimes(2);

    await fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    const drawer = screen.getByRole('dialog', { name: 'Filters' });
    await waitFor(() => expect(within(drawer).getByText('1 missing')).toBeTruthy());
    await fireEvent.click(within(drawer).getByRole('checkbox', { name: 'Missing files only' }));
    expect(queries.at(-1)?.missing_only).toBe(true);
    await fireEvent.click(within(drawer).getByRole('button', { name: 'Clear' }));
    expect(queries.at(-1)?.missing_only).toBeNull();
    unmount();

    const inspectorRelink = vi.fn();
    const inspectorPreview = vi.fn();
    render(Inspector, {
      track: missing,
      onclose: vi.fn(),
      onrelink: inspectorRelink,
      onpreview: inspectorPreview,
    });

    const aside = screen.getByRole('complementary', { name: 'Inspector' });
    expect(within(aside).getByText('Missing file')).toBeTruthy();
    const playBtn = within(aside).getByRole('button', {
      name: `Play ${missing.title}`,
    }) as HTMLButtonElement;
    expect(playBtn.disabled).toBe(true);
    await fireEvent.click(within(aside).getByRole('button', { name: 'Relink…' }));
    expect(inspectorRelink).toHaveBeenCalledWith(missing);
  });

  it('lists ranked audio-matched candidates, searches another folder and relinks in one click', async () => {
    const target = missingTrack(2);
    const ranked: RelinkCandidate[] = [
      {
        candidate_path: 'D:\\Music\\Archive\\Track 2.flac',
        existing_track_id: null,
        evidence: { kind: 'exact_bytes' },
        file: {
          path: 'D:\\Music\\Archive\\Track 2.flac',
          format: 'flac',
          size_bytes: 9_000_000,
          duration_ms: 301_000,
          sample_rate: 44_100,
          bit_depth: 16,
          bitrate_kbps: 900,
        },
      },
      {
        candidate_path: 'D:\\Music\\Portable\\Track 2.mp3',
        existing_track_id: 9,
        evidence: { kind: 'same_recording', similarity: 0.93 },
        file: {
          path: 'D:\\Music\\Portable\\Track 2.mp3',
          format: 'mp3',
          size_bytes: 2_400_000,
          duration_ms: 301_100,
          sample_rate: 44_100,
          bit_depth: null,
          bitrate_kbps: 320,
        },
      },
    ];

    const updated: LibraryTrack = {
      ...target,
      location: ranked[0]!.candidate_path,
      missing_since: null,
    };

    const client: RelinkClient = {
      candidates: vi.fn(async () => ranked),
      preview: vi.fn(),
      apply: vi.fn(async () => updated),
      pickFolder: vi.fn(async () => 'D:\\Music\\Archive'),
    };
    const onrelinked = vi.fn();

    render(RelinkSheet, {
      open: true,
      track: target,
      client,
      onrelinked,
    });

    await waitFor(() => expect(screen.getByText('Exact file copy')).toBeTruthy());
    expect(screen.getByText('Same recording · 93% match')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Browse folder…' }));
    await waitFor(() =>
      expect(client.candidates).toHaveBeenLastCalledWith(target.id, 'D:\\Music\\Archive'),
    );

    await fireEvent.click(
      screen.getByRole('button', { name: `Relink to ${ranked[0]!.candidate_path}` }),
    );
    await waitFor(() =>
      expect(client.apply).toHaveBeenCalledWith(target.id, ranked[0]!.candidate_path, false),
    );
    expect(onrelinked).toHaveBeenCalledWith(updated);
  });

  it('shows before/after comparison and requires confirmation before replacing with different audio', async () => {
    const target = missingTrack(5);
    const mismatchPreview: RelinkPreview = {
      track_id: target.id,
      evidence: { kind: 'different_recording', similarity: 0.41 },
      requires_confirmation: true,
      merges_existing_track_id: 14,
      current: {
        path: target.location,
        format: 'flac',
        size_bytes: 9_000_000,
        duration_ms: 301_000,
        sample_rate: 44_100,
        bit_depth: 16,
        bitrate_kbps: 900,
      },
      candidate: {
        path: 'D:\\Music\\Other\\Different Track.mp3',
        format: 'mp3',
        size_bytes: 3_100_000,
        duration_ms: 215_000,
        sample_rate: 44_100,
        bit_depth: null,
        bitrate_kbps: 320,
      },
    };

    const updated: LibraryTrack = {
      ...target,
      format: 'mp3',
      location: mismatchPreview.candidate.path,
      missing_since: null,
    };

    const client: RelinkClient = {
      candidates: vi.fn(async () => []),
      preview: vi.fn(async () => mismatchPreview),
      apply: vi.fn(async () => updated),
      pickAudioFile: vi.fn(async () => mismatchPreview.candidate.path),
    };
    const onrelinked = vi.fn();

    render(RelinkSheet, {
      open: true,
      track: target,
      client,
      onrelinked,
    });

    await waitFor(() =>
      expect(screen.getByText('No matching copies found in your library.')).toBeTruthy(),
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Browse file…' }));
    await waitFor(() =>
      expect(client.preview).toHaveBeenCalledWith(target.id, mismatchPreview.candidate.path),
    );

    const comparison = screen.getByLabelText('Before and after file comparison');
    expect(comparison.textContent).toContain('/music/5.flac');
    expect(comparison.textContent).toContain('D:\\Music\\Other\\Different Track.mp3');
    expect(screen.getByText(/Different audio · 41% match/)).toBeTruthy();
    expect(screen.getByRole('alert').textContent).toContain('does not match the missing track');

    const applyBtn = screen.getByRole('button', {
      name: 'Relink selected file',
    }) as HTMLButtonElement;
    expect(applyBtn.disabled).toBe(true);

    await fireEvent.click(
      screen.getByRole('checkbox', {
        name: 'Replace this track with different audio anyway',
      }),
    );
    expect(applyBtn.disabled).toBe(false);

    await fireEvent.click(applyBtn);
    await waitFor(() =>
      expect(client.apply).toHaveBeenCalledWith(target.id, mismatchPreview.candidate.path, true),
    );
    expect(onrelinked).toHaveBeenCalledWith(updated);
  });
});
