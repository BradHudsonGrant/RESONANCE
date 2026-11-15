// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { LibrarySource } from '../ipc/bindings/LibrarySource';
import type { LibrarySources } from '../ipc/bindings/LibrarySources';
import type { SourceSubfolder } from '../ipc/bindings/SourceSubfolder';
import SourcesSettings, { type SourcesClient } from './SourcesSettings.svelte';

const DEFAULT_IGNORED = ['_replaced', '_wrong_match', '_trash', '_converted_originals'];

function makeSource(overrides: Partial<LibrarySource> = {}): LibrarySource {
  return {
    id: 1,
    folder: 'D:\\Sets\\Club',
    include_subfolders: true,
    watch: true,
    watching_active: true,
    excluded_subfolders: [],
    track_count: 5,
    ...overrides,
  };
}

function fakeClient(initial?: Partial<LibrarySources>) {
  let state: LibrarySources = {
    sources: initial?.sources ?? [makeSource()],
    ignored_folders: initial?.ignored_folders ?? [...DEFAULT_IGNORED],
  };

  const subfoldersByParent = new Map<string, SourceSubfolder[]>([
    [
      '1:',
      [
        { name: 'Garage', path: 'Garage', enabled: true, has_subfolders: false },
        { name: 'House', path: 'House', enabled: true, has_subfolders: true },
      ],
    ],
    ['1:House', [{ name: 'Deep', path: 'House/Deep', enabled: true, has_subfolders: false }]],
  ]);

  const recomputeSubfolderEnabled = (source: LibrarySource) => {
    for (const list of subfoldersByParent.values()) {
      for (const item of list) {
        item.enabled = !source.excluded_subfolders.some(
          (ex) => item.path === ex || item.path.startsWith(`${ex}/`),
        );
      }
    }
  };

  const client: SourcesClient = {
    sources: vi.fn(async () => structuredClone(state)),
    subfolders: vi.fn(async (sourceId: number, parent: string | null) =>
      structuredClone(subfoldersByParent.get(`${sourceId}:${parent ?? ''}`) ?? []),
    ),
    scan: vi.fn(async (folder: string) => {
      if (!state.sources.some((item) => item.folder === folder)) {
        state = {
          ...state,
          sources: [
            ...state.sources,
            makeSource({ id: state.sources.length + 1, folder, track_count: 3 }),
          ],
        };
      }
      return {
        source_folder: folder,
        found: 3,
        added: 3,
        changed: 0,
        unchanged: 0,
        unreadable_count: 0,
        unreadable: [],
      };
    }),
    updateSource: vi.fn(async (params) => {
      state = {
        ...state,
        sources: state.sources.map((source) => {
          if (source.id !== params.source_id) return source;
          const next: LibrarySource = {
            ...source,
            include_subfolders: params.include_subfolders,
            watch: params.watch,
            watching_active: params.watch,
            excluded_subfolders: [...params.excluded_subfolders],
            track_count: !params.include_subfolders ? 1 : 5 - params.excluded_subfolders.length * 2,
          };
          recomputeSubfolderEnabled(next);
          return next;
        }),
      };
      return structuredClone(state);
    }),
    removeSource: vi.fn(async (sourceId: number) => {
      state = {
        ...state,
        sources: state.sources.filter((source) => source.id !== sourceId),
      };
      return structuredClone(state);
    }),
    setIgnoredFolders: vi.fn(async (folders: string[]) => {
      state = {
        ...state,
        ignored_folders: [...folders],
      };
      return structuredClone(state);
    }),
  };

  return { client, getState: () => state };
}

describe('SourcesSettings', () => {
  it('loads sources, adds a folder, rescans and removes a source', async () => {
    const { client } = fakeClient({ sources: [] });
    const onchanged = vi.fn();
    render(SourcesSettings, { client, onchanged });

    await vi.waitFor(() =>
      expect(
        screen.getByText('No music folders added yet. Files on disk are never changed by a scan.'),
      ).toBeTruthy(),
    );

    const input = screen.getByLabelText('Add music folder') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'D:\\Sets\\Warmup' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add folder' }));

    await vi.waitFor(() =>
      expect(screen.getByRole('region', { name: 'D:\\Sets\\Warmup' })).toBeTruthy(),
    );
    expect(input.value).toBe('');
    expect(screen.getByRole('status').textContent).toBe(
      '3 files: 3 added, 0 changed, 0 unchanged.',
    );
    expect(onchanged).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Rescan D:\\Sets\\Warmup' }));
    await vi.waitFor(() => expect(onchanged).toHaveBeenCalledTimes(2));

    await fireEvent.click(screen.getByRole('button', { name: 'Remove D:\\Sets\\Warmup' }));
    await vi.waitFor(() =>
      expect(screen.queryByRole('region', { name: 'D:\\Sets\\Warmup' })).toBeNull(),
    );
    expect(onchanged).toHaveBeenCalledTimes(3);
  });

  it('toggles subfolder inclusion, nested subfolders and watching', async () => {
    const { client } = fakeClient();
    const onchanged = vi.fn();
    render(SourcesSettings, { client, onchanged });

    const card = await vi.waitFor(() => screen.getByRole('region', { name: 'D:\\Sets\\Club' }));
    expect(card.textContent).toContain('5 tracks');
    expect(card.textContent).toContain('Watching');

    await fireEvent.click(within(card).getByRole('button', { name: 'Subfolders' }));
    const houseCheck = await vi.waitFor(() =>
      within(card).getByRole('checkbox', { name: 'Include House' }),
    );
    await fireEvent.click(within(card).getByRole('button', { name: 'Expand House' }));
    const deepCheck = (await vi.waitFor(() =>
      within(card).getByRole('checkbox', { name: 'Include House/Deep' }),
    )) as HTMLInputElement;
    expect(deepCheck.disabled).toBe(false);

    const watchToggle = within(card).getByRole('checkbox', {
      name: 'Watch for changes',
    }) as HTMLInputElement;
    await fireEvent.click(houseCheck);
    await vi.waitFor(() => {
      expect(watchToggle.disabled).toBe(false);
      expect(card.textContent).toContain('3 tracks');
    });
    expect(
      (within(card).getByRole('checkbox', { name: 'Include House/Deep' }) as HTMLInputElement)
        .disabled,
    ).toBe(true);
    expect(card.textContent).toContain('1 off');
    expect(onchanged).toHaveBeenCalledTimes(1);

    await fireEvent.click(watchToggle);
    await vi.waitFor(() => {
      expect(watchToggle.disabled).toBe(false);
      expect(card.textContent).not.toContain('Watching');
    });

    const includeAll = within(card).getByRole('checkbox', {
      name: 'Include subfolders',
    }) as HTMLInputElement;
    await fireEvent.click(includeAll);
    await vi.waitFor(() => {
      expect(includeAll.disabled).toBe(false);
      expect(card.textContent).toContain('1 track');
    });
    expect(within(card).queryByRole('button', { name: 'Subfolders' })).toBeNull();
  });

  it('edits and resets the ignored folder names', async () => {
    const { client } = fakeClient();
    const onchanged = vi.fn();
    render(SourcesSettings, { client, onchanged });

    await vi.waitFor(() => expect(screen.getByText('_replaced')).toBeTruthy());
    expect(screen.queryByRole('button', { name: 'Reset defaults' })).toBeNull();

    const input = screen.getByLabelText('Folder name to ignore') as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'samples' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Ignore' }));

    await vi.waitFor(() => expect(screen.getByText('samples')).toBeTruthy());
    expect(screen.getByRole('button', { name: 'Reset defaults' })).toBeTruthy();
    expect(onchanged).toHaveBeenCalledTimes(1);

    await fireEvent.click(screen.getByRole('button', { name: 'Remove _trash' }));
    await vi.waitFor(() => expect(screen.queryByText('_trash')).toBeNull());
    expect(onchanged).toHaveBeenCalledTimes(2);

    await fireEvent.click(screen.getByRole('button', { name: 'Reset defaults' }));
    await vi.waitFor(() => expect(screen.getByText('_trash')).toBeTruthy());
    expect(screen.queryByText('samples')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Reset defaults' })).toBeNull();
    expect(onchanged).toHaveBeenCalledTimes(3);
  });
});
