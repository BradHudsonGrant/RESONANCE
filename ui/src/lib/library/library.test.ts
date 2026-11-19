// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import { CoreError } from '../ipc/client';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';
import { fakeLibrary, fakeTableLayout, fakeTrack } from './fake-library';
import { audioSummary, trackSeed } from './format';
import Inspector from './Inspector.svelte';
import Library from './Library.svelte';
import { TrackPages } from './track-pages.svelte';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

describe('Library', () => {
  it('shows the Library header, track count and a virtualised slice of 100,000 rows', async () => {
    const { pages } = fakeLibrary(100_000);
    await pages.load();

    render(Library, { pages, onscan: vi.fn() });

    expect(screen.getByRole('heading', { name: 'Library' })).toBeTruthy();
    expect(screen.getByText('100,000 tracks')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Filters' })).toBeTruthy();

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
    expect(
      screen.getAllByRole('row').filter((row) => row.getAttribute('aria-selected') === 'true'),
    ).toHaveLength(0);

    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onopen).toHaveBeenCalledOnce();

    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'ArrowUp' });

    const top = screen
      .getAllByRole('row')
      .filter((row) => row.getAttribute('aria-selected') === 'true');
    expect(top[0]?.textContent).toContain('Track 0');
  });

  it('toggles filter pills and returns to all tracks when an active pill is clicked again', async () => {
    const queries: TrackQuery[] = [];
    const pages = new TrackPages((offset, limit, query) => {
      if (query) queries.push(query);
      const count = query?.pill === 'favourites' ? 2 : 12;
      return Promise.resolve({
        total: count,
        library_total: 12,
        offset,
        tracks: Array.from({ length: Math.min(limit, count) }, (_, i) => fakeTrack(offset + i)),
      });
    });
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
    expect(favouritesPill.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('grid', { name: 'Tracks' })).toBeTruthy();
    expect(queries.at(-1)?.pill).toBe('favourites');

    await fireEvent.click(favouritesPill);
    expect(allPill.getAttribute('aria-pressed')).toBe('true');
    expect(queries.at(-1)?.pill).toBe('all');
  });

  it('cycles column sort headers, toggles row favourites and opens the filter drawer', async () => {
    const queries: TrackQuery[] = [];
    const pages = new TrackPages((offset, limit, query) => {
      if (query) queries.push(query);
      return Promise.resolve({
        total: 4,
        library_total: 4,
        offset,
        tracks: Array.from({ length: Math.min(limit, 4) }, (_, i) => fakeTrack(offset + i)),
      });
    });
    await pages.load();
    const ontogglefavourite = vi.fn();
    const onclearsearch = vi.fn();
    const loadFacets = vi.fn().mockResolvedValue({
      bpm_min: 120,
      bpm_max: 132,
      year_min: 2020,
      year_max: 2024,
      labels: ['Northline'],
      missing_count: 0,
    });

    render(Library, {
      pages,
      onscan: vi.fn(),
      ontogglefavourite,
      onclearsearch,
      loadFacets,
      tableLayout: fakeTableLayout(),
    });

    const bpmBtn = screen.getByRole('button', { name: 'BPM' });
    const header = () => bpmBtn.closest('[role="columnheader"]')?.getAttribute('aria-sort');
    await fireEvent.click(bpmBtn);
    await vi.waitFor(() => expect(queries.at(-1)).toMatchObject({ sort: 'bpm', direction: 'asc' }));
    expect(header()).toBe('ascending');

    await fireEvent.click(bpmBtn);
    await vi.waitFor(() =>
      expect(queries.at(-1)).toMatchObject({ sort: 'bpm', direction: 'desc' }),
    );
    expect(header()).toBe('descending');

    await fireEvent.click(bpmBtn);
    await vi.waitFor(() =>
      expect(queries.at(-1)).toMatchObject({ sort: 'index', direction: 'asc' }),
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Favourite Track 0' }));
    expect(ontogglefavourite).toHaveBeenCalledWith(expect.objectContaining({ id: 1 }));

    await fireEvent.click(screen.getByRole('button', { name: 'Filters' }));
    expect(loadFacets).toHaveBeenCalledOnce();
    const drawer = screen.getByRole('dialog', { name: 'Filters' });
    await fireEvent.click(within(drawer).getByRole('button', { name: '8A (Am)' }));
    expect(queries.at(-1)?.keys).toEqual(['8A']);

    await fireEvent.click(within(drawer).getByRole('button', { name: 'Clear' }));
    expect(queries.at(-1)?.keys).toBeNull();
  });

  it('shows a distinct no-matches empty state when filtered results are empty and clears all filters', async () => {
    const pages = new TrackPages((_offset, _limit, query) =>
      Promise.resolve({
        total: query?.text ? 0 : 3,
        library_total: 3,
        offset: 0,
        tracks: query?.text ? [] : [fakeTrack(0), fakeTrack(1), fakeTrack(2)],
      }),
    );
    await pages.load();
    await pages.setQuery({ text: 'missing', sort: 'title', direction: 'asc' });
    const onclearsearch = vi.fn();

    render(Library, {
      pages,
      onscan: vi.fn(),
      onclearsearch,
      tableLayout: fakeTableLayout([{ column: 'title', field: 'title', direction: 'asc' }]),
    });

    expect(screen.getByRole('heading', { name: 'No tracks match' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(onclearsearch).toHaveBeenCalledOnce();
    expect(pages.query.text).toBeUndefined();
    expect(pages.query.sort).toBe('title');
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

  it('shows active collection breadcrumb, empty collection state and keeps collection when clearing filters', async () => {
    const pages = new TrackPages((_offset, _limit, query) =>
      Promise.resolve({
        total: query?.text ? 0 : 0,
        library_total: 4,
        offset: 0,
        tracks: [],
      }),
    );
    await pages.load();
    await pages.setQuery({ collection_id: 7 });
    const onclearcollection = vi.fn();
    const activeCollection = { id: 7, name: 'Warm Up', position: 0, track_count: 0 };

    render(Library, { pages, activeCollection, onscan: vi.fn(), onclearcollection });

    expect(screen.getByRole('heading', { name: 'Warm Up' })).toBeTruthy();
    expect(screen.getByRole('heading', { name: 'This collection is empty' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Show all tracks' }));
    expect(onclearcollection).toHaveBeenCalledOnce();

    await pages.setQuery({ collection_id: 7, text: 'zzz' });
    await fireEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(pages.query.collection_id).toBe(7);
    expect(pages.query.text).toBeUndefined();
  });

  it('supports multi-select checkboxes, Shift+Arrow range ticks, shortcuts and row actions', async () => {
    const pages = new TrackPages(async () => ({
      total: 3,
      library_total: 3,
      offset: 0,
      tracks: [
        { ...fakeTrack(0), collections: [{ id: 1, name: 'Warm Up' }] },
        fakeTrack(1),
        fakeTrack(2),
      ],
    }));
    await pages.load();
    const collections = [
      { id: 1, name: 'Warm Up', position: 0, track_count: 1 },
      { id: 2, name: 'Peak Hour', position: 1, track_count: 0 },
    ];
    const onaddtocollection = vi.fn(async () => {});
    const onremovefromcollection = vi.fn(async () => {});
    const oncreatecollectionandadd = vi.fn(async () => {});

    render(Library, {
      pages,
      collections,
      activeCollection: collections[0],
      onscan: vi.fn(),
      onaddtocollection,
      onremovefromcollection,
      oncreatecollectionandadd,
    });

    await fireEvent.click(screen.getByRole('button', { name: 'Actions for Track 0' }));
    await fireEvent.click(screen.getByRole('menuitemcheckbox', { name: /Peak Hour/ }));
    expect(onaddtocollection).toHaveBeenCalledWith(2, [1]);

    const grid = screen.getByRole('grid', { name: 'Tracks' });
    grid.focus();
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });

    const firstRow = screen
      .getAllByRole('row')
      .find((row) => row.getAttribute('aria-selected') === 'true')!;
    expect(firstRow.querySelector('.status-cell .status-icon')).toBeTruthy();
    expect(firstRow.querySelector('.status-cell button')).toBeNull();
    expect(firstRow.querySelector('.row-actions .row-star')).toBeTruthy();

    await fireEvent.keyDown(grid, { key: 'c' });
    const removeActiveItem = screen.getByRole('menuitem', { name: 'Remove from Warm Up' });
    const warmItem = screen.getByRole('menuitemcheckbox', { name: /Warm Up/ });
    const peakItem = screen.getByRole('menuitemcheckbox', { name: /Peak Hour/ });
    const newItem = screen.getByRole('menuitem', { name: 'New collection…' });
    expect(document.activeElement).toBe(removeActiveItem);
    await fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(warmItem);
    await fireEvent.keyDown(window, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(peakItem);
    await fireEvent.keyDown(window, { key: 'End' });
    expect(document.activeElement).toBe(newItem);
    await fireEvent.keyDown(window, { key: 'Home' });
    expect(document.activeElement).toBe(removeActiveItem);
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(grid);

    await fireEvent.keyDown(grid, { key: 'ArrowDown', shiftKey: true });
    expect(screen.getByText('2 selected')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Add to collection' }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'New collection…' }));
    const nameInput = screen.getByRole('textbox', { name: 'New collection name' });
    await fireEvent.input(nameInput, { target: { value: 'Closing' } });
    await fireEvent.submit(nameInput.closest('form')!);
    expect(oncreatecollectionandadd).toHaveBeenCalledWith('Closing', [1, 2]);

    await fireEvent.keyDown(grid, { key: 'Delete' });
    expect(onremovefromcollection).toHaveBeenCalledWith(1, [1, 2]);
  });

  it('triggers preview from the row status button and double-click while keeping Enter for open', async () => {
    const { pages } = fakeLibrary(3);
    await pages.load();
    const onopen = vi.fn();
    const onpreview = vi.fn();

    render(Library, {
      pages,
      previewTrackId: 1,
      previewPlaying: true,
      onopen,
      onpreview,
      onscan: vi.fn(),
    });

    const pauseRowZero = screen.getByRole('button', { name: 'Pause Track 0' });
    expect(pauseRowZero.getAttribute('aria-pressed')).toBe('true');
    await fireEvent.click(pauseRowZero);
    expect(onpreview).toHaveBeenCalledWith(expect.objectContaining({ id: 1, title: 'Track 0' }));

    const rows = screen.getAllByRole('row');
    await fireEvent.dblClick(rows[2]!);
    expect(onpreview).toHaveBeenCalledWith(expect.objectContaining({ id: 2, title: 'Track 1' }));
    expect(onopen).not.toHaveBeenCalled();

    const grid = screen.getByRole('grid', { name: 'Tracks' });
    await fireEvent.keyDown(grid, { key: 'Enter' });
    expect(onopen).toHaveBeenCalledOnce();
  });

  it('deselects on repeat row click, tick box click, Escape or empty space, and supports Select all and Clear', async () => {
    const { pages } = fakeLibrary(4);
    await pages.load();
    render(Library, { pages, onscan: vi.fn() });

    const rows = () => screen.getAllByRole('row');
    const selectedRows = () => rows().filter((row) => row.getAttribute('aria-selected') === 'true');

    // Click a row to select, click the same row again to deselect.
    await fireEvent.click(rows()[1]!);
    expect(selectedRows()).toHaveLength(1);
    await fireEvent.click(rows()[1]!);
    expect(selectedRows()).toHaveLength(0);

    // Click a row to select, then click its filled tick box to deselect.
    await fireEvent.click(rows()[2]!);
    expect(selectedRows()).toHaveLength(1);
    await fireEvent.click(within(rows()[2]!).getByRole('checkbox', { name: 'Tick Track 1' }));
    expect(selectedRows()).toHaveLength(0);

    // Select a row, then Escape deselects even when nothing is multi-ticked.
    await fireEvent.click(rows()[1]!);
    expect(selectedRows()).toHaveLength(1);
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(selectedRows()).toHaveLength(0);

    // Select a row, then click another row's tick box -> both rows enter multi-selection,
    // Select all ticks all 4, and Clear clears both checked rows and active row selection.
    await fireEvent.click(rows()[1]!);
    await fireEvent.click(within(rows()[3]!).getByRole('checkbox', { name: 'Tick Track 2' }));
    expect(screen.getByText('2 selected')).toBeTruthy();

    const selectAll = screen.getByRole('button', { name: 'Select all' });
    expect(selectAll.getAttribute('aria-disabled')).toBe('false');
    await fireEvent.click(selectAll);
    expect(screen.getByText('4 selected')).toBeTruthy();
    // It stays in place, disabled, so the buttons beside it do not shift.
    expect(screen.getByRole('button', { name: 'Select all' })).toBe(selectAll);
    expect(selectAll.getAttribute('aria-disabled')).toBe('true');
    await fireEvent.click(selectAll);
    expect(screen.getByText('4 selected')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Clear' }));
    expect(screen.queryByRole('region', { name: 'Selected tracks' })).toBeNull();
    expect(selectedRows()).toHaveLength(0);

    // Ctrl+A selects all loaded tracks; header checkbox deselects all; empty viewport click deselects.
    await fireEvent.keyDown(window, { key: 'a', ctrlKey: true });
    expect(screen.getByText('4 selected')).toBeTruthy();
    await fireEvent.click(screen.getByRole('checkbox', { name: 'Deselect all tracks' }));
    expect(screen.queryByRole('region', { name: 'Selected tracks' })).toBeNull();
    expect(selectedRows()).toHaveLength(0);

    // Shift+Click selects a range and clicking empty viewport space clears everything.
    await fireEvent.click(rows()[1]!);
    await fireEvent.click(rows()[3]!, { shiftKey: true });
    expect(screen.getByText('3 selected')).toBeTruthy();
    const viewport = document.querySelector('.viewport')!;
    await fireEvent.click(viewport);
    expect(screen.queryByRole('region', { name: 'Selected tracks' })).toBeNull();
    expect(selectedRows()).toHaveLength(0);
  });

  it('swaps the filter pills for the selection actions and clears them with Escape', async () => {
    const { pages } = fakeLibrary(4);
    await pages.load();
    render(Library, { pages, onscan: vi.fn() });
    const grid = screen.getByRole('grid', { name: 'Tracks' });
    const pills = screen.getByRole('toolbar', { name: 'Track filters' });

    expect(screen.queryByRole('region', { name: 'Selected tracks' })).toBeNull();
    expect(pills.hasAttribute('inert')).toBe(false);

    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'x' });
    const region = screen.getByRole('region', { name: 'Selected tracks' });
    expect(within(region).getByText('1 selected').getAttribute('aria-live')).toBe('polite');
    expect(pills.hasAttribute('inert')).toBe(true);

    // Escape from a selection action clears it and hands focus back to the table.
    const clear = within(region).getByRole('button', { name: 'Clear' });
    clear.focus();
    await fireEvent.keyDown(clear, { key: 'Escape' });
    expect(screen.queryByRole('region', { name: 'Selected tracks' })).toBeNull();
    expect(pills.hasAttribute('inert')).toBe(false);
    expect(document.activeElement).toBe(grid);

    // Escape on the table clears it too.
    await fireEvent.keyDown(grid, { key: 'ArrowDown' });
    await fireEvent.keyDown(grid, { key: 'x' });
    expect(screen.getByText('1 selected')).toBeTruthy();
    await fireEvent.keyDown(window, { key: 'Escape' });
    expect(screen.queryByRole('region', { name: 'Selected tracks' })).toBeNull();
  });
});

describe('Inspector and track formatting', () => {
  it('shows seeded artwork, audio format, two detail rows, favourite button and file facts for a track', async () => {
    const onclose = vi.fn();
    const ontogglefavourite = vi.fn();
    const onpreview = vi.fn();
    const track = {
      ...fakeTrack(3),
      album: 'Night Bus',
      album_artist: 'Ada Marsh',
      track_number: '4',
      label: 'Northline',
      comment: 'Vinyl rip',
    };

    render(Inspector, {
      track,
      previewTrackId: track.id,
      previewPlaying: false,
      onclose,
      onpreview,
      ontogglefavourite,
    });

    const aside = screen.getByRole('complementary', { name: 'Inspector' });
    expect(within(aside).getByRole('heading', { name: 'Track 3' })).toBeTruthy();
    expect(within(aside).getByRole('img', { name: 'Track 3 artwork' })).toBeTruthy();
    expect(aside.textContent).toContain('FLAC · 44.1 kHz · 16-bit');
    expect(aside.textContent).toContain('124.0');
    expect(aside.textContent).toContain('8A · Am');
    expect(aside.textContent).toContain('5:01');
    expect(aside.textContent).toContain('Night Bus');
    expect(aside.textContent).toContain('Northline');
    expect(aside.textContent).toContain('Vinyl rip');
    expect(aside.textContent).toContain('/music/3.flac');

    const playBtn = within(aside).getByRole('button', { name: 'Play Track 3' });
    expect(playBtn.getAttribute('aria-pressed')).toBe('false');
    await fireEvent.click(playBtn);
    expect(onpreview).toHaveBeenCalledWith(track);

    const favBtn = within(aside).getByRole('button', { name: 'Favourite track' });
    expect(favBtn.getAttribute('aria-pressed')).toBe('false');
    await fireEvent.click(favBtn);
    expect(ontogglefavourite).toHaveBeenCalledWith(track);

    await fireEvent.click(within(aside).getByRole('button', { name: 'Hide the inspector' }));
    expect(onclose).toHaveBeenCalledOnce();
  });

  it('displays collection pills and wires inspector add and remove controls', async () => {
    const onaddtocollection = vi.fn(async () => {});
    const onremovefromcollection = vi.fn(async () => {});
    const track = {
      ...fakeTrack(4),
      collections: [{ id: 1, name: 'Warm Up' }],
    };
    const collections = [
      { id: 1, name: 'Warm Up', position: 0, track_count: 1 },
      { id: 2, name: 'Peak Hour', position: 1, track_count: 0 },
    ];

    render(Inspector, {
      track,
      collections,
      onclose: vi.fn(),
      onaddtocollection,
      onremovefromcollection,
    });

    const aside = screen.getByRole('complementary', { name: 'Inspector' });
    await fireEvent.click(within(aside).getByRole('button', { name: 'Remove Warm Up' }));
    expect(onremovefromcollection).toHaveBeenCalledWith(1, [track.id]);

    await fireEvent.click(within(aside).getByRole('button', { name: 'Add to collection' }));
    await fireEvent.click(screen.getByRole('menuitemcheckbox', { name: /Peak Hour/ }));
    expect(onaddtocollection).toHaveBeenCalledWith(2, [track.id]);
  });

  it('shows the stored cover, takes a picked one and offers a proposed release cover', async () => {
    const track = { ...fakeTrack(0), artwork: 'a'.repeat(64) };
    const onpickartwork = vi.fn();
    const onacceptartwork = vi.fn();
    const onskipartwork = vi.fn();
    const { container } = render(Inspector, {
      track,
      onclose: vi.fn(),
      artwork: {
        track_id: track.id,
        artwork: track.artwork,
        proposed: { artwork: 'b'.repeat(64), provider: 'MusicBrainz' },
      },
      onpickartwork,
      onacceptartwork,
      onskipartwork,
    });

    const images = [...container.querySelectorAll('img')].map((image) => image.getAttribute('src'));
    expect(images).toEqual([`/artwork/${'a'.repeat(64)}`, `/artwork/${'b'.repeat(64)}`]);
    expect(screen.getByText('Release cover from MusicBrainz')).toBeTruthy();

    const picture = new File([new Uint8Array([0xff, 0xd8, 0xff])], 'cover.jpg', {
      type: 'image/jpeg',
    });
    const picker = container.querySelector<HTMLInputElement>('input[type="file"]');
    if (!picker) throw new Error('no picker');
    Object.defineProperty(picker, 'files', { value: [picture], configurable: true });
    await fireEvent.change(picker);
    expect(onpickartwork).toHaveBeenCalledWith(track.id, picture);

    await fireEvent.click(screen.getByRole('button', { name: 'Use cover' }));
    expect(onacceptartwork).toHaveBeenCalledWith(track.id);
    await fireEvent.click(screen.getByRole('button', { name: 'Skip' }));
    expect(onskipartwork).toHaveBeenCalledWith(track.id);
  });

  it('draws the dot wave when a track has no cover or its cover fails to load', async () => {
    const { container } = render(Inspector, {
      track: { ...fakeTrack(1), artwork: 'c'.repeat(64) },
      onclose: vi.fn(),
    });
    const cover = container.querySelector('.cover .thumb');
    expect(cover?.getAttribute('data-artwork')).toBe('image');

    const image = container.querySelector('.cover img');
    if (!image) throw new Error('no image');
    await fireEvent.error(image);

    expect(cover?.getAttribute('data-artwork')).toBe('wave');
    expect(container.querySelector('.cover canvas')).toBeTruthy();
  });

  it('explains when audio properties are missing and renders a quiet empty state', () => {
    const unreadable = { ...fakeTrack(0), artist: null, audio: null, format: 'mp3' };
    const { unmount } = render(Inspector, { track: unreadable, onclose: vi.fn() });

    expect(screen.getByText('No artist tag')).toBeTruthy();
    expect(screen.getByText(/could not be read as audio/)).toBeTruthy();
    unmount();

    render(Inspector, { track: undefined, onclose: vi.fn() });
    expect(screen.getByText(/Select a track in the table/)).toBeTruthy();
  });

  it('derives stable track seeds and readable lossless or lossy summaries', () => {
    expect(trackSeed('/a/first.wav')).toBe(trackSeed('/a/first.wav'));
    expect(trackSeed('/a/first.wav')).not.toBe(trackSeed('/a/second.wav'));
    expect(
      audioSummary('mp3', {
        duration_ms: 1000,
        sample_rate: 48_000,
        channels: 2,
        bit_depth: null,
        bitrate_kbps: 320,
      }),
    ).toBe('MP3 · 48 kHz · 320 kbps');
    expect(audioSummary('wav', null)).toBe('WAV');
  });
});
