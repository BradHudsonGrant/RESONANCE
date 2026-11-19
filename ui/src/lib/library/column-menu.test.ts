// @vitest-environment happy-dom
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { DEFAULT_TABLE_LAYOUT } from '../ipc/bindings/DefaultTableLayout';
import type { LibraryTableLayout } from '../ipc/bindings/LibraryTableLayout';
import ColumnMenu from './ColumnMenu.svelte';
import { fitColumns, gridTemplate } from './columns';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

const anchor = { left: 400, right: 420, top: 10, bottom: 30 };

function withGenre(): LibraryTableLayout {
  const genre = DEFAULT_TABLE_LAYOUT.available.find((column) => column.id === 'genre')!;
  const label = DEFAULT_TABLE_LAYOUT.available.find((column) => column.id === 'label')!;
  const shownGenre = { ...genre, shown: true };
  const shownLabel = { ...label, shown: true };
  return {
    ...DEFAULT_TABLE_LAYOUT,
    columns: [...DEFAULT_TABLE_LAYOUT.columns, shownGenre, shownLabel],
    available: DEFAULT_TABLE_LAYOUT.available.map((column) =>
      column.id === 'genre' ? shownGenre : column.id === 'label' ? shownLabel : column,
    ),
    drop_order: ['label', 'genre'],
  };
}

describe('column chooser', () => {
  it('lists every column by group, focuses the first one and keeps the title', async () => {
    render(ColumnMenu, {
      props: {
        open: true,
        anchor,
        layout: DEFAULT_TABLE_LAYOUT,
        onchange: vi.fn(),
        onclose: vi.fn(),
      },
    });
    const menu = screen.getByRole('menu', { name: 'Columns' });
    expect(menu).toBeTruthy();
    for (const group of ['Track', 'Release', 'Audio', 'File']) {
      expect(screen.getByRole('group', { name: group })).toBeTruthy();
    }
    const title = screen.getByRole('menuitemcheckbox', { name: 'Title' });
    expect(title.getAttribute('aria-checked')).toBe('true');
    expect(title.getAttribute('aria-disabled')).toBe('true');
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole('menuitemcheckbox', { name: 'Artist' })),
    );
  });

  it('moves with the arrow keys, Home and End, and closes on Escape', async () => {
    const onclose = vi.fn();
    render(ColumnMenu, {
      props: {
        open: true,
        anchor,
        layout: DEFAULT_TABLE_LAYOUT,
        onchange: vi.fn(),
        onclose,
      },
    });
    const artist = screen.getByRole('menuitemcheckbox', { name: 'Artist' });
    await vi.waitFor(() => expect(document.activeElement).toBe(artist));
    await fireEvent.keyDown(artist, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(screen.getByRole('menuitemcheckbox', { name: 'BPM' }));
    await fireEvent.keyDown(document.activeElement!, { key: 'End' });
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: 'Reset to default' }));
    await fireEvent.keyDown(document.activeElement!, { key: 'Home' });
    expect(document.activeElement).toBe(artist);
    await fireEvent.keyDown(artist, { key: 'ArrowUp' });
    expect(document.activeElement).toBe(screen.getByRole('menuitem', { name: 'Reset to default' }));
    await fireEvent.keyDown(document.activeElement!, { key: 'Escape' });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it('narrows the list as the user types and sends each change', async () => {
    const onchange = vi.fn();
    render(ColumnMenu, {
      props: {
        open: true,
        anchor,
        layout: DEFAULT_TABLE_LAYOUT,
        onchange,
        onclose: vi.fn(),
      },
    });
    const search = screen.getByRole('searchbox', { name: 'Find a column' });
    await fireEvent.input(search, { target: { value: 'gen' } });
    expect(screen.queryByRole('menuitemcheckbox', { name: 'Artist' })).toBeNull();
    await fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Genre' }));
    expect(onchange).toHaveBeenLastCalledWith({ kind: 'toggle', column: 'genre' });

    await fireEvent.input(search, { target: { value: 'nothing like this' } });
    expect(screen.getByText('No column matches')).toBeTruthy();

    await fireEvent.click(screen.getByRole('menuitemcheckbox', { name: 'Wrap long text' }));
    expect(onchange).toHaveBeenLastCalledWith({ kind: 'set_wrap', wrap: true });
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Reset to default' }));
    expect(onchange).toHaveBeenLastCalledWith({ kind: 'reset' });
  });

  it('marks the columns a narrow table has dropped', () => {
    render(ColumnMenu, {
      props: {
        open: true,
        anchor,
        layout: withGenre(),
        hidden: ['label'],
        onchange: vi.fn(),
        onclose: vi.fn(),
      },
    });
    const label = screen.getByRole('menuitemcheckbox', { name: /Label/ });
    expect(label.textContent).toContain('Hidden, too narrow');
    expect(screen.getByRole('menuitemcheckbox', { name: 'Genre' }).textContent).not.toContain(
      'Hidden',
    );
  });
});

describe('column fitting', () => {
  it('drops the last picked column first and never a default one', () => {
    const layout = withGenre();
    const wide = fitColumns(layout, 5000);
    expect(wide.hidden).toEqual([]);
    const narrow = fitColumns(layout, 750);
    expect(narrow.hidden.map((column) => column.id)).toEqual(['label']);
    const tiny = fitColumns(layout, 100);
    expect(tiny.hidden.map((column) => column.id)).toEqual(['label', 'genre']);
    expect(tiny.shown.map((column) => column.id)).toEqual([
      'title',
      'artist',
      'bpm',
      'key',
      'time',
    ]);
  });

  it('gives a dragged width in pixels and lets title and artist share the rest', () => {
    const columns = DEFAULT_TABLE_LAYOUT.columns;
    expect(gridTemplate(columns, false)).toBe(
      '30px 22px 36px minmax(150px, 1.8fr) minmax(120px, 1.3fr) 58px 52px 56px 56px',
    );
    expect(gridTemplate(columns, true, { title: 300 })).toBe(
      '30px 22px 36px 300px minmax(68px, 1.3fr) 58px 52px 56px 56px',
    );
  });
});
