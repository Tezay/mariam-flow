/**
 * The service-hours editor's pure logic.
 *
 * The rules here mirror the ones the appliance enforces (`schedule.rs`).
 * Duplicating them is deliberate: the appliance stays the authority — it
 * refuses what it refuses, whatever the browser thinks — but an editor that
 * only learns of a mistake after a round trip cannot point at the row that
 * caused it while the operator is still looking at it.
 *
 * One divergence is intentional. The appliance answers with the *first*
 * problem it finds, because an API returns one error. The screen shows
 * *every* problem at once, because seven days are edited together and fixing
 * them one refusal at a time is the slow way through.
 */

import { WEEKDAYS, type Interval, type ServiceWindow, type Weekday } from './api';

/** A time of day the appliance would accept, `HH:MM` on a real clock. */
export function isValidTime(value: string): boolean {
  const match = /^(\d{2}):(\d{2})$/.exec(value);
  if (!match) {
    return false;
  }
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  return hours < 24 && minutes < 60;
}

/** Minutes since midnight, for comparing and sorting times. */
export function minutesOf(time: string): number {
  const [hours, minutes] = time.split(':');
  return Number(hours) * 60 + Number(minutes);
}

/** What is wrong with one day's intervals. */
export type Problem =
  /** A time that is not `HH:MM` on a real clock. */
  | { day: Weekday; kind: 'time'; value: string }
  /** An interval that ends before, or when, it starts. */
  | { day: Weekday; kind: 'order'; interval: Interval }
  /** Two intervals of one day that overlap. */
  | { day: Weekday; kind: 'overlap'; first: Interval; second: Interval };

/**
 * Every problem in a week, in the order a reader scans the screen.
 *
 * Days are walked in their displayed order and each day's own problems are
 * reported, so the list can be filtered per row without sorting it again.
 */
export function validateWeek(weekly: Record<Weekday, Interval[]>): Problem[] {
  const problems: Problem[] = [];

  for (const day of WEEKDAYS) {
    const intervals = weekly[day] ?? [];

    // Malformed times first: everything below compares them as numbers, and
    // comparing a half-typed "0" against a real time says nothing useful.
    const malformed = intervals.some((interval) => {
      for (const value of [interval.from, interval.to]) {
        if (!isValidTime(value)) {
          problems.push({ day, kind: 'time', value });
          return true;
        }
      }
      return false;
    });
    if (malformed) {
      continue;
    }

    for (const interval of intervals) {
      if (minutesOf(interval.to) <= minutesOf(interval.from)) {
        problems.push({ day, kind: 'order', interval });
      }
    }

    const sorted = [...intervals].sort((a, b) => minutesOf(a.from) - minutesOf(b.from));
    for (let index = 1; index < sorted.length; index += 1) {
      const first = sorted[index - 1];
      const second = sorted[index];
      if (minutesOf(second.from) < minutesOf(first.to)) {
        problems.push({ day, kind: 'overlap', first, second });
      }
    }
  }

  return problems;
}

/**
 * Whether a schedule never opens.
 *
 * The appliance accepts this — a site may genuinely be shut — so it is a
 * warning rather than a refusal. It is worth saying out loud all the same:
 * an appliance that never opens never estimates, and a week left empty by
 * accident looks exactly like one emptied on purpose.
 */
export function isNeverOpen(weekly: Record<Weekday, Interval[]>): boolean {
  return WEEKDAYS.every((day) => (weekly[day] ?? []).length === 0);
}

/** A week with no hours on any day. */
export function emptyWeek(): Record<Weekday, Interval[]> {
  return {
    monday: [],
    tuesday: [],
    wednesday: [],
    thursday: [],
    friday: [],
    saturday: [],
    sunday: [],
  };
}

/**
 * The same intervals on every day of the week.
 *
 * Most sites keep one set of hours from Monday to Friday, so typing them
 * seven times is the tedium this removes. Copies are made per day: sharing
 * one array would make editing Tuesday change Monday too.
 */
export function copyDayToAll(
  weekly: Record<Weekday, Interval[]>,
  source: Weekday,
): Record<Weekday, Interval[]> {
  const intervals = weekly[source] ?? [];
  const copied = emptyWeek();
  for (const day of WEEKDAYS) {
    copied[day] = intervals.map((interval) => ({ ...interval }));
  }
  return copied;
}

/** A fresh interval, placed at hours a service plausibly keeps. */
export function newInterval(existing: Interval[]): Interval {
  if (existing.length === 0) {
    return { from: '09:00', to: '17:00' };
  }
  // Start after whatever already ends latest, so a second interval added to
  // a morning does not land on top of it and read as an error immediately.
  const latest = Math.max(...existing.map((interval) => minutesOf(interval.to)));
  const from = Math.min(latest + 60, 22 * 60);
  return { from: asTime(from), to: asTime(Math.min(from + 120, 23 * 60 + 59)) };
}

function asTime(minutes: number): string {
  const hours = Math.floor(minutes / 60);
  return `${String(hours).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
}

/**
 * The time zone this browser believes it is in.
 *
 * Used to pre-fill the field, never to decide it: an appliance is almost
 * always configured on site, so the browser's zone is the site's zone, but
 * the operator can always say otherwise.
 */
export function browserTimeZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
  } catch {
    return 'UTC';
  }
}

/**
 * Every IANA zone this browser knows, or a usable handful if it cannot say.
 *
 * The fallback matters less for its contents than for existing: a browser
 * without `supportedValuesOf` must still be able to name a zone, and the
 * appliance refuses anything it does not recognise anyway.
 */
export function timeZoneNames(): string[] {
  const supported = (Intl as unknown as { supportedValuesOf?: (key: string) => string[] })
    .supportedValuesOf;
  if (typeof supported === 'function') {
    try {
      const zones = supported('timeZone');
      if (zones.length > 0) {
        return zones;
      }
    } catch {
      // Falls through to the short list below.
    }
  }
  const detected = browserTimeZone();
  const fallback = ['UTC', 'Europe/Paris', 'Europe/London', 'Europe/Brussels', 'Europe/Madrid'];
  return fallback.includes(detected) ? fallback : [detected, ...fallback];
}

/** A schedule with nothing declared, ready to be filled in. */
export function emptyWindow(timezone: string): ServiceWindow {
  return { timezone, weekly: emptyWeek(), closures: [] };
}

/** A deep copy, so editing a draft never touches what was loaded. */
export function cloneWindow(window: ServiceWindow): ServiceWindow {
  const weekly = emptyWeek();
  for (const day of WEEKDAYS) {
    weekly[day] = (window.weekly?.[day] ?? []).map((interval) => ({ ...interval }));
  }
  return {
    timezone: window.timezone,
    weekly,
    closures: window.closures?.map((c) => ({ ...c })) ?? [],
  };
}

/**
 * When the service next changes, as a phrase.
 *
 * The instant is rendered in the reader's own zone rather than the site's:
 * it answers "when, for me", and the two coincide whenever the appliance is
 * being configured on site. The weekday is dropped when the change falls on
 * the current day, where naming it adds nothing.
 */
export function formatNextChange(
  changesAtUs: number,
  nowUs: number,
  locale: string,
  hour12: boolean,
): string {
  const at = new Date(changesAtUs / 1000);
  const now = new Date(nowUs / 1000);
  const time = at.toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit', hour12 });
  if (at.toDateString() === now.toDateString()) {
    return time;
  }
  return `${at.toLocaleDateString(locale, { weekday: 'long' })} ${time}`;
}
