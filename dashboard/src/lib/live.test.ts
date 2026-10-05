import { describe, expect, it } from 'vitest';

import { DENSITY_FILL, DENSITY_SWATCH, activity, densityName, formatClock } from './live';
import { DENSITY_CLASSES, type Estimate, type LiveSnapshot } from './api/live';
import { SILENT_AFTER_US } from './sensors';

describe('activity', () => {
  const NOW = 1_785_600_000_000_000;
  const estimated = (ts_us: number): Estimate => ({
    ts_us,
    wait_minutes: 3.2,
    people: 4,
    level: 1.4,
    class: 'low',
    confidence: 0.8,
    reliable: true,
  });
  const reported = (
    changes: Partial<LiveSnapshot> = {},
    stream: Partial<LiveSnapshot['stream']> = {},
  ): LiveSnapshot => ({
    estimate: estimated(NOW),
    service: { open: true },
    recording: false,
    now_us: NOW,
    ...changes,
    stream: {
      running: true,
      estimating: true,
      frames: 1000,
      estimates: 10,
      last_frame_us: NOW,
      edge_stamped: true,
      nodes: {},
      ...stream,
    },
  });

  it('is estimating while the last estimate is recent', () => {
    expect(activity(reported()).kind).toBe('estimating');
    expect(activity(reported({ estimate: estimated(NOW - SILENT_AFTER_US) })).kind).toBe(
      'estimating',
    );
  });

  it('is interrupted once the estimate it still holds has aged', () => {
    // The appliance keeps its last estimate for as long as nothing replaces it.
    const held = estimated(NOW - SILENT_AFTER_US - 1);

    expect(activity(reported({ estimate: held }))).toEqual({ kind: 'interrupted', last: held });
  });

  it('is starting while frames arrive and no window has filled', () => {
    expect(activity(reported({ estimate: undefined })).kind).toBe('starting');
  });

  it('is interrupted, not starting, when no frame arrives either', () => {
    const silent = reported({ estimate: undefined }, { last_frame_us: NOW - SILENT_AFTER_US - 1 });

    expect(activity(silent)).toEqual({ kind: 'interrupted', last: null });
    expect(activity(reported({ estimate: undefined }, { last_frame_us: undefined })).kind).toBe(
      'interrupted',
    );
  });

  it('is idle when no estimator is attached, whatever it once estimated', () => {
    expect(activity(reported({}, { estimating: false })).kind).toBe('idle');
  });

  it('puts a recording before everything else, and a closed site before estimating', () => {
    expect(activity(reported({ recording: true, service: { open: false } })).kind).toBe(
      'recording',
    );
    expect(activity(reported({ service: { open: false } })).kind).toBe('closed');
  });

  it('ages a replayed capture on its own clock, not the appliance clock', () => {
    // Its timestamps are those of the recording: against the clock, every
    // estimate of a replay would be years old.
    const then = NOW - 400 * 24 * 3_600_000_000;
    const replay = reported(
      { estimate: estimated(then) },
      { edge_stamped: false, last_frame_us: then + 1_000_000 },
    );

    expect(activity(replay).kind).toBe('estimating');
  });
});

describe('densityName', () => {
  it('maps the stored integers to their names', () => {
    expect(densityName(0)).toBe('empty');
    expect(densityName(1)).toBe('low');
    expect(densityName(2)).toBe('medium');
    expect(densityName(3)).toBe('saturated');
  });

  it('falls back rather than producing an unknown translation key', () => {
    // A value outside 0..=3 can only come from a corrupted row; showing
    // "empty" is wrong but harmless, showing `class.undefined` is a broken
    // screen.
    expect(densityName(9)).toBe('empty');
    expect(densityName(-1)).toBe('empty');
  });
});

describe('density colour maps', () => {
  // The type makes every class present; only a copy-paste can still point a
  // class at the wrong colour, which is what these check.
  it.each(DENSITY_CLASSES)('%s maps to its own fill and swatch', (name) => {
    expect(DENSITY_FILL[name]).toBe(`fill-density-${name}`);
    expect(DENSITY_SWATCH[name]).toBe(`bg-density-${name}`);
  });

  it('names no colour twice', () => {
    expect(new Set(Object.values(DENSITY_FILL)).size).toBe(DENSITY_CLASSES.length);
    expect(new Set(Object.values(DENSITY_SWATCH)).size).toBe(DENSITY_CLASSES.length);
  });
});

describe('formatClock', () => {
  // 2026-07-30T23:41:00Z, rendered in UTC by pinning the test's timezone
  // through the locale's own behaviour is not portable, so only the shape
  // and the 12/24 distinction are asserted.
  const NOON_UTC = Date.UTC(2026, 6, 30, 12, 5) * 1000;

  it('renders a 24-hour clock without a meridiem', () => {
    const rendered = formatClock(NOON_UTC, 'fr-FR', false);
    expect(rendered).toMatch(/^\d{2}:\d{2}$/);
  });

  it('renders a 12-hour clock with one', () => {
    const rendered = formatClock(NOON_UTC, 'en-GB', true);
    expect(rendered).toMatch(/AM|PM/i);
  });

  it('follows the locale it is given, not the environment', () => {
    // The bug this guards: a French interface printing "11:41 PM" because
    // the browser happened to be American.
    expect(formatClock(NOON_UTC, 'fr-FR', false)).not.toMatch(/AM|PM/i);
  });
});
