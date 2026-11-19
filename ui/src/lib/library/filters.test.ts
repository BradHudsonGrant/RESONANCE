// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';

import type { FilterSummary } from '../ipc/bindings/FilterSummary';
import type { TrackQuery } from '../ipc/bindings/TrackQuery';
import CommandBar from '../shell/CommandBar.svelte';
import {
  described,
  FAKE_FILTER_FIELDS,
  fakeFilters,
  fakeTrack,
  MISSING_GENRE,
  settled,
} from './fake-library';
import FilterDrawer from './FilterDrawer.svelte';
import { FilterState, type FiltersClient } from './filters.svelte';
import Library from './Library.svelte';
import { TrackPages } from './track-pages.svelte';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

function drawer(query: TrackQuery = {}) {
  const onchange = vi.fn();
  const onadded = vi.fn();
  const view = render(FilterDrawer, {
    open: true,
    query,
    fields: FAKE_FILTER_FIELDS,
    facets: null,
    onchange,
    onadded,
    onclear: vi.fn(),
  });
  return { onchange, onadded, view, sheet: screen.getByRole('dialog', { name: 'Filters' }) };
}

describe('Filter builder', () => {
  it('builds "missing genre" and counts it once when the drawer closes', async () => {
    const { onchange, onadded, view, sheet } = drawer();
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Add a condition' }));
    expect(onchange).not.toHaveBeenCalled();

    await fireEvent.change(within(sheet).getByRole('combobox', { name: 'Condition' }), {
      target: { value: 'missing' },
    });
    expect(onchange).toHaveBeenLastCalledWith({ filters: [MISSING_GENRE] });

    await view.rerender({ query: { filters: [MISSING_GENRE] } });
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Done' }));
    await settled();
    expect(onadded).toHaveBeenCalledWith([MISSING_GENRE]);
  });

  it('waits for a value, and reads numbers in the units the column shows', async () => {
    const { onchange, sheet } = drawer();
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Add a condition' }));
    await fireEvent.change(within(sheet).getByRole('combobox', { name: 'Column' }), {
      target: { value: 'bitrate' },
    });
    await fireEvent.change(within(sheet).getByRole('combobox', { name: 'Condition' }), {
      target: { value: 'below' },
    });
    expect(onchange).not.toHaveBeenCalled();
    await fireEvent.input(within(sheet).getByRole('textbox', { name: 'Bitrate value' }), {
      target: { value: '320' },
    });
    expect(onchange).toHaveBeenLastCalledWith({
      filters: [{ any_of: [{ column: 'bitrate', condition: { op: 'below', value: 320 } }] }],
    });
  });

  it('joins the conditions of an any-of group with OR', async () => {
    const { onchange, sheet } = drawer();
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Add an any-of group' }));
    const columns = within(sheet).getAllByRole('combobox', { name: 'Column' });
    await fireEvent.change(columns[1]!, { target: { value: 'artwork' } });
    await fireEvent.change(within(sheet).getAllByRole('combobox', { name: 'Condition' })[0]!, {
      target: { value: 'missing' },
    });
    expect(onchange).toHaveBeenLastCalledWith({
      filters: [
        {
          any_of: [
            { column: 'genre', condition: { op: 'missing' } },
            { column: 'artwork', condition: { op: 'missing' } },
          ],
        },
      ],
    });
  });

  it('shows the filters already on the table', () => {
    const { sheet } = drawer({
      filters: [
        { any_of: [{ column: 'bitrate', condition: { op: 'range', min: 128, max: 256 } }] },
      ],
    });
    expect(
      (within(sheet).getByRole('combobox', { name: 'Column' }) as HTMLSelectElement).value,
    ).toBe('bitrate');
    expect(
      (within(sheet).getByRole('textbox', { name: 'From kbps' }) as HTMLInputElement).value,
    ).toBe('128');
  });
});

async function library() {
  const queries: TrackQuery[] = [];
  const pages = new TrackPages((offset, limit, query) => {
    queries.push(query);
    return Promise.resolve({
      total: query.filters?.length ? 2 : 4,
      library_total: 4,
      offset,
      tracks: Array.from({ length: Math.min(limit, 4) }, (_, at) => fakeTrack(offset + at)),
    });
  });
  await pages.load();
  const fake = fakeFilters();
  render(Library, { pages, onscan: vi.fn(), filters: fake.filters });
  await settled();
  const bar = screen.getByRole('toolbar', { name: 'Track filters' });
  return { ...fake, pages, queries, bar };
}

describe('Quick filters bar', () => {
  it('adds a recommended filter as a token with its count, and removes it', async () => {
    const { calls, queries, bar } = await library();
    const chip = within(bar).getByRole('button', { name: /^No genre.*recommended$/ });
    await fireEvent.click(chip);
    await settled();
    expect(queries.at(-1)?.filters).toEqual([MISSING_GENRE]);
    expect(calls.some((call) => call.name === 'recordUse')).toBe(true);

    const token = within(bar).getByRole('group', { name: 'Filter: No genre' });
    expect(within(token).getByText('2')).toBeTruthy();
    expect(within(bar).queryByRole('button', { name: /^No genre.*recommended$/ })).toBeNull();

    await fireEvent.click(within(token).getByRole('button', { name: 'Remove filter No genre' }));
    await settled();
    expect(queries.at(-1)?.filters).toBeNull();
    expect(within(bar).getByRole('button', { name: /^No genre.*recommended$/ })).toBeTruthy();
  });

  it('pins a filter from its token and unpins it from the chip', async () => {
    const { calls, bar } = await library();
    await fireEvent.click(within(bar).getByRole('button', { name: /^No genre.*recommended$/ }));
    await settled();
    await fireEvent.click(within(bar).getByRole('button', { name: /^No genre/ }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Pin to quick filters' }));
    await settled();
    expect(calls.at(-1)).toMatchObject({ name: 'change', arg: { kind: 'pin' } });

    await fireEvent.click(within(bar).getByRole('button', { name: 'Remove filter No genre' }));
    await settled();
    const pinned = within(bar).getByRole('button', { name: 'No genre' });
    await fireEvent.keyDown(pinned, { key: 'F10', shiftKey: true });
    const menu = screen.getByRole('menu', { name: 'Quick filter No genre' });
    // The only chip cannot move, so focus starts on the first item that can be chosen.
    expect(
      within(menu).getByRole('menuitem', { name: 'Move left' }).getAttribute('aria-disabled'),
    ).toBe('true');
    expect(document.activeElement).toBe(within(menu).getByRole('menuitem', { name: 'Remove' }));
    await fireEvent.click(within(menu).getByRole('menuitem', { name: 'Remove' }));
    await settled();
    expect(calls.at(-1)).toMatchObject({ name: 'change', arg: { kind: 'remove' } });
    expect(within(bar).queryByRole('button', { name: 'No genre' })).toBeNull();
  });

  it('shows no recommendations once they are turned off', async () => {
    const { filters, bar } = await library();
    await filters.change({ kind: 'set_recommend', on: false });
    await settled();
    expect(within(bar).queryByRole('button', { name: /^No genre.*recommended$/ })).toBeNull();
  });

  it('saves the filters as a smart view', async () => {
    const { calls, bar } = await library();
    await fireEvent.click(within(bar).getByRole('button', { name: /^No genre.*recommended$/ }));
    await settled();
    await fireEvent.click(within(bar).getByRole('button', { name: /^No genre/ }));
    await fireEvent.click(screen.getByRole('menuitem', { name: 'Save as smart view' }));
    const sheet = screen.getByRole('dialog', { name: 'Save as smart view' });
    expect((within(sheet).getByRole('textbox', { name: 'Name' }) as HTMLInputElement).value).toBe(
      'No genre',
    );
    await fireEvent.click(within(sheet).getByRole('button', { name: 'Save view' }));
    await settled();
    expect(calls.at(-1)).toMatchObject({
      name: 'saveView',
      arg: { name: 'No genre', query: { filters: [MISSING_GENRE] } },
    });
  });
});

describe('Filter summaries', () => {
  it('keeps one read on the wire and only the newest query waiting', async () => {
    const answers: ((value: FilterSummary[]) => void)[] = [];
    const summaries = vi.fn(() => new Promise<FilterSummary[]>((resolve) => answers.push(resolve)));
    const state = new FilterState({ ...quietClient(), summaries });
    const query = (count: number): TrackQuery => ({
      filters: Array.from({ length: count }, (_, at) => ({
        any_of: [{ column: 'bitrate', condition: { op: 'below', value: at + 100 } }],
      })),
    });
    state.refreshSummaries(query(1));
    state.refreshSummaries(query(2));
    state.refreshSummaries(query(3));
    expect(summaries).toHaveBeenCalledTimes(1);
    answers[0]!([{ label: 'one', key: 'a', count: null }]);
    await settled();
    expect(summaries).toHaveBeenCalledTimes(2);
    expect(summaries).toHaveBeenLastCalledWith(query(3));
  });
});

/** A client whose every call answers at once with nothing, for a test that swaps one call. */
function quietClient(): FiltersClient {
  const quick = { pinned: [], recommended: [], recommend: true };
  return {
    fields: async () => FAKE_FILTER_FIELDS,
    summaries: async () => [],
    parse: async () => described(MISSING_GENRE),
    recordUse: async () => quick,
    quickFilters: async () => quick,
    changeQuickFilters: async () => quick,
    smartViews: async () => [],
    saveView: async () => [],
    renameView: async () => [],
    deleteView: async () => [],
  };
}

describe('Command bar filters', () => {
  it('reads a / filter in the core and adds it on Enter', async () => {
    const parseFilter = vi.fn(async (text: string) => {
      if (text === 'missing genre') return described(MISSING_GENRE);
      throw new Error('That is not a filter.');
    });
    const onfilter = vi.fn();
    render(CommandBar, { commands: [], inputId: 'commands', parseFilter, onfilter });
    const input = screen.getByRole('combobox', { name: 'Commands' }) as HTMLInputElement;
    await fireEvent.focus(input);

    await fireEvent.input(input, { target: { value: '/' } });
    expect(
      screen.getByRole('option', { name: '/ missing genre — Tracks with no genre' }),
    ).toBeTruthy();

    await fireEvent.input(input, { target: { value: '/build set' } });
    await settled();
    expect(screen.getByRole('option', { name: 'That is not a filter.' })).toBeTruthy();

    await fireEvent.input(input, { target: { value: '/missing genre' } });
    await settled();
    expect(parseFilter).toHaveBeenLastCalledWith('missing genre');
    expect(screen.getByRole('option', { name: 'Filter: No genre' })).toBeTruthy();
    await fireEvent.keyDown(input, { key: 'Enter' });
    expect(onfilter).toHaveBeenCalledWith(described(MISSING_GENRE));
  });
});
