import { describe, expect, it } from 'vitest';

import { RowHeights } from './row-heights';

describe('row heights', () => {
  it('places rows at the base height until they are measured', () => {
    const rows = new RowHeights(100_000, 33);
    expect(rows.total).toBe(3_300_000);
    expect(rows.offsetOf(50_000)).toBe(1_650_000);
    expect(rows.indexAt(0)).toBe(0);
    expect(rows.indexAt(32)).toBe(0);
    expect(rows.indexAt(33)).toBe(1);
    expect(rows.indexAt(10_000_000)).toBe(99_999);
  });

  it('moves every later row when one grows and finds rows by offset', () => {
    const rows = new RowHeights(10, 33);
    expect(rows.set(2, 69)).toBe(36);
    expect(rows.set(2, 69)).toBe(0);
    expect(rows.offsetOf(2)).toBe(66);
    expect(rows.offsetOf(3)).toBe(135);
    expect(rows.total).toBe(366);
    expect(rows.indexAt(100)).toBe(2);
    expect(rows.indexAt(135)).toBe(3);
    rows.set(2, 33);
    expect(rows.indexAt(100)).toBe(3);
  });

  it('forgets measured rows on reset', () => {
    const rows = new RowHeights(4, 33);
    rows.set(0, 51);
    rows.reset(6);
    expect(rows.total).toBe(198);
    expect(rows.heightOf(0)).toBe(33);
  });
});
