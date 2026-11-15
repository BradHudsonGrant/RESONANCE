import { untrack } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';

import type { LibraryTrack } from '../ipc/bindings/LibraryTrack';
import type { TrackPage } from '../ipc/bindings/TrackPage';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';

export const PAGE_SIZE = 200;

/** About 12,000 rows: far more than fits on a screen, and small next to a large library. */
const KEPT_PAGES = 60;

/**
 * The core answers one read at a time, so reads asked for while a scrollbar is dragged would
 * queue behind each other and the page that is finally in view would wait for all of them.
 * Two in flight keeps the core busy, and the rest wait here, where a newer view replaces them.
 */
const READS_IN_FLIGHT = 2;

/** Rows past each edge of the view that are read before they come into it. */
const READ_AHEAD = PAGE_SIZE / 2;

export type ReadPage = (offset: number, limit: number, query: TrackQuery) => Promise<TrackPage>;

export type LibraryState =
  { kind: 'loading' } | { kind: 'ready' } | { kind: 'error'; message: string };

export function countActiveDrawerFilters(query: TrackQuery): number {
  let count = 0;
  if (query.bpm_min != null || query.bpm_max != null) count += 1;
  if (query.keys && query.keys.length > 0) count += 1;
  if (query.year_min != null || query.year_max != null) count += 1;
  if (query.label && query.label.trim().length > 0) count += 1;
  if (query.formats && query.formats.length > 0) count += 1;
  return count;
}

export function isQueryNarrowed(query: TrackQuery): boolean {
  return (
    Boolean(query.text && query.text.trim().length > 0) ||
    (query.pill !== undefined && query.pill !== null && query.pill !== 'all') ||
    countActiveDrawerFilters(query) > 0
  );
}

/**
 * The library as a table sees it: how many tracks match the active query, and the pages read so
 * far. Rows are read a page at a time as they come into view, so a library of any size opens as
 * fast as a small one.
 */
export class TrackPages {
  state = $state<LibraryState>({ kind: 'loading' });
  total = $state(0);
  libraryTotal = $state(0);
  query = $state<TrackQuery>({});
  scrollResetCount = $state(0);

  #read: ReadPage;
  #pages = new SvelteMap<number, LibraryTrack[]>();
  #selected = $state<number | null>(null);
  #selectedTrack = $state<LibraryTrack>();
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- bookkeeping that nothing renders
  #reading = new Set<number>();
  #waiting: number[] = [];
  #centre = 0;
  // A reload starts a new generation, and a page that arrives for an older one is dropped.
  #generation = 0;

  constructor(read: ReadPage) {
    this.#read = read;
  }

  get selected(): number | null {
    return this.#selected;
  }

  set selected(index: number | null) {
    if (index === this.#selected) return;
    this.#selected = index;
    this.#selectedTrack = index === null ? undefined : this.track(index);
  }

  /** Kept separately so scrolling can release its page without clearing the inspector. */
  get selectedTrack(): LibraryTrack | undefined {
    return this.#selectedTrack;
  }

  async load() {
    this.#generation += 1;
    this.selected = null;
    this.#pages.clear();
    this.#reading.clear();
    this.#waiting = [];
    this.#centre = 0;
    this.state = { kind: 'loading' };
    await this.#readPage(0, false);
  }

  /**
   * Switches search, filter or sort criteria while keeping existing rows visible until page 0 of
   * the new query arrives, so rapid typing never blanks the table.
   */
  async setQuery(next: TrackQuery) {
    this.query = next;
    if (this.state.kind !== 'ready') {
      await this.load();
      return;
    }
    this.#generation += 1;
    this.#reading.clear();
    this.#waiting = [];
    this.#centre = 0;
    await this.#readPage(0, true);
  }

  /**
   * Re-reads the pages currently held in memory without clearing rows or selection, so live source
   * updates and watcher events refresh the table in place.
   */
  async refresh() {
    if (this.state.kind !== 'ready') {
      await this.load();
      return;
    }
    this.#generation += 1;
    this.#reading.clear();
    const cached = [...this.#pages.keys()].sort(
      (a, b) => Math.abs(a + 0.5 - this.#centre) - Math.abs(b + 0.5 - this.#centre),
    );
    if (!cached.includes(0)) cached.unshift(0);
    const first = cached.shift() ?? 0;
    this.#waiting = cached;
    await this.#readPage(first, false);
  }

  /** Updates a track's favourite flag in cached pages and the selected track immediately. */
  setLocalFavourite(trackId: number, favourite: boolean) {
    for (const [page, tracks] of this.#pages.entries()) {
      const index = tracks.findIndex((item) => item.id === trackId);
      if (index !== -1) {
        const updated = [...tracks];
        updated[index] = { ...tracks[index]!, favourite };
        this.#pages.set(page, updated);
      }
    }
    if (this.#selectedTrack?.id === trackId) {
      this.#selectedTrack = { ...this.#selectedTrack, favourite };
    }
  }

  /** The track at a place in current query order, or nothing while its page is still on its way. */
  track(index: number): LibraryTrack | undefined {
    return this.#pages.get(Math.floor(index / PAGE_SIZE))?.[index % PAGE_SIZE];
  }

  /**
   * Reads the pages that hold the rows from `first` up to `end`, and the rows just past them,
   * nearest the middle of the view first. Pages asked for by an earlier view and not yet sent
   * are dropped.
   */
  want(first: number, end: number) {
    untrack(() => {
      if (this.state.kind !== 'ready' || end <= first) return;
      const firstPage = Math.floor(Math.max(0, first - READ_AHEAD) / PAGE_SIZE);
      const lastPage = Math.floor((Math.min(this.total, end + READ_AHEAD) - 1) / PAGE_SIZE);
      this.#centre = (first + end) / 2 / PAGE_SIZE;
      const missing: number[] = [];
      for (let page = firstPage; page <= lastPage; page += 1) {
        if (!this.#pages.has(page) && !this.#reading.has(page)) missing.push(page);
      }
      this.#waiting = missing.sort(
        (a, b) => Math.abs(a + 0.5 - this.#centre) - Math.abs(b + 0.5 - this.#centre),
      );
      this.#send();
    });
  }

  #send() {
    while (this.#reading.size < READS_IN_FLIGHT) {
      const page = this.#waiting.shift();
      if (page === undefined) return;
      void this.#readPage(page, false);
    }
  }

  async #readPage(page: number, replacePages: boolean) {
    const generation = this.#generation;
    this.#reading.add(page);
    try {
      const reply = await this.#read(page * PAGE_SIZE, PAGE_SIZE, this.query);
      if (generation !== this.#generation) return;
      if (replacePages) {
        this.#pages.clear();
        this.selected = null;
        this.scrollResetCount += 1;
      }
      this.total = reply.total;
      this.libraryTotal = reply.library_total;
      this.#pages.set(page, reply.tracks);
      for (const cached of [...this.#pages.keys()]) {
        if (cached > 0 && cached * PAGE_SIZE >= reply.total) {
          this.#pages.delete(cached);
        }
      }
      if (this.#selected !== null) {
        if (reply.total === 0) {
          this.selected = null;
        } else if (this.#selected >= reply.total) {
          this.#selected = reply.total - 1;
          this.#selectedTrack = this.track(this.#selected);
        } else if (Math.floor(this.#selected / PAGE_SIZE) === page) {
          this.#selectedTrack = reply.tracks[this.#selected % PAGE_SIZE];
        }
      }
      this.#keepNearby();
      if (this.state.kind === 'loading') this.state = { kind: 'ready' };
    } catch (error) {
      if (generation !== this.#generation) return;
      this.state = {
        kind: 'error',
        message: error instanceof Error ? error.message : String(error),
      };
    } finally {
      if (generation === this.#generation) {
        this.#reading.delete(page);
        this.#send();
      }
    }
  }

  /** Lets go of the pages farthest from the part of the table last asked for. */
  #keepNearby() {
    while (this.#pages.size > KEPT_PAGES) {
      let farthest = 0;
      let distance = -1;
      for (const page of this.#pages.keys()) {
        if (Math.abs(page - this.#centre) > distance) {
          distance = Math.abs(page - this.#centre);
          farthest = page;
        }
      }
      this.#pages.delete(farthest);
    }
  }
}
