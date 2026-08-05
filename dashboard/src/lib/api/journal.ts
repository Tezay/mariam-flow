/** Reading the appliance journal. */

/** The families the journal sorts events into. */
export const EVENT_CATEGORIES = ['access', 'lifecycle', 'installation', 'nodes'] as const;
export type EventCategory = (typeof EVENT_CATEGORIES)[number];

/** Everything the appliance records about itself. */
export const EVENT_KINDS = [
  'login-succeeded',
  'login-failed',
  'login-throttled',
  'logged-out',
  'credential-reset',
  'started',
  'stopped',
  'configuration-changed',
  'service-opened',
  'service-closed',
  'stage-completed',
  'calibration-started',
  'calibration-stopped',
  'model-activated',
  'model-rejected',
  'node-appeared',
  'node-lost',
  'clock-stepped',
  'unknown',
] as const;
export type EventKind = (typeof EVENT_KINDS)[number];

/** One line of the appliance journal. */
export type RecordedEvent = {
  /** Row identifier, and the cursor a reader pages on. */
  id: number;
  ts_us: number;
  category: EventCategory;
  kind: EventKind;
  client?: string;
  detail?: string;
};

/**
 * Reads a slice of the journal, newest first.
 *
 * Paged on the row rather than on an offset: the journal is written while it
 * is read, and an offset would repeat some rows and skip others.
 */
export async function fetchEvents(page: {
  limit?: number;
  before?: number;
  after?: number;
  category?: EventCategory | null;
}): Promise<RecordedEvent[]> {
  const query = new URLSearchParams();
  if (page.limit !== undefined) query.set('limit', String(page.limit));
  if (page.before !== undefined) query.set('before', String(page.before));
  if (page.after !== undefined) query.set('after', String(page.after));
  if (page.category) query.set('category', page.category);
  try {
    const response = await fetch(`/api/events?${query}`);
    return response.ok ? ((await response.json()) as RecordedEvent[]) : [];
  } catch {
    return [];
  }
}
