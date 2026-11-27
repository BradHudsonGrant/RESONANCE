import { describe, expect, it, vi } from 'vitest';

import { TrackOverviews } from './overviews.svelte';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => (resolve = done));
  return { promise, resolve };
}

const column = (peak: number) => new Uint8Array([peak, 0, 0, 0]);

describe('TrackOverviews', () => {
  it('reads only tracks not read yet and keeps null for a track without one', async () => {
    const read = vi.fn((ids: readonly number[]) =>
      Promise.resolve(ids.map((id) => (id === 2 ? null : column(id)))),
    );
    const overviews = new TrackOverviews(read);

    overviews.want([1, 2]);
    await vi.waitFor(() => expect(overviews.get(2)).toBeNull());
    overviews.want([1, 2, 3]);
    await vi.waitFor(() => expect(overviews.get(3)).toBeDefined());

    expect(read.mock.calls).toEqual([[[1, 2]], [[3]]]);
    expect([...overviews.get(1)!]).toEqual([1, 0, 0, 0]);
  });

  it('keeps one read in flight and only the newest view waiting', async () => {
    const first = deferred<(Uint8Array | null)[]>();
    const read = vi
      .fn<(ids: readonly number[]) => Promise<(Uint8Array | null)[]>>()
      .mockReturnValueOnce(first.promise)
      .mockImplementation((ids) => Promise.resolve(ids.map((id) => column(id))));
    const overviews = new TrackOverviews(read);

    overviews.want([1]);
    overviews.want([5, 6]);
    overviews.want([9]);
    first.resolve([column(1)]);
    await vi.waitFor(() => expect(overviews.get(9)).toBeDefined());

    expect(read.mock.calls).toEqual([[[1]], [[9]]]);
    expect(overviews.get(5)).toBeUndefined();
  });

  it('reads a track again after forgetting the ones that had none', async () => {
    let drawn = false;
    const read = vi.fn((ids: readonly number[]) =>
      Promise.resolve(ids.map((id) => (drawn ? column(id) : null))),
    );
    const overviews = new TrackOverviews(read);
    overviews.want([4]);
    await vi.waitFor(() => expect(overviews.get(4)).toBeNull());

    drawn = true;
    overviews.forgetMissing();
    overviews.want([4]);

    await vi.waitFor(() => expect(overviews.get(4)).toBeInstanceOf(Uint8Array));
  });

  it('leaves rows empty when a read fails and asks again next time', async () => {
    const read = vi
      .fn<(ids: readonly number[]) => Promise<(Uint8Array | null)[]>>()
      .mockRejectedValueOnce(new Error('Waveforms did not load (500).'))
      .mockResolvedValue([column(8)]);
    const overviews = new TrackOverviews(read);

    overviews.want([8]);
    await vi.waitFor(() => expect(read).toHaveBeenCalledTimes(1));
    await Promise.resolve();
    expect(overviews.get(8)).toBeUndefined();
    overviews.want([8]);

    await vi.waitFor(() => expect(overviews.get(8)).toBeInstanceOf(Uint8Array));
  });
});
