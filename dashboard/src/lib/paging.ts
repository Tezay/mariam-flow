/** How many rows a page of a history shows. */
export const PAGE_SIZE = 8;

/** The slice a given page shows, clamped to what exists. */
export function page<T>(items: T[], index: number): T[] {
  const pages = Math.max(1, Math.ceil(items.length / PAGE_SIZE));
  const clamped = Math.min(Math.max(0, index), pages - 1);
  return items.slice(clamped * PAGE_SIZE, clamped * PAGE_SIZE + PAGE_SIZE);
}

/** How many pages a total needs. */
export function pageCount(total: number): number {
  return Math.max(1, Math.ceil(total / PAGE_SIZE));
}
