// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it } from 'vitest';

import { DEFAULT_TABLE_LAYOUT } from '../ipc/bindings/DefaultTableLayout';
import { KEY_WHEEL } from '../ipc/bindings/KeyWheel';
import SettingsSheet from '../shell/SettingsSheet.svelte';
import { cellText } from './columns';
import { FAKE_FILTER_FIELDS, fakeTrack } from './fake-library';
import FilterDrawer from './FilterDrawer.svelte';
import { keyCode, keyDetail, keyNotation, setKeyNotation } from './key-notation.svelte';

HTMLElement.prototype.showPopover ??= () => {};
HTMLElement.prototype.hidePopover ??= () => {};

const keyColumn = DEFAULT_TABLE_LAYOUT.columns.find((column) => column.id === 'key')!;

afterEach(() => {
  setKeyNotation('camelot');
  localStorage.clear();
});

describe('key notation', () => {
  it('reads every key of the wheel in each notation', () => {
    expect(KEY_WHEEL).toHaveLength(24);
    expect(new Set(KEY_WHEEL.map((key) => key.open_key)).size).toBe(24);
    for (const key of KEY_WHEEL) {
      expect(keyCode(key, 'camelot')).toBe(key.camelot);
      expect(keyCode(key, 'open_key')).toBe(key.open_key);
      expect(keyCode(key, 'musical')).toBe(key.name);
    }
    const aMinor = KEY_WHEEL.find((key) => key.camelot === '8A')!;
    expect(keyDetail(aMinor, 'camelot')).toBe('8A · Am');
    expect(keyDetail(aMinor, 'open_key')).toBe('1m · Am');
    expect(keyDetail(aMinor, 'musical')).toBe('Am · 8A');
  });

  it('defaults to Camelot and keeps a chosen notation for the next start', () => {
    expect(keyNotation.current).toBe('camelot');
    setKeyNotation('open_key');
    expect(localStorage.getItem('resonance-key-notation')).toBe('open_key');
    expect(cellText(keyColumn, fakeTrack(0))).toBe('1m');
  });

  it('changes the table, filter drawer and Settings at once when chosen in Settings', async () => {
    render(SettingsSheet, { open: true });
    const drawer = render(FilterDrawer, {
      open: true,
      query: {
        filters: [{ any_of: [{ column: 'key', condition: { op: 'any_of', values: ['8A'] } }] }],
      },
      fields: FAKE_FILTER_FIELDS,
      facets: null,
      onchange: () => {},
      onclear: () => {},
    });
    const keys = () => within(drawer.container).getByRole('group', { name: 'Key values' });
    expect(
      within(keys()).getByRole('button', { name: '8A (Am)' }).getAttribute('aria-pressed'),
    ).toBe('true');

    await fireEvent.click(screen.getByRole('tab', { name: 'Open Key' }));

    expect(keyNotation.current).toBe('open_key');
    expect(cellText(keyColumn, fakeTrack(0))).toBe('1m');
    const aMinor = within(keys()).getByRole('button', { name: '1m (Am)' });
    expect(aMinor.getAttribute('aria-pressed')).toBe('true');

    await fireEvent.click(screen.getByRole('tab', { name: 'Musical' }));
    expect(within(keys()).getByRole('button', { name: 'Am (8A)' })).toBeTruthy();
  });
});
