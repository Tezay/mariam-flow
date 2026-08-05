import { describe, expect, it } from 'vitest';

import { type RecordedEvent } from './api/journal';
import { byDay, hasMore, newest, oldest, PAGE, prepend, toneOf } from './journal';

function event(id: number, ts_us: number): RecordedEvent {
  return { id, ts_us, category: 'lifecycle', kind: 'service-opened' };
}

/** 2 August 2026, 12:00 and 23:00 UTC, and the day before. */
const NOON = Date.UTC(2026, 7, 2, 12) * 1000;
const DAY = 86_400_000_000;

describe('byDay', () => {
  it('groups consecutive rows of the same day and keeps their order', () => {
    const days = byDay([event(9, NOON + 3600e6), event(8, NOON), event(7, NOON - DAY)], 'en-GB');

    expect(days).toHaveLength(2);
    expect(days[0].events.map((e) => e.id)).toEqual([9, 8]);
    expect(days[1].events.map((e) => e.id)).toEqual([7]);
  });

  it('has nothing to group when there is nothing', () => {
    expect(byDay([], 'en-GB')).toEqual([]);
  });
});

describe('prepend', () => {
  it('puts what arrived above what was being read', () => {
    const shown = [event(5, NOON), event(4, NOON)];
    expect(prepend([event(7, NOON), event(6, NOON)], shown).map((e) => e.id)).toEqual([7, 6, 5, 4]);
  });

  it('never shows a row twice', () => {
    // The poll and the first page can overlap; a duplicated row would read as
    // the appliance having done the same thing twice.
    const shown = [event(5, NOON), event(4, NOON)];
    expect(prepend([event(6, NOON), event(5, NOON)], shown).map((e) => e.id)).toEqual([6, 5, 4]);
  });
});

describe('paging', () => {
  it('asks for more only when the last page was full', () => {
    expect(hasMore(Array.from({ length: PAGE }, (_, n) => event(n, NOON)))).toBe(true);
    expect(hasMore([event(1, NOON)])).toBe(false);
    expect(hasMore([])).toBe(false);
  });

  it('reads back from the oldest row and forward from the newest', () => {
    const shown = [event(9, NOON), event(8, NOON), event(7, NOON)];
    expect(oldest(shown)).toBe(7);
    expect(newest(shown)).toBe(9);
    expect(oldest([])).toBeUndefined();
    expect(newest([])).toBeUndefined();
  });
});

describe('toneOf', () => {
  it('spends red on someone trying secrets', () => {
    expect(toneOf('login-failed')).toBe('fault');
    expect(toneOf('login-throttled')).toBe('fault');
  });

  it('spends amber on the appliance running degraded', () => {
    expect(toneOf('model-rejected')).toBe('attention');
    expect(toneOf('node-lost')).toBe('attention');
    expect(toneOf('credential-reset')).toBe('attention');
    expect(toneOf('unknown')).toBe('attention');
  });

  it('leaves everything routine uncoloured', () => {
    // A journal where every family has a hue is a wall of colour in which an
    // incident is exactly as visible as a routine save.
    for (const kind of [
      'login-succeeded',
      'logged-out',
      'started',
      'stopped',
      'configuration-changed',
      'service-opened',
      'service-closed',
      'stage-completed',
      'calibration-started',
      'calibration-stopped',
      'model-activated',
      'node-appeared',
    ] as const) {
      expect(toneOf(kind)).toBe('plain');
    }
  });
});
