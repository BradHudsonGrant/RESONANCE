import { describe, expect, it } from 'vitest';

import { CoreError } from '../ipc/client';
import { fakeLibrary, fakeTrack, settled } from './fake-library';
import { PAGE_SIZE, TrackPages } from './track-pages.svelte';

describe('TrackPages', () => {
  it('reads the first page and the size of the library when it loads', async () => {
    const { asked, pages } = fakeLibrary(1000);

    await pages.load();

    expect(pages.state).toEqual({ kind: 'ready' });
    expect(pages.total).toBe(1000);
    expect(pages.track(0)?.title).toBe('Track 0');
    expect(pages.track(PAGE_SIZE)).toBeUndefined();
    expect(asked).toEqual([0]);
  });

  it('reads each page in view once, however often it is asked for', async () => {
    const { asked, pages } = fakeLibrary(1000);
    await pages.load();

    pages.want(150, 450);
    pages.want(150, 450);
    await settled();

    expect(asked).toEqual([0, 200, 400]);
    expect(pages.track(449)?.title).toBe('Track 449');
  });

  it('keeps the pages near the view and reads a distant one again when it comes back', async () => {
    const { asked, pages } = fakeLibrary(100_000);
    await pages.load();

    for (let page = 1; page <= 80; page += 1) {
      pages.want(page * PAGE_SIZE, page * PAGE_SIZE + 1);
      await settled();
    }

    expect(pages.track(0)).toBeUndefined();
    expect(pages.track(80 * PAGE_SIZE)?.title).toBe(`Track ${80 * PAGE_SIZE}`);

    pages.want(0, 1);
    await settled();

    expect(asked.filter((offset) => offset === 0)).toHaveLength(2);
    expect(pages.track(0)?.title).toBe('Track 0');
  });

  it('reads the rows just past the view before they come into it', async () => {
    const { asked, pages } = fakeLibrary(1000);
    await pages.load();

    pages.want(150, 190);
    await settled();

    expect(asked).toEqual([0, 200]);
  });

  it('sends two reads at a time and drops the ones a newer view no longer needs', async () => {
    const asked: number[] = [];
    const answers: (() => void)[] = [];
    const pages = new TrackPages(
      (offset, limit) =>
        new Promise((resolve) => {
          asked.push(offset);
          answers.push(() =>
            resolve({ total: 100_000, library_total: 100_000, offset, tracks: [] }),
          );
          if (offset === 0) answers.shift()?.();
          void limit;
        }),
    );
    await pages.load();

    pages.want(10_000, 10_800);
    pages.want(90_000, 90_040);
    answers.shift()?.();
    answers.shift()?.();
    await settled();

    expect(asked.slice(0, 3)).toEqual([0, 10_200, 10_400]);
    expect(asked.slice(3).every((offset) => offset >= 89_800)).toBe(true);
  });

  it('says what went wrong when the core cannot answer', async () => {
    const pages = new TrackPages(() =>
      Promise.reject(new CoreError('disconnected', 'The core is not running.')),
    );

    await pages.load();

    expect(pages.state).toEqual({ kind: 'error', message: 'The core is not running.' });
  });

  it('starts again from the top on a reload', async () => {
    const { asked, pages } = fakeLibrary(1000);
    await pages.load();
    pages.want(400, 401);
    await settled();

    await pages.load();

    expect(pages.track(400)).toBeUndefined();
    expect(asked).toEqual([0, 400, 200, 0]);
  });

  it('keeps the selected track when scrolling lets go of its page', async () => {
    const { pages } = fakeLibrary(100_000);
    await pages.load();
    pages.selected = 0;

    for (let page = 1; page <= 80; page += 1) {
      pages.want(page * PAGE_SIZE, page * PAGE_SIZE + 1);
      await settled();
    }

    expect(pages.track(0)).toBeUndefined();
    expect(pages.selected).toBe(0);
    expect(pages.selectedTrack).toEqual(fakeTrack(0));

    pages.selected = 80 * PAGE_SIZE;
    expect(pages.selectedTrack).toEqual(fakeTrack(80 * PAGE_SIZE));
    pages.selected = null;
    expect(pages.selectedTrack).toBeUndefined();
  });

  it('shows a newly selected track when its page arrives', async () => {
    const { pages } = fakeLibrary(1000);
    await pages.load();
    pages.selected = 0;

    pages.selected = 400;
    expect(pages.selectedTrack).toBeUndefined();
    pages.want(400, 401);
    await settled();

    expect(pages.selectedTrack).toEqual(fakeTrack(400));
  });

  it('keeps a newer selection when the previous selection finishes loading', async () => {
    const { pages } = fakeLibrary(1000);
    await pages.load();

    pages.selected = 400;
    pages.want(400, 401);
    pages.selected = 1;
    await settled();

    expect(pages.selectedTrack).toEqual(fakeTrack(1));
  });

  it('clears the selection when reloading changes the library order', async () => {
    const { pages } = fakeLibrary(1000);
    await pages.load();
    pages.selected = 0;

    await pages.load();

    expect(pages.selected).toBeNull();
    expect(pages.selectedTrack).toBeUndefined();
  });

  it('refreshes loaded pages in place while keeping selection and clamping when tracks shrink', async () => {
    let currentTotal = 10;
    const pages = new TrackPages((offset, limit) =>
      Promise.resolve({
        total: currentTotal,
        library_total: currentTotal,
        offset,
        tracks: Array.from(
          { length: Math.max(0, Math.min(limit, currentTotal - offset)) },
          (_, i) => fakeTrack(offset + i),
        ),
      }),
    );

    await pages.load();
    pages.selected = 4;
    currentTotal = 12;
    await pages.refresh();

    expect(pages.state).toEqual({ kind: 'ready' });
    expect(pages.total).toBe(12);
    expect(pages.selected).toBe(4);
    expect(pages.selectedTrack).toEqual(fakeTrack(4));

    currentTotal = 3;
    await pages.refresh();
    expect(pages.total).toBe(3);
    expect(pages.selected).toBe(2);
    expect(pages.selectedTrack).toEqual(fakeTrack(2));
  });

  it('switches query without clearing cached rows until page 0 arrives, then resets scroll', async () => {
    let releaseSecond!: () => void;
    let call = 0;
    const pages = new TrackPages((offset, limit, query) => {
      call += 1;
      if (call === 1) {
        return Promise.resolve({
          total: 2,
          library_total: 10,
          offset,
          tracks: [fakeTrack(0), fakeTrack(1)].slice(0, limit),
        });
      }
      return new Promise((resolve) => {
        releaseSecond = () =>
          resolve({
            total: query?.text ? 1 : 2,
            library_total: 10,
            offset,
            tracks: [{ ...fakeTrack(5), favourite: true }],
          });
      });
    });

    await pages.load();
    pages.selected = 0;
    const initialResets = pages.scrollResetCount;

    const pending = pages.setQuery({ text: 'ada', sort: 'title', direction: 'asc' });
    expect(pages.track(0)?.title).toBe('Track 0');
    expect(pages.scrollResetCount).toBe(initialResets);

    releaseSecond();
    await pending;

    expect(pages.total).toBe(1);
    expect(pages.libraryTotal).toBe(10);
    expect(pages.selected).toBeNull();
    expect(pages.scrollResetCount).toBe(initialResets + 1);
    expect(pages.track(0)?.id).toBe(6);

    pages.setLocalFavourite(6, false);
    expect(pages.track(0)?.favourite).toBe(false);
  });
});
