/** When the site serves. */

import type { SaveOutcome } from './http';

/** The days of the week, in the order they are stored and shown. */
export const WEEKDAYS = [
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
  'saturday',
  'sunday',
] as const;

export type Weekday = (typeof WEEKDAYS)[number];

/** One serving interval, `HH:MM`, with `to` exclusive. */
export type Interval = { from: string; to: string };

/** A closed period, `YYYY-MM-DD`, both ends inclusive. */
export type Closure = { from: string; to: string };

/** When a site serves. */
export type ServiceWindow = {
  timezone: string;
  weekly: Record<Weekday, Interval[]>;
  closures: Closure[];
};

/** Reads the stored service schedule, or `null` when none is declared. */
export async function fetchServiceWindow(): Promise<ServiceWindow | null | 'unreachable'> {
  try {
    const response = await fetch('/api/service-window');
    if (!response.ok) {
      return 'unreachable';
    }
    return (await response.json()) as ServiceWindow | null;
  } catch {
    return 'unreachable';
  }
}

/** Replaces the stored schedule, or clears it when given `null`. */
export async function saveServiceWindow(window: ServiceWindow | null): Promise<SaveOutcome> {
  try {
    const response = await fetch('/api/service-window', {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(window),
    });
    if (response.ok) {
      return { kind: 'ok' };
    }
    if (response.status === 400) {
      const body = (await response.json().catch(() => null)) as { error?: string } | null;
      return { kind: 'refused', message: body?.error ?? '' };
    }
    return { kind: 'error' };
  } catch {
    return { kind: 'error' };
  }
}
