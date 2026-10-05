/**
 * How the history of estimates is laid out on a time axis.
 *
 * The appliance stores a row for each minute it estimated in and none for the
 * others, so what is missing between two rows is worked out here.
 */

import { type MinuteSummary } from './api/live';

export const MINUTE_US = 60_000_000;

/** The stretches of history on offer, in minutes, the first being the default. */
export const RANGES = [30, 60, 180, 360] as const;

export type Range = (typeof RANGES)[number];

export function isRange(value: number): value is Range {
  return (RANGES as readonly number[]).includes(value);
}

export function rangeLabel(minutes: number): string {
  return minutes < 60 ? `${minutes}min` : `${minutes / 60}h`;
}

/** A part of the window: minutes that follow one another, or none at all. */
export type Stretch =
  | {
      kind: 'run';
      from_us: number;
      to_us: number;
      minutes: MinuteSummary[];
      /** Runs up to the present, where the estimate in progress continues it. */
      live: boolean;
    }
  | { kind: 'gap'; from_us: number; to_us: number };

/** A stored minute that ended longer ago than this is not the one before the present. */
const FOLLOWED_WITHIN_US = 2 * MINUTE_US;

/**
 * Cuts the window into what was estimated and what was not.
 *
 * The minute in progress is not stored until it ends, so while the appliance
 * is estimating the last run is carried to the present; when it has only just
 * resumed, the present is a run of its own with no stored minute yet.
 */
export function stretches(
  minutes: MinuteSummary[],
  fromUs: number,
  toUs: number,
  estimating: boolean,
): Stretch[] {
  const parts: Stretch[] = [];
  let cursor = fromUs;
  for (const minute of minutes) {
    const end = minute.minute_us + MINUTE_US;
    if (end <= fromUs || minute.minute_us >= toUs) {
      continue;
    }
    const last = parts[parts.length - 1];
    if (last?.kind === 'run' && minute.minute_us === cursor) {
      last.minutes.push(minute);
      last.to_us = end;
    } else {
      if (minute.minute_us > cursor) {
        parts.push({ kind: 'gap', from_us: cursor, to_us: minute.minute_us });
      }
      parts.push({
        kind: 'run',
        from_us: Math.max(minute.minute_us, fromUs),
        to_us: end,
        minutes: [minute],
        live: false,
      });
    }
    cursor = end;
  }
  if (cursor >= toUs) {
    return parts;
  }

  const last = parts[parts.length - 1];
  if (!estimating) {
    parts.push({ kind: 'gap', from_us: cursor, to_us: toUs });
  } else if (last?.kind === 'run' && toUs - cursor <= FOLLOWED_WITHIN_US) {
    last.to_us = toUs;
    last.live = true;
  } else {
    const resumed = Math.max(cursor, toUs - (toUs % MINUTE_US));
    if (resumed > cursor) {
      parts.push({ kind: 'gap', from_us: cursor, to_us: resumed });
    }
    parts.push({ kind: 'run', from_us: resumed, to_us: toUs, minutes: [], live: true });
  }
  return parts;
}

export type Pointed =
  | { kind: 'minute'; minute: MinuteSummary }
  | { kind: 'now' }
  | { kind: 'gap'; from_us: number; to_us: number };

/** Finds what lies at `us`, snapping to the nearest minute inside a run. */
export function pointedAt(parts: Stretch[], us: number): Pointed | null {
  const part = parts.find((candidate) => us >= candidate.from_us && us <= candidate.to_us);
  if (!part) {
    return null;
  }
  if (part.kind === 'gap') {
    return part;
  }
  const centre = (minute: MinuteSummary) => minute.minute_us + MINUTE_US / 2;
  let nearest: MinuteSummary | null = null;
  for (const minute of part.minutes) {
    if (nearest === null || Math.abs(centre(minute) - us) < Math.abs(centre(nearest) - us)) {
      nearest = minute;
    }
  }
  if (part.live && (nearest === null || part.to_us - us < Math.abs(centre(nearest) - us))) {
    return { kind: 'now' };
  }
  return nearest ? { kind: 'minute', minute: nearest } : null;
}

/**
 * The values the vertical scale is graduated at, from zero to its top.
 *
 * The top sits above the peak rather than on it, so that a flat series does
 * not run along the edge of the plot; and the scale never drops below a
 * minute, so that a quiet hour does not amplify rounding noise into a curve.
 */
export function waitTicks(peak: number): number[] {
  const reach = Math.max(1, peak) * 1.15;
  const step = [0.5, 1, 2, 5, 10, 20, 50].find((size) => size * 3 >= reach) ?? 100;
  const count = Math.ceil(reach / step);
  return Array.from({ length: count + 1 }, (_, index) => index * step);
}

/**
 * The instants the time axis is labelled at: round ones on the reader's own
 * clock, `stepMinutes` apart.
 */
export function timeTicks(fromUs: number, toUs: number, stepMinutes: number): number[] {
  const start = new Date(fromUs / 1000);
  start.setSeconds(0, 0);
  const sinceMidnight = start.getHours() * 60 + start.getMinutes();
  let tick =
    start.getTime() + ((stepMinutes - (sinceMidnight % stepMinutes)) % stepMinutes) * 60_000;
  if (tick * 1000 < fromUs) {
    tick += stepMinutes * 60_000;
  }
  const ticks: number[] = [];
  for (; tick * 1000 <= toUs; tick += stepMinutes * 60_000) {
    ticks.push(tick * 1000);
  }
  return ticks;
}

/** An SVG path through the plotted points. */
export function linePath(points: { x: number; y: number }[]): string {
  return points
    .map((point, index) => `${index === 0 ? 'M' : 'L'}${point.x.toFixed(1)},${point.y.toFixed(1)}`)
    .join(' ');
}
