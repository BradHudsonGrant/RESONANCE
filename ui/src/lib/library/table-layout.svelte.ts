import { DEFAULT_TABLE_LAYOUT } from '../ipc/bindings/DefaultTableLayout';
import type { LibraryTableLayout } from '../ipc/bindings/LibraryTableLayout';
import type { TableLayoutChange } from '../ipc/bindings/TableLayoutChange';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';

export type LoadTableLayout = () => Promise<LibraryTableLayout>;
export type ChangeTableLayout = (change: TableLayoutChange) => Promise<LibraryTableLayout>;

type SortQuery = Required<Pick<TrackQuery, 'sort' | 'direction' | 'then_sort' | 'then_direction'>>;

/** The track query's sort for a layout: its primary and secondary keys, else library order. */
export function sortQuery(layout: LibraryTableLayout): SortQuery {
  const [primary, secondary] = layout.sort;
  return {
    sort: primary?.field ?? 'index',
    direction: primary?.direction ?? 'asc',
    then_sort: secondary?.field ?? null,
    then_direction: secondary?.direction ?? null,
  };
}

export function sameSort(query: TrackQuery, wanted: SortQuery): boolean {
  return (
    JSON.stringify(query.sort ?? 'index') === JSON.stringify(wanted.sort) &&
    (query.direction ?? 'asc') === wanted.direction &&
    JSON.stringify(query.then_sort ?? null) === JSON.stringify(wanted.then_sort) &&
    (query.then_sort == null || (query.then_direction ?? 'asc') === wanted.then_direction)
  );
}

/**
 * The library table's columns as the core keeps them. The table draws the default layout until
 * the core answers, and every change goes to the core, which applies its rules and answers with
 * the layout to draw. Changes run one at a time in the order they were made.
 */
export class TableLayoutState {
  current = $state<LibraryTableLayout>(DEFAULT_TABLE_LAYOUT);

  #load: LoadTableLayout | undefined;
  #change: ChangeTableLayout | undefined;
  #queue: Promise<unknown> = Promise.resolve();

  constructor(load?: LoadTableLayout, change?: ChangeTableLayout) {
    this.#load = load;
    this.#change = change;
  }

  async load(): Promise<void> {
    const next = await this.#load?.();
    if (next) this.current = next;
  }

  change(change: TableLayoutChange): Promise<LibraryTableLayout> {
    const run = this.#queue.then(async () => {
      const next = await this.#change?.(change);
      if (next) this.current = next;
      return this.current;
    });
    this.#queue = run.catch(() => undefined);
    return run;
  }
}
