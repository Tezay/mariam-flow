import { describe, expect, it } from 'vitest';

import { PAGE_SIZE, page, pageCount } from './paging';

describe('page', () => {
  const items = Array.from({ length: 20 }, (_, n) => n);

  it('shows one page worth at a time', () => {
    expect(page(items, 0)).toHaveLength(PAGE_SIZE);
    expect(page(items, 0)[0]).toBe(0);
    expect(page(items, 1)[0]).toBe(PAGE_SIZE);
  });

  it('clamps a page beyond the end rather than showing nothing', () => {
    // Deleting the last recording on the last page must not leave the reader
    // staring at an empty list.
    expect(page(items, 99)).toEqual(page(items, pageCount(items.length) - 1));
  });

  it('clamps a negative page too', () => {
    expect(page(items, -3)).toEqual(page(items, 0));
  });

  it('survives an empty history', () => {
    expect(page([], 0)).toEqual([]);
    expect(pageCount(0)).toBe(1);
  });
});
