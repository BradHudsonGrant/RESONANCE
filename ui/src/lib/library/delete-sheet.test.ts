// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { JournalOperation } from '../ipc/bindings/JournalOperation';
import DeleteSheet from './DeleteSheet.svelte';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.open = false;
};

const moved: JournalOperation = {
  id: 4,
  kind: 'delete_files',
  state: 'applied',
  summary: 'Moved 2 files to the bin',
  can_undo: true,
  created_ms: Date.UTC(2026, 10, 25, 9, 30),
  applied_ms: Date.UTC(2026, 10, 25, 9, 30),
  undone_ms: null,
  recovery: null,
};

describe('DeleteSheet', () => {
  it('moves the selected tracks to the bin once confirmed', async () => {
    const deleteTracks = vi.fn(() => Promise.resolve({ operation: moved, no_bin: null }));
    const ondeleted = vi.fn();
    render(DeleteSheet, { open: true, trackIds: [1, 2], client: { deleteTracks }, ondeleted });

    const sheet = screen.getByRole('dialog', { name: 'Move 2 tracks to the bin' });
    expect(deleteTracks).not.toHaveBeenCalled();
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Move to bin' }));
    await waitFor(() => expect(ondeleted).toHaveBeenCalledTimes(1));
    expect(deleteTracks).toHaveBeenCalledWith([1, 2], false);
  });

  it('asks again before deleting for good when the bin refuses the files', async () => {
    const deleteTracks = vi.fn((_ids: number[], forGood: boolean) =>
      Promise.resolve(
        forGood
          ? {
              operation: { ...moved, kind: 'delete_files_for_good' as const, can_undo: false },
              no_bin: null,
            }
          : { operation: null, no_bin: 'song.mp3 could not be moved to the bin: no Recycle Bin' },
      ),
    );
    const ondeleted = vi.fn();
    render(DeleteSheet, { open: true, trackIds: [3], client: { deleteTracks }, ondeleted });

    await fireEvent.click(screen.getByRole('button', { name: 'Move to bin' }));
    const sheet = await screen.findByRole('dialog', { name: 'Delete for good' });
    expect(within(sheet).getByRole('alert').textContent).toContain('no Recycle Bin');
    expect(ondeleted).not.toHaveBeenCalled();

    await fireEvent.click(within(sheet).getByRole('button', { name: 'Delete for good' }));
    await waitFor(() => expect(ondeleted).toHaveBeenCalledTimes(1));
    expect(deleteTracks).toHaveBeenLastCalledWith([3], true);
  });
});
