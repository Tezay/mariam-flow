import { describe, expect, it } from 'vitest';

import { type MinuteSummary } from './api/live';
import {
  MINUTE_US,
  RANGES,
  isRange,
  linePath,
  pointedAt,
  rangeLabel,
  stretches,
  timeTicks,
  waitTicks,
} from './history';

/** Noon, on a minute boundary. */
const NOON = Date.UTC(2026, 9, 6, 12) * 1000;

/** The minute starting `offset` minutes after noon. */
function minute(offset: number, wait = 3): MinuteSummary {
  return {
    minute_us: NOON + offset * MINUTE_US,
    samples: 60,
    reliable_samples: 60,
    wait_minutes: wait,
    level: 1,
    class: 1,
    confidence: 0.9,
  };
}

const at = (offset: number) => NOON + offset * MINUTE_US;
const shape = (parts: ReturnType<typeof stretches>) =>
  parts.map((part) => [
    part.kind,
    (part.from_us - NOON) / MINUTE_US,
    (part.to_us - NOON) / MINUTE_US,
  ]);

describe('stretches', () => {
  it('shows the minutes nothing was estimated in as a gap of their own', () => {
    const stored = [minute(0), minute(1), minute(5), minute(6)];

    expect(shape(stretches(stored, at(0), at(7), false))).toEqual([
      ['run', 0, 2],
      ['gap', 2, 5],
      ['run', 5, 7],
    ]);
  });

  it('opens and closes on a gap when the window reaches past what is stored', () => {
    expect(shape(stretches([minute(3)], at(0), at(10), false))).toEqual([
      ['gap', 0, 3],
      ['run', 3, 4],
      ['gap', 4, 10],
    ]);
    expect(shape(stretches([], at(0), at(10), false))).toEqual([['gap', 0, 10]]);
  });

  it('carries the last run to the present while the appliance is estimating', () => {
    // The minute in progress is stored only once it ends.
    const parts = stretches([minute(0), minute(1)], at(0), at(2.5), true);

    expect(shape(parts)).toEqual([['run', 0, 2.5]]);
    expect(parts[0]).toMatchObject({ live: true });
  });

  it('starts a run at the present when estimating has only just resumed', () => {
    const parts = stretches([minute(0)], at(0), at(6.5), true);

    expect(shape(parts)).toEqual([
      ['run', 0, 1],
      ['gap', 1, 6],
      ['run', 6, 6.5],
    ]);
    expect(parts[2]).toMatchObject({ minutes: [], live: true });
  });

  it('leaves out what lies outside the window, and clips what straddles its start', () => {
    const stored = [minute(-5), minute(0), minute(1), minute(20)];

    expect(shape(stretches(stored, at(0.5), at(2), false))).toEqual([['run', 0.5, 2]]);
  });
});

describe('pointedAt', () => {
  const parts = stretches([minute(0, 2), minute(1, 4), minute(5, 6)], at(0), at(6.5), true);

  it('snaps to the nearest minute of a run', () => {
    expect(pointedAt(parts, at(0.9))).toEqual({ kind: 'minute', minute: minute(0, 2) });
    expect(pointedAt(parts, at(1.1))).toEqual({ kind: 'minute', minute: minute(1, 4) });
  });

  it('reports a gap as a whole, not a moment inside it', () => {
    expect(pointedAt(parts, at(3))).toEqual({ kind: 'gap', from_us: at(2), to_us: at(5) });
  });

  it('points at the present once it is nearer than the last stored minute', () => {
    expect(pointedAt(parts, at(6.4))).toEqual({ kind: 'now' });
    expect(pointedAt(parts, at(5.6))).toEqual({ kind: 'minute', minute: minute(5, 6) });
  });

  it('points at nothing outside the window', () => {
    expect(pointedAt(parts, at(-1))).toBeNull();
  });
});

describe('waitTicks', () => {
  it('tops out above the peak, so a flat series does not run along the edge', () => {
    expect(waitTicks(4)).toEqual([0, 2, 4, 6]);
    expect(waitTicks(16.8)).toEqual([0, 10, 20]);
  });

  it('never graduates below a minute, however quiet the hour', () => {
    expect(waitTicks(0)).toEqual([0, 0.5, 1, 1.5]);
  });

  it('keeps to a handful of round values over two decades', () => {
    for (const peak of [0.3, 1, 2.5, 7, 12, 33, 90]) {
      const ticks = waitTicks(peak);

      expect(ticks.length).toBeGreaterThanOrEqual(3);
      expect(ticks.length).toBeLessThanOrEqual(5);
      expect(ticks[ticks.length - 1]).toBeGreaterThan(peak);
    }
  });
});

describe('timeTicks', () => {
  it('falls on round times of the reader’s clock, a step apart', () => {
    const from = NOON + 3 * MINUTE_US + 20_000_000;
    const ticks = timeTicks(from, from + 30 * MINUTE_US, 5);

    expect(ticks).toHaveLength(6);
    expect(ticks[0]).toBeGreaterThanOrEqual(from);
    for (const tick of ticks) {
      const local = new Date(tick / 1000);
      expect(local.getMinutes() % 5).toBe(0);
      expect(local.getSeconds()).toBe(0);
    }
    expect(ticks[1] - ticks[0]).toBe(5 * MINUTE_US);
  });

  it('includes a window that starts on a round time', () => {
    expect(timeTicks(NOON, NOON + 10 * MINUTE_US, 5)).toEqual([NOON, at(5), at(10)]);
  });
});

describe('ranges', () => {
  it('opens on the shortest, and names each by its usual unit', () => {
    expect(RANGES[0]).toBe(30);
    expect(RANGES.map(rangeLabel)).toEqual(['30min', '1h', '3h', '6h']);
  });

  it('accepts only a range on offer, whatever was stored', () => {
    expect(isRange(180)).toBe(true);
    expect(isRange(45)).toBe(false);
    expect(isRange(Number.NaN)).toBe(false);
  });
});

describe('linePath', () => {
  it('starts with a move and continues with lines', () => {
    expect(
      linePath([
        { x: 0, y: 10 },
        { x: 5, y: 2.25 },
      ]),
    ).toBe('M0.0,10.0 L5.0,2.3');
  });

  it('produces nothing from no points', () => {
    expect(linePath([])).toBe('');
  });
});
