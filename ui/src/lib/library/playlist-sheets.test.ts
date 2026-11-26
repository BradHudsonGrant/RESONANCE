// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { LibraryCollection } from '../ipc/bindings/LibraryCollection';
import type { PlaylistMirror } from '../ipc/bindings/PlaylistMirror';
import PlaylistExportSheet from './PlaylistExportSheet.svelte';
import PlaylistImportSheet from './PlaylistImportSheet.svelte';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.open = false;
};

const warmUp: LibraryCollection = {
  id: 7,
  name: 'Warm Up',
  position: 0,
  track_count: 3,
  mirror: false,
};

const mirror: PlaylistMirror = {
  collection: { ...warmUp, id: 8, mirror: true },
  matched: 3,
  not_in_library: 0,
  skipped: 0,
};

describe('PlaylistImportSheet', () => {
  it('keeps the typed path without a picker', async () => {
    const importPlaylist = vi.fn(() => Promise.resolve(mirror));
    render(PlaylistImportSheet, {
      open: true,
      client: { find: () => Promise.resolve([]), importPlaylist },
    });

    const sheet = screen.getByRole('dialog', { name: 'Import playlist' });
    expect(within(sheet).queryByRole('button', { name: 'Browse…' })).toBeNull();
    await fireEvent.input(within(sheet).getByLabelText('Playlist file'), {
      target: { value: '/music/sets/warm-up.m3u8' },
    });
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Import' }));
    await waitFor(() => expect(importPlaylist).toHaveBeenCalledWith('/music/sets/warm-up.m3u8'));
  });

  it('fills the field from the picker and imports the picked file', async () => {
    const importPlaylist = vi.fn(() => Promise.resolve(mirror));
    const pickPlaylistFile = vi.fn(() => Promise.resolve('/music/sets/peak.m3u'));
    const onimported = vi.fn();
    render(PlaylistImportSheet, {
      open: true,
      client: { find: () => Promise.resolve([]), importPlaylist, pickPlaylistFile },
      onimported,
    });

    const sheet = screen.getByRole('dialog', { name: 'Import playlist' });
    const field = within(sheet).getByLabelText<HTMLInputElement>('Playlist file');
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Browse…' }));
    await waitFor(() => expect(field.value).toBe('/music/sets/peak.m3u'));
    expect(importPlaylist).not.toHaveBeenCalled();

    await fireEvent.click(within(sheet).getByRole('button', { name: 'Import' }));
    await waitFor(() => expect(onimported).toHaveBeenCalledWith(mirror));
    expect(importPlaylist).toHaveBeenCalledWith('/music/sets/peak.m3u');
  });

  it('leaves the field alone when the dialog is cancelled', async () => {
    const pickPlaylistFile = vi.fn(() => Promise.resolve(null));
    render(PlaylistImportSheet, {
      open: true,
      client: {
        find: () => Promise.resolve([]),
        importPlaylist: vi.fn(() => Promise.resolve(mirror)),
        pickPlaylistFile,
      },
    });

    const sheet = screen.getByRole('dialog', { name: 'Import playlist' });
    const field = within(sheet).getByLabelText<HTMLInputElement>('Playlist file');
    await fireEvent.input(field, { target: { value: '/music/typed.m3u8' } });
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Browse…' }));
    await waitFor(() => expect(pickPlaylistFile).toHaveBeenCalledTimes(1));
    expect(field.value).toBe('/music/typed.m3u8');
  });
});

describe('PlaylistExportSheet', () => {
  it('suggests a path in the folder without a picker', async () => {
    const exportCollection = vi.fn(() =>
      Promise.resolve({ path: '/music/Warm Up.m3u8', tracks: 3 }),
    );
    render(PlaylistExportSheet, {
      open: true,
      collection: warmUp,
      folder: '/music/',
      client: { exportCollection },
    });

    const sheet = screen.getByRole('dialog', { name: 'Export to M3U8' });
    expect(within(sheet).queryByRole('button', { name: 'Browse…' })).toBeNull();
    expect(within(sheet).getByLabelText<HTMLInputElement>('Save as').value).toBe(
      '/music/Warm Up.m3u8',
    );
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(within(sheet).getByRole('status').textContent).toContain('3 tracks'),
    );
    expect(exportCollection).toHaveBeenCalledWith(7, '/music/Warm Up.m3u8');
  });

  it('starts the save dialog in the folder with the suggested name and saves there', async () => {
    const exportCollection = vi.fn(() =>
      Promise.resolve({ path: 'D:\\Sets\\Warm Up.m3u8', tracks: 3 }),
    );
    const pickPlaylistSavePath = vi.fn(() => Promise.resolve('D:\\Sets\\Warm Up.m3u8'));
    render(PlaylistExportSheet, {
      open: true,
      collection: warmUp,
      folder: 'D:\\Music',
      client: { exportCollection, pickPlaylistSavePath },
    });

    const sheet = screen.getByRole('dialog', { name: 'Export to M3U8' });
    const field = within(sheet).getByLabelText<HTMLInputElement>('Save as');
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Browse…' }));
    expect(pickPlaylistSavePath).toHaveBeenCalledWith('D:\\Music', 'Warm Up.m3u8');
    await waitFor(() => expect(field.value).toBe('D:\\Sets\\Warm Up.m3u8'));
    expect(exportCollection).not.toHaveBeenCalled();

    await fireEvent.click(within(sheet).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(exportCollection).toHaveBeenCalledWith(7, 'D:\\Sets\\Warm Up.m3u8'));
  });

  it('keeps the suggested path when the save dialog is cancelled', async () => {
    const pickPlaylistSavePath = vi.fn(() => Promise.resolve(null));
    render(PlaylistExportSheet, {
      open: true,
      collection: warmUp,
      folder: null,
      client: {
        exportCollection: vi.fn(() => Promise.resolve({ path: 'Warm Up.m3u8', tracks: 3 })),
        pickPlaylistSavePath,
      },
    });

    const sheet = screen.getByRole('dialog', { name: 'Export to M3U8' });
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Browse…' }));
    await waitFor(() => expect(pickPlaylistSavePath).toHaveBeenCalledWith(null, 'Warm Up.m3u8'));
    expect(within(sheet).getByLabelText<HTMLInputElement>('Save as').value).toBe('Warm Up.m3u8');
  });
});
