// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { JournalOperation } from '../ipc/bindings/JournalOperation';
import DeleteSheet from './DeleteSheet.svelte';
import HistorySheet from './HistorySheet.svelte';

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

describe('HistorySheet', () => {
  it('lists changes newest first and undoes the one picked', async () => {
    const rolledBack: JournalOperation = {
      ...moved,
      id: 3,
      kind: 'rename_files',
      state: 'rolled_back',
      summary: 'Renaming 1 file',
      can_undo: false,
      recovery: 'The app stopped part way, so the steps that had happened were put back.',
    };
    const undone: JournalOperation = { ...moved, state: 'undone', can_undo: false };
    let operations = [moved, rolledBack];
    const client = {
      recentOperations: vi.fn(() => Promise.resolve(operations)),
      undo: vi.fn(() => {
        operations = [undone, rolledBack];
        return Promise.resolve({ operation: undone, restored: 2 });
      }),
    };
    const onundone = vi.fn();
    render(HistorySheet, { open: true, client, onundone });

    const list = await screen.findByRole('list', { name: 'Changes to files' });
    const [first, second, ...rest] = within(list).getAllByRole('listitem');
    if (!first || !second) throw new Error('expected 2 changes');
    expect(rest).toHaveLength(0);
    expect(first.textContent).toContain('Moved 2 files to the bin');
    expect(second.textContent).toContain('Put back');
    expect(second.textContent).toContain('The app stopped part way');
    expect(within(second).queryByRole('button')).toBeNull();

    await fireEvent.click(
      within(first).getByRole('button', { name: 'Undo Moved 2 files to the bin' }),
    );
    await waitFor(() => expect(onundone).toHaveBeenCalledTimes(1));
    expect(client.undo).toHaveBeenCalledWith(4);
    await waitFor(() => expect(within(list).queryByRole('button', { name: /^Undo/ })).toBeNull());
    expect(within(list).getAllByRole('listitem')[0]?.textContent).toContain('Undone');
  });
});
