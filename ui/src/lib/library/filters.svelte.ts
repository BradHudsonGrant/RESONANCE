import { SvelteMap } from 'svelte/reactivity';

import type { DescribedFilter } from '../ipc/bindings/DescribedFilter';
import type { FilterField } from '../ipc/bindings/FilterField';
import type { FilterSummary } from '../ipc/bindings/FilterSummary';
import type { FilterTerm } from '../ipc/bindings/FilterTerm';
import type { QuickFilterChange } from '../ipc/bindings/QuickFilterChange';
import type { QuickFilters } from '../ipc/bindings/QuickFilters';
import type { SmartView } from '../ipc/bindings/SmartView';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';

/** What the filters need from the core. Every rule and every label comes from there. */
export interface FiltersClient {
  fields: () => Promise<FilterField[]>;
  summaries: (query: TrackQuery) => Promise<FilterSummary[]>;
  parse: (text: string) => Promise<DescribedFilter>;
  recordUse: (term: FilterTerm) => Promise<QuickFilters>;
  quickFilters: () => Promise<QuickFilters>;
  changeQuickFilters: (change: QuickFilterChange) => Promise<QuickFilters>;
  smartViews: () => Promise<SmartView[]>;
  saveView: (name: string, query: TrackQuery) => Promise<SmartView[]>;
  renameView: (viewId: number, name: string) => Promise<SmartView[]>;
  deleteView: (viewId: number) => Promise<SmartView[]>;
}

export function termsOf(query: TrackQuery): FilterTerm[] {
  return query.filters ?? [];
}

/** The same term built twice compares equal, whatever order its values came in. */
export function sameTerm(a: FilterTerm, b: FilterTerm): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** The query with `term` added, unless it is on already. */
export function withTerm(query: TrackQuery, term: FilterTerm): TrackQuery {
  const terms = termsOf(query);
  return terms.some((existing) => sameTerm(existing, term))
    ? query
    : withTerms(query, [...terms, term]);
}

/** The query with `terms` in place of its filters. */
export function withTerms(query: TrackQuery, terms: FilterTerm[]): TrackQuery {
  return { ...query, filters: terms.length > 0 ? terms : null };
}

/** Whatever holds the table's query. */
export interface QueryHolder {
  query: TrackQuery;
  setQuery: (next: TrackQuery) => Promise<void>;
}

/** The filters on the table, the bar's quick filters and the saved smart views. */
export class FilterState {
  quick = $state<QuickFilters>({ pinned: [], recommended: [], recommend: true });
  /** How each term reads and what it matches, by the term, so a token keeps its words while
   * its count is read again. */
  #summaries = new SvelteMap<string, FilterSummary>();
  views = $state<SmartView[]>([]);
  fields = $state<FilterField[]>([]);

  #client: FiltersClient | undefined;
  #summaryQuery: TrackQuery | null = null;
  #summaryInFlight = false;
  #fieldsLoad: Promise<FilterField[]> | undefined;

  constructor(client?: FiltersClient) {
    this.#client = client;
  }

  get available(): boolean {
    return this.#client !== undefined;
  }

  async load(): Promise<void> {
    if (!this.#client) return;
    const [quick, views] = await Promise.all([
      this.#client.quickFilters(),
      this.#client.smartViews(),
    ]);
    // A core too old to know quick filters answers with nothing, and the bar stays empty.
    if (quick) this.quick = quick;
    if (views) this.views = views;
  }

  /** The builder's columns, read once. */
  loadFields(): Promise<FilterField[]> {
    if (!this.#client) return Promise.resolve([]);
    this.#fieldsLoad ??= this.#client
      .fields()
      .then((fields) => {
        this.fields = fields ?? [];
        return this.fields;
      })
      .catch((error: unknown) => {
        this.#fieldsLoad = undefined;
        throw error;
      });
    return this.#fieldsLoad;
  }

  /**
   * Reads how the query's terms read and what each matches. One read is on the wire at a time
   * and only the newest query waits behind it, with no timer, so the counts keep up with typing.
   */
  refreshSummaries(query: TrackQuery): void {
    if (!this.#client) return;
    if (termsOf(query).length === 0) {
      this.#summaryQuery = null;
      return;
    }
    this.#summaryQuery = query;
    if (this.#summaryInFlight) return;
    void this.#readSummaries();
  }

  async #readSummaries() {
    const client = this.#client;
    if (!client) return;
    this.#summaryInFlight = true;
    try {
      while (this.#summaryQuery) {
        const asked = this.#summaryQuery;
        this.#summaryQuery = null;
        try {
          const next = await client.summaries(asked);
          termsOf(asked).forEach((term, at) => {
            const summary = next[at];
            if (summary) this.#summaries.set(JSON.stringify(term), summary);
          });
        } catch {
          // The tokens keep their last words until the next read answers.
        }
      }
    } finally {
      this.#summaryInFlight = false;
    }
  }

  summaryFor(term: FilterTerm): FilterSummary | undefined {
    return this.#summaries.get(JSON.stringify(term));
  }

  /** Keeps how a term reads that the core already gave, so its token has words at once. */
  remember(filter: DescribedFilter): void {
    const key = JSON.stringify(filter.term);
    if (!this.#summaries.has(key)) {
      this.#summaries.set(key, { label: filter.label, key: filter.key, count: null });
    }
  }

  /** Puts a filter on the table and counts the use, from any of the places that add one. */
  add(holder: QueryHolder, term: FilterTerm): void {
    const next = withTerm(holder.query, term);
    if (next === holder.query) return;
    void holder.setQuery(next);
    void this.recordUse(term);
  }

  /** Counts one use of a term, which can change the recommendations. */
  async recordUse(term: FilterTerm): Promise<void> {
    if (!this.#client) return;
    try {
      this.quick = await this.#client.recordUse(term);
    } catch {
      // A use that was not counted only leaves the recommendations as they were.
    }
  }

  async change(change: QuickFilterChange): Promise<void> {
    if (!this.#client) return;
    this.quick = await this.#client.changeQuickFilters(change);
  }

  parse(text: string): Promise<DescribedFilter> {
    if (!this.#client) return Promise.reject(new Error('Filters are not available.'));
    return this.#client.parse(text);
  }

  async saveView(name: string, query: TrackQuery): Promise<void> {
    if (!this.#client) return;
    this.views = await this.#client.saveView(name, query);
  }

  async renameView(viewId: number, name: string): Promise<void> {
    if (!this.#client) return;
    this.views = await this.#client.renameView(viewId, name);
  }

  async deleteView(viewId: number): Promise<void> {
    if (!this.#client) return;
    this.views = await this.#client.deleteView(viewId);
  }
}
