// @vitest-environment happy-dom
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { JournalOperation } from '../ipc/bindings/JournalOperation';
import type { RenamePreview } from '../ipc/bindings/RenamePreview';
import FilenameSettings from './FilenameSettings.svelte';
import { filenameTemplate, LivePreview, setFilenameTemplate } from './rename-template.svelte';
import RenameSheet, { type RenameClient } from './RenameSheet.svelte';

HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
  this.open = true;
};
HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
  this.open = false;
};

function previewFor(template: string): RenamePreview {
  const ready = !template.includes('{bpm}');
  return {
    total: 2,
    unchanged: ready ? 0 : 1,
    ready: ready ? 1 : 0,
    collision: 0,
    missing_fields: 1,
    too_long: 0,
    file_missing: 0,
    files: [
      {
        track_id: 2,
        status: 'missing_fields',
        old_name: 'untagged.flac',
        new_name: null,
        missing: ['artist'],
      },
      {
        track_id: 1,
        status: ready ? 'ready' : 'unchanged',
        old_name: '01 first light.wav',
        new_name: ready ? 'Ada Marsh - First Light.wav' : '01 first light.wav',
        missing: [],
      },
    ],
  };
}

const appliedRename: JournalOperation = {
  id: 7,
  kind: 'rename_files',
  state: 'applied',
  summary: 'Renamed 1 file',
  can_undo: true,
  created_ms: 0,
  applied_ms: 0,
  undone_ms: null,
  recovery: null,
};

function fakeClient(): RenameClient & { operations: JournalOperation[] } {
  const client = {
    operations: [] as JournalOperation[],
    preview: vi.fn((_scope, template: string) =>
      template.includes('{genre}')
        ? Promise.reject(new Error('{genre} is not a token.'))
        : Promise.resolve(previewFor(template)),
    ),
    apply: vi.fn(() => {
      client.operations = [appliedRename];
      return Promise.resolve({ operation_id: 7, renamed: 1, failed: [], queued: false });
    }),
    recentOperations: vi.fn(() => Promise.resolve(client.operations)),
    undo: vi.fn(() => {
      client.operations = [];
      return Promise.resolve({
        operation: { ...appliedRename, state: 'undone' as const, can_undo: false },
        restored: 1,
      });
    }),
  };
  return client;
}

describe('RenameSheet', () => {
  it('shows the scope and live preview, renames the ready files and undoes them', async () => {
    setFilenameTemplate('{artist} - {title} ({mix})');
    const client = fakeClient();
    const onrenamed = vi.fn();
    render(RenameSheet, {
      open: true,
      scope: { kind: 'tracks', track_ids: [1, 2] },
      scopeLabel: 'Selected tracks',
      client,
      onrenamed,
    });

    const sheet = screen.getByRole('dialog', { name: 'Rename files' });
    await waitFor(() => expect(sheet.textContent).toMatch(/Selected tracks\s+2 files/));
    const rows = within(sheet).getByRole('list', { name: 'Preview of new file names' });
    expect(rows.textContent).toContain('Ada Marsh - First Light.wav');
    expect(rows.textContent).toContain('No artist');
    expect(sheet.textContent?.replace(/\s+/g, ' ')).toContain(
      '1 ready · 0 unchanged · 1 cannot be renamed',
    );
    expect(client.preview).toHaveBeenCalledWith(
      { kind: 'tracks', track_ids: [1, 2] },
      '{artist} - {title} ({mix})',
      50,
    );

    const field = within(sheet).getByRole('textbox', { name: 'Filename template' });
    await fireEvent.input(field, { target: { value: '{artist} - {genre}' } });
    await waitFor(() =>
      expect(within(sheet).getByRole('alert').textContent).toContain('{genre} is not a token.'),
    );
    expect(
      within(sheet).getByRole<HTMLButtonElement>('button', { name: 'Rename 0 files' }).disabled,
    ).toBe(true);

    await fireEvent.input(field, { target: { value: '{artist} - {title}' } });
    const rename = await within(sheet).findByRole('button', { name: 'Rename 1 file' });
    await fireEvent.click(rename);
    await waitFor(() =>
      expect(within(sheet).getByRole('status').textContent).toContain('Renamed 1 file.'),
    );
    expect(client.apply).toHaveBeenCalledWith(
      { kind: 'tracks', track_ids: [1, 2] },
      '{artist} - {title}',
    );
    expect(onrenamed).toHaveBeenCalledTimes(1);

    await fireEvent.click(
      await within(sheet).findByRole('button', { name: 'Undo: Renamed 1 file' }),
    );
    await waitFor(() =>
      expect(within(sheet).getByRole('status').textContent).toContain(
        'Undone. 1 file have their old names back.',
      ),
    );
    expect(client.undo).toHaveBeenCalledWith(7);
    await waitFor(() => expect(within(sheet).queryByRole('button', { name: /^Undo/ })).toBeNull());
  });
});

describe('FilenameSettings', () => {
  it('keeps a template only once it previews cleanly', async () => {
    setFilenameTemplate('{artist} - {title} ({mix})');
    const client = fakeClient();
    render(FilenameSettings, { client });

    const field = screen.getByRole('textbox', { name: 'Filename template' });
    await fireEvent.input(field, { target: { value: '{genre}' } });
    await waitFor(() => expect(screen.getByRole('alert')).toBeTruthy());
    expect(filenameTemplate.current).toBe('{artist} - {title} ({mix})');

    await fireEvent.input(field, { target: { value: '{title}' } });
    await waitFor(() => expect(filenameTemplate.current).toBe('{title}'));
    expect(client.preview).toHaveBeenLastCalledWith({ kind: 'query', query: {} }, '{title}', 3);
  });
});

describe('LivePreview', () => {
  it('sends the first template at once and only the latest one while a request is out', async () => {
    const resolvers: Array<() => void> = [];
    const preview = vi.fn(
      (_scope, template: string) =>
        new Promise<RenamePreview>((resolve) => {
          resolvers.push(() => resolve({ ...previewFor(template), total: template.length }));
        }),
    );
    const live = new LivePreview(() => ({ preview }), 10);
    const scope = { kind: 'query' as const, query: {} };

    live.request(scope, '{a}');
    live.request(scope, '{ab}');
    live.request(scope, '{abc}');
    expect(preview).toHaveBeenCalledTimes(1);

    resolvers[0]?.();
    await waitFor(() => expect(preview).toHaveBeenCalledTimes(2));
    expect(preview).toHaveBeenLastCalledWith(scope, '{abc}', 10);
    expect(live.preview).toBeNull();

    resolvers[1]?.();
    await waitFor(() => expect(live.template).toBe('{abc}'));
    expect(live.preview?.total).toBe(5);
  });
});
