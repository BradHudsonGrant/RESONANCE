import { SvelteMap } from 'svelte/reactivity';

export type ReadOverviews = (trackIds: readonly number[]) => Promise<(Uint8Array | null)[]>;

/** Several screens of rows on the largest display, at 560 bytes a track. */
const KEPT = 2_000;

/**
 * The waveform overviews of the rows on screen. One read is in flight at a time and only the
 * newest view waits behind it, so a fast scroll never queues reads for rows long gone.
 */
export class TrackOverviews {
  #read: ReadOverviews;
  #found = new SvelteMap<number, Uint8Array | null>();
  #reading = false;
  #waiting: readonly number[] | null = null;

  constructor(read: ReadOverviews) {
    this.#read = read;
  }

  /** Undefined until read, then null for a track without an overview yet. */
  get(trackId: number): Uint8Array | null | undefined {
    return this.#found.get(trackId);
  }

  /** Reads the overviews of the tracks in view that have not been read. */
  want(trackIds: readonly number[]): void {
    if (this.#reading) {
      this.#waiting = trackIds;
      return;
    }
    const unread = trackIds.filter((id) => !this.#found.has(id));
    if (unread.length > 0) void this.#load(unread);
  }

  /**
   * Forgets the tracks that had no overview, so they are read again once identification has
   * drawn them.
   */
  forgetMissing(): void {
    for (const [id, overview] of this.#found) if (overview === null) this.#found.delete(id);
  }

  async #load(trackIds: readonly number[]): Promise<void> {
    this.#reading = true;
    try {
      const overviews = await this.#read(trackIds);
      trackIds.forEach((id, index) => this.#found.set(id, overviews[index] ?? null));
      for (const id of this.#found.keys()) {
        if (this.#found.size <= KEPT) break;
        this.#found.delete(id);
      }
    } catch {
      // Rows keep their empty waveform, and the next view asks again.
    } finally {
      this.#reading = false;
      const waiting = this.#waiting;
      this.#waiting = null;
      if (waiting) this.want(waiting);
    }
  }
}
