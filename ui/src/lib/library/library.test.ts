// @vitest-environment happy-dom
import { fireEvent, render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import { CoreError } from '../ipc/client';
import { fakeLibrary } from './fake-library';
import Library from './Library.svelte';
import { TrackPages } from './track-pages.svelte';

describe('Library', () => {
  it('shows the tracks of a scanned library, a few rows at a time', async () => {
    const { pages } = fakeLibrary(100_000);
    await pages.load();

    render(Library, { pages });

    const grid = screen.getByRole('grid', { name: 'Tracks' });
    expect(grid.getAttribute('aria-rowcount')).toBe('100001');
    expect(screen.getByText('Track 0')).toBeTruthy();
    expect(screen.getAllByRole('row').length).toBeLessThan(60);
  });

  it('moves the selection with the arrow keys', async () => {
    const { pages } = fakeLibrary(50);
    await pages.load();
    render(Library, { pages });
    const grid = screen.getByRole('grid', { name: 'Tracks' });

    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });

    const selected = screen
      .getAllByRole('row')
      .filter((row) => row.getAttribute('aria-selected') === 'true');
    expect(selected).toHaveLength(1);
    expect(selected[0]?.textContent).toContain('Track 1');

    await fireEvent.keyDown(grid, { key: 'ArrowUp' });
    await fireEvent.keyDown(grid, { key: 'ArrowUp' });

    const top = screen
      .getAllByRole('row')
      .filter((row) => row.getAttribute('aria-selected') === 'true');
    expect(top[0]?.textContent).toContain('Track 0');
  });

  it('says how to add tracks when the library is empty', async () => {
    const { pages } = fakeLibrary(0);
    await pages.load();

    render(Library, { pages });

    expect(screen.getByRole('heading', { name: 'No tracks yet' })).toBeTruthy();
    expect(screen.getByText('resonance scan <folder>')).toBeTruthy();
  });

  it('says what went wrong and offers to try again', async () => {
    const pages = new TrackPages(() =>
      Promise.reject(new CoreError('disconnected', 'The core is not running.')),
    );
    await pages.load();

    render(Library, { pages });

    expect(screen.getByRole('alert').textContent).toContain('The core is not running.');
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
  });
});
