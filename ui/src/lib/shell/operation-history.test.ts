// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { JournalOperation } from '../ipc/bindings/JournalOperation';
import ActivitySheet from './ActivitySheet.svelte';
import OperationHistory from './OperationHistory.svelte';

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

function fakeClient() {
  let operations = [moved, rolledBack];
  return {
    recentOperations: vi.fn(() => Promise.resolve(operations)),
    undo: vi.fn(() => {
      operations = [undone, rolledBack];
      return Promise.resolve({ operation: undone, restored: 2 });
    }),
  };
}

describe('OperationHistory', () => {
  it('lists changes newest first and undoes the one picked', async () => {
    const client = fakeClient();
    const onundone = vi.fn();
    render(OperationHistory, { open: true, client, onundone });

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

  it('undoes the newest change that can be undone when Ctrl+Z asks', async () => {
    const client = fakeClient();
    const onundone = vi.fn();
    const view = render(OperationHistory, { open: true, client, onundone, undoRequest: 0 });
    await screen.findByRole('list', { name: 'Changes to files' });
    expect(client.undo).not.toHaveBeenCalled();

    await view.rerender({ open: true, client, onundone, undoRequest: 1 });
    await waitFor(() => expect(client.undo).toHaveBeenCalledWith(4));
    expect(onundone).toHaveBeenCalledTimes(1);

    await view.rerender({ open: true, client, onundone, undoRequest: 2 });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Nothing to undo'));
    expect(client.undo).toHaveBeenCalledTimes(1);
  });
});

describe('OperationHistory first Ctrl+Z', () => {
  it('undoes when Activity first opens because of Ctrl+Z', async () => {
    const client = fakeClient();
    render(OperationHistory, { open: true, client, undoRequest: 1 });
    await waitFor(() => expect(client.undo).toHaveBeenCalledWith(4));
  });
});

describe('OperationHistory queue', () => {
  it('keeps a second Ctrl+Z pressed while the first undo still runs', async () => {
    const older: JournalOperation = { ...moved, id: 2, summary: 'Moved 1 file to the bin' };
    let operations = [moved, older];
    let release = () => {};
    const client = {
      recentOperations: vi.fn(() => Promise.resolve(operations)),
      undo: vi.fn((id: number) => {
        const done = {
          ...(id === moved.id ? moved : older),
          state: 'undone' as const,
          can_undo: false,
        };
        operations = operations.map((operation) => (operation.id === id ? done : operation));
        return new Promise<{ operation: JournalOperation; restored: number }>((resolve) => {
          release = () => resolve({ operation: done, restored: 1 });
        });
      }),
    };
    const view = render(OperationHistory, { open: true, client, undoRequest: 0 });
    await screen.findByRole('list', { name: 'Changes to files' });

    await view.rerender({ open: true, client, undoRequest: 1 });
    await waitFor(() => expect(client.undo).toHaveBeenCalledWith(4));
    await view.rerender({ open: true, client, undoRequest: 2 });
    release();
    await waitFor(() => expect(client.undo).toHaveBeenCalledWith(2));
    release();
    expect(client.undo).toHaveBeenCalledTimes(2);
  });
});

describe('ActivitySheet history', () => {
  it('shows changes to files below the jobs', async () => {
    render(ActivitySheet, { open: true, historyClient: fakeClient() });
    const sheet = screen.getByRole('dialog', { name: 'Activity' });
    const list = await within(sheet).findByRole('list', { name: 'Changes to files' });
    expect(list.textContent).toContain('Moved 2 files to the bin');
  });
});
