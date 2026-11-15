// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { CoreError } from '../ipc/client';
import { fakeLibrary } from './fake-library';
import Library from './Library.svelte';
import { TrackPages } from './track-pages.svelte';

describe('Library', () => {
  it('shows the Library header, track count and a virtualised slice of 100,000 rows', async () => {
    const { pages } = fakeLibrary(100_000);
    await pages.load();

    render(Library, { pages, onscan: vi.fn() });

    expect(screen.getByRole('heading', { name: 'Library' })).toBeTruthy();
    expect(screen.getByText('100,000 tracks')).toBeTruthy();

    const grid = screen.getByRole('grid', { name: 'Tracks' });
    expect(grid.getAttribute('aria-rowcount')).toBe('100001');
    expect(screen.getByText('Track 0')).toBeTruthy();
    expect(screen.getAllByRole('row').length).toBeLessThan(60);
  });

  it('moves the selection with the arrow keys, opens on Enter and ticks rows with X or click', async () => {
    const { pages } = fakeLibrary(50);
    await pages.load();
    const onopen = vi.fn();
    render(Library, { pages, onopen, onscan: vi.fn() });
    const grid = screen.getByRole('grid', { name: 'Tracks' });

    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });

    const selected = screen
      .getAllByRole('row')
      .filter((row) => row.getAttribute('aria-selected') === 'true');
    expect(selected).toHaveLength(1);
    expect(selected[0]?.textContent).toContain('Track 1');

    const tick = within(selected[0]!).getByRole('checkbox', { name: 'Tick Track 1' });
    expect(tick.getAttribute('aria-checked')).toBe('false');
    await fireEvent.keyDown(grid, { key: 'x' });
    expect(tick.getAttribute('aria-checked')).toBe('true');
    await fireEvent.click(tick);
    expect(tick.getAttribute('aria-checked')).toBe('false');

    await fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onopen).toHaveBeenCalledOnce();

    await fireEvent.keyDown(grid, { key: 'ArrowUp' });
    await fireEvent.keyDown(grid, { key: 'ArrowUp' });

    const top = screen
      .getAllByRole('row')
      .filter((row) => row.getAttribute('aria-selected') === 'true');
    expect(top[0]?.textContent).toContain('Track 0');
  });

  it('toggles filter pills and names the milestone for filters still to come', async () => {
    const { pages } = fakeLibrary(12);
    await pages.load();
    render(Library, { pages, onscan: vi.fn() });

    const toolbar = screen.getByRole('toolbar', { name: 'Track filters' });
    const allPill = within(toolbar).getByRole('button', { name: 'All tracks' });
    const analysedPill = within(toolbar).getByRole('button', { name: 'Analysed' });
    const favouritesPill = within(toolbar).getByRole('button', { name: 'Favourites' });

    expect(allPill.getAttribute('aria-pressed')).toBe('true');
    await fireEvent.click(analysedPill);
    expect(analysedPill.getAttribute('aria-pressed')).toBe('true');
    expect(allPill.getAttribute('aria-pressed')).toBe('false');
    expect(screen.getByRole('heading', { name: 'Analysed arrives in M5' })).toBeTruthy();

    await fireEvent.click(favouritesPill);
    expect(screen.getByRole('heading', { name: 'Favourites arrives in M2' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Show all tracks' }));
    expect(allPill.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('grid', { name: 'Tracks' })).toBeTruthy();
  });

  it('opens the folder scan when the empty library offers to add music', async () => {
    const { pages } = fakeLibrary(0);
    await pages.load();
    const onscan = vi.fn();

    render(Library, { pages, onscan });

    expect(screen.getByRole('heading', { name: 'No tracks yet' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Add a music folder' }));

    expect(onscan).toHaveBeenCalledOnce();
  });

  it('shows a busy loading state while reading the library', () => {
    const pages = new TrackPages(() => new Promise(() => {}));
    pages.load();

    render(Library, { pages, onscan: vi.fn() });

    expect(screen.getByRole('status').textContent).toContain('Reading the library');
  });

  it('says what went wrong and offers to try again', async () => {
    const pages = new TrackPages(() =>
      Promise.reject(new CoreError('disconnected', 'The core is not running.')),
    );
    await pages.load();

    render(Library, { pages, onscan: vi.fn() });

    expect(screen.getByRole('alert').textContent).toContain('The core is not running.');
    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
  });
});
