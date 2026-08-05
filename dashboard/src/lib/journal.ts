/**
 * Reading the appliance journal.
 *
 * The appliance records what happened; this decides how a reader is shown it,
 * which is mostly a matter of grouping by day and of never letting the list
 * move while it is being read.
 */

import { type EventKind, type RecordedEvent } from './api/journal';

/** How many rows a page of the journal holds. */
export const PAGE = 25;

/**
 * How many pages load themselves before a reader has to ask.
 *
 * A journal is often read to reach a particular hour, and a list that never
 * stops growing is a list that cannot be left. The bound also keeps the page
 * off a modest phone's limits: four pages is a hundred rows.
 */
export const AUTO_PAGES = 4;

/** How loudly a row asks to be noticed. */
export type Tone = 'fault' | 'attention' | 'plain';

/**
 * What a row means for whoever is reading.
 *
 * Colour is spent only where something needs doing. A journal where every
 * family has its own hue is a wall of colour in which an incident is exactly
 * as visible as a routine save.
 */
export function toneOf(kind: EventKind): Tone {
  switch (kind) {
    // Someone is trying secrets.
    case 'login-failed':
    case 'login-throttled':
      return 'fault';
    // The appliance is running but degraded, or something changed that an
    // operator would want to have noticed.
    case 'model-rejected':
    case 'node-lost':
    case 'credential-reset':
    case 'unknown':
      return 'attention';
    default:
      return 'plain';
  }
}

/** One day of the journal, newest day first. */
export type Day = { key: string; events: RecordedEvent[] };

/**
 * Groups a flat list into days, preserving order.
 *
 * The day is taken in the reader's own zone rather than UTC: someone reading
 * an incident at half past midnight is looking for it under today.
 */
export function byDay(events: RecordedEvent[], locale: string): Day[] {
  const days: Day[] = [];
  for (const event of events) {
    const key = new Date(event.ts_us / 1000).toLocaleDateString(locale, {
      weekday: 'long',
      day: 'numeric',
      month: 'long',
    });
    const last = days.at(-1);
    if (last?.key === key) {
      last.events.push(event);
    } else {
      days.push({ key, events: [event] });
    }
  }
  return days;
}

/**
 * Merges rows that arrived while the list was being read.
 *
 * Prepended rather than refetched: the poll already carries them, and asking
 * again would give a different answer from the one the count was based on.
 */
export function prepend(arrived: RecordedEvent[], shown: RecordedEvent[]): RecordedEvent[] {
  const known = new Set(shown.map((event) => event.id));
  return [...arrived.filter((event) => !known.has(event.id)), ...shown];
}

/** Whether a further page is worth asking for. */
export function hasMore(lastPage: RecordedEvent[]): boolean {
  return lastPage.length === PAGE;
}

/** The row a further page reads back from. */
export function oldest(events: RecordedEvent[]): number | undefined {
  return events.at(-1)?.id;
}

/** The row a poll reads forward from. */
export function newest(events: RecordedEvent[]): number | undefined {
  return events[0]?.id;
}
