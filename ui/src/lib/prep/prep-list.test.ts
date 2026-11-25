import { describe, expect, it } from 'vitest';

import { nextListSort, withFindText, withListSort } from './prep-view';

describe('Prep list columns', () => {
  it('sorts ascending, then descending, then back to the stage order', () => {
    const first = nextListSort(null, 'label');
    expect(first).toEqual({ column: 'label', direction: 'asc' });
    const second = nextListSort(first, 'label');
    expect(second).toEqual({ column: 'label', direction: 'desc' });
    expect(nextListSort(second, 'label')).toBeNull();
    expect(nextListSort(second, 'year')).toEqual({ column: 'year', direction: 'asc' });
  });

  it('adds the sort and the find text to the view query', () => {
    const query = withListSort(withFindText({ text: 'house' }, 'harbour'), {
      column: 'year',
      direction: 'desc',
    });
    expect(query).toMatchObject({
      text: 'house harbour',
      sort: 'year',
      direction: 'desc',
      then_sort: 'title',
    });
    expect(withListSort(null, null)).toBeNull();
    expect(withFindText(null, '')).toBeNull();
  });
});
