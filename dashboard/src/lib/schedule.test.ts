import { describe, expect, it } from 'vitest';

import { WEEKDAYS } from './api/schedule';
import {
  cloneWindow,
  copyDayToAll,
  emptyWeek,
  emptyWindow,
  formatNextChange,
  isNeverOpen,
  isValidTime,
  minutesOf,
  newInterval,
  offeredZones,
  timeZoneNames,
  validateWeek,
} from './schedule';

function week(
  overrides: Partial<Record<(typeof WEEKDAYS)[number], { from: string; to: string }[]>>,
) {
  return { ...emptyWeek(), ...overrides };
}

describe('isValidTime', () => {
  it('accepts a real time of day', () => {
    expect(isValidTime('00:00')).toBe(true);
    expect(isValidTime('23:59')).toBe(true);
    expect(isValidTime('08:30')).toBe(true);
  });

  it('rejects an impossible clock', () => {
    expect(isValidTime('24:00')).toBe(false);
    expect(isValidTime('12:60')).toBe(false);
  });

  it('rejects anything that is not HH:MM', () => {
    // An empty or half-typed field reaches here on every keystroke.
    expect(isValidTime('')).toBe(false);
    expect(isValidTime('8:30')).toBe(false);
    expect(isValidTime('0830')).toBe(false);
  });
});

describe('minutesOf', () => {
  it('counts from midnight', () => {
    expect(minutesOf('00:00')).toBe(0);
    expect(minutesOf('08:30')).toBe(510);
    expect(minutesOf('23:59')).toBe(1439);
  });
});

describe('validateWeek', () => {
  it('passes a plain week', () => {
    expect(validateWeek(week({ monday: [{ from: '08:00', to: '14:00' }] }))).toEqual([]);
  });

  it('passes two services in one day', () => {
    const problems = validateWeek(
      week({
        monday: [
          { from: '08:00', to: '12:00' },
          { from: '14:00', to: '18:00' },
        ],
      }),
    );
    expect(problems).toEqual([]);
  });

  it('accepts intervals that touch', () => {
    // `to` is exclusive, so noon belongs to the afternoon only. This is the
    // rule the appliance applies, and the editor must not be stricter.
    const problems = validateWeek(
      week({
        monday: [
          { from: '08:00', to: '12:00' },
          { from: '12:00', to: '18:00' },
        ],
      }),
    );
    expect(problems).toEqual([]);
  });

  it('catches an interval that ends before it starts', () => {
    const problems = validateWeek(week({ tuesday: [{ from: '18:00', to: '09:00' }] }));
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({ day: 'tuesday', kind: 'order' });
  });

  it('catches overlapping intervals whatever order they were typed in', () => {
    const typed = validateWeek(
      week({
        thursday: [
          { from: '11:00', to: '14:00' },
          { from: '08:00', to: '12:00' },
        ],
      }),
    );
    expect(typed).toHaveLength(1);
    expect(typed[0]).toMatchObject({ day: 'thursday', kind: 'overlap' });
  });

  it('reports every day at fault, not just the first', () => {
    // The appliance answers with one error because an API returns one; the
    // screen edits seven days at once and must show them all.
    const problems = validateWeek(
      week({
        monday: [{ from: '18:00', to: '09:00' }],
        friday: [
          { from: '08:00', to: '12:00' },
          { from: '11:00', to: '14:00' },
        ],
      }),
    );
    expect(problems.map((problem) => problem.day)).toEqual(['monday', 'friday']);
  });

  it('reports a malformed time and stops judging that day', () => {
    // Comparing a half-typed field against a real time produces nonsense;
    // one complaint about the field itself is the useful answer.
    const problems = validateWeek(week({ monday: [{ from: '8:0', to: '14:00' }] }));
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({ day: 'monday', kind: 'time' });
  });

  it('survives a day the appliance never sent', () => {
    const partial = { ...emptyWeek(), monday: [{ from: '08:00', to: '14:00' }] };
    delete (partial as Record<string, unknown>).sunday;
    expect(() => validateWeek(partial)).not.toThrow();
  });
});

describe('isNeverOpen', () => {
  it('recognises a week with no hours at all', () => {
    expect(isNeverOpen(emptyWeek())).toBe(true);
  });

  it('is false as soon as one day serves', () => {
    expect(isNeverOpen(week({ sunday: [{ from: '10:00', to: '12:00' }] }))).toBe(false);
  });
});

describe('copyDayToAll', () => {
  it('puts one day’s hours on every day', () => {
    const copied = copyDayToAll(week({ monday: [{ from: '08:00', to: '14:00' }] }), 'monday');
    for (const day of WEEKDAYS) {
      expect(copied[day]).toEqual([{ from: '08:00', to: '14:00' }]);
    }
  });

  it('gives each day its own intervals', () => {
    // Sharing one array would make editing Tuesday change Monday too — the
    // classic aliasing bug behind a "copy" button.
    const copied = copyDayToAll(week({ monday: [{ from: '08:00', to: '14:00' }] }), 'monday');
    copied.tuesday[0].from = '10:00';
    expect(copied.monday[0].from).toBe('08:00');
  });

  it('can copy an empty day, closing the week', () => {
    const copied = copyDayToAll(week({ monday: [{ from: '08:00', to: '14:00' }] }), 'sunday');
    expect(isNeverOpen(copied)).toBe(true);
  });
});

describe('newInterval', () => {
  it('proposes ordinary hours on an empty day', () => {
    expect(newInterval([])).toEqual({ from: '09:00', to: '17:00' });
  });

  it('lands after what is already there, never on top of it', () => {
    // A second interval that overlaps the first would show an error before
    // the operator has typed anything.
    const added = newInterval([{ from: '08:00', to: '12:00' }]);
    expect(minutesOf(added.from)).toBeGreaterThanOrEqual(minutesOf('12:00'));
    expect(validateWeek(week({ monday: [{ from: '08:00', to: '12:00' }, added] }))).toEqual([]);
  });

  it('stays inside the day when hours already run late', () => {
    const added = newInterval([{ from: '08:00', to: '23:30' }]);
    expect(isValidTime(added.from)).toBe(true);
    expect(isValidTime(added.to)).toBe(true);
    expect(minutesOf(added.to)).toBeGreaterThan(minutesOf(added.from));
  });
});

describe('cloneWindow', () => {
  it('leaves the original untouched when the copy is edited', () => {
    // The editor works on a draft so "cancel" can mean something.
    const original = {
      ...emptyWindow('Europe/Paris'),
      weekly: week({ monday: [{ from: '08:00', to: '14:00' }] }),
    };
    const draft = cloneWindow(original);
    draft.weekly.monday[0].to = '18:00';
    draft.timezone = 'UTC';
    expect(original.weekly.monday[0].to).toBe('14:00');
    expect(original.timezone).toBe('Europe/Paris');
  });

  it('fills in days and closures the appliance omitted', () => {
    const sparse = { timezone: 'Europe/Paris' } as unknown as Parameters<typeof cloneWindow>[0];
    const cloned = cloneWindow(sparse);
    expect(cloned.closures).toEqual([]);
    for (const day of WEEKDAYS) {
      expect(cloned.weekly[day]).toEqual([]);
    }
  });
});

describe('offeredZones', () => {
  it('offers what the platform knows', () => {
    expect(offeredZones(['Europe/Paris', 'Europe/London'], 'Europe/Paris')).toEqual([
      'Europe/Paris',
      'Europe/London',
    ]);
  });

  it('adds the detected zone when the platform does not list it', () => {
    // `Intl.supportedValuesOf` answers with canonical identifiers only, and
    // `UTC` is not one — so a machine set to UTC detects a zone its own
    // browser will not offer. Pre-filling the field with a value that has no
    // matching option renders the select empty.
    const canonical = ['Africa/Abidjan', 'Europe/Paris'];
    expect(offeredZones(canonical, 'UTC')).toEqual(['UTC', ...canonical]);
  });

  it('falls back to a usable handful when the platform knows nothing', () => {
    const zones = offeredZones([], 'Europe/Paris');
    expect(zones.length).toBeGreaterThan(0);
    expect(zones).toContain('Europe/Paris');
  });

  it('still offers the detected zone when falling back', () => {
    expect(offeredZones([], 'Pacific/Auckland')).toContain('Pacific/Auckland');
  });

  it('never offers a zone twice', () => {
    const zones = offeredZones(['Europe/Paris', 'Europe/London'], 'Europe/Paris');
    expect(new Set(zones).size).toBe(zones.length);
  });
});

describe('timeZoneNames', () => {
  it('offers the zone this browser reports, wherever it is running', () => {
    // The assertion that failed on a runner set to UTC while passing on a
    // laptop in Europe/Paris. It stays here, on the real environment, as the
    // check that the pure rule above is actually wired to it.
    const zones = timeZoneNames();
    expect(zones.length).toBeGreaterThan(0);
    expect(zones).toContain(Intl.DateTimeFormat().resolvedOptions().timeZone);
  });
});

describe('formatNextChange', () => {
  /* Built in local time on purpose. The function decides between "today" and
     another day by the reader's own calendar, so fixtures pinned to UTC only
     mean what they look like where the machine's offset is small: 10:00 and
     14:00 UTC are one day in Paris and two in Auckland, which made this suite
     depend on where it ran. Local constructors make "the same day" true
     wherever the tests are executed. */
  const local = (day: number, hour: number) => new Date(2026, 6, day, hour, 0).getTime() * 1000;
  const NOW = local(30, 10);

  it('names only the time when the change is later today', () => {
    const rendered = formatNextChange(local(30, 14), NOW, 'fr-FR', false);
    expect(rendered).toMatch(/^\d{2}:\d{2}$/);
  });

  it('names the weekday when it is another day', () => {
    const rendered = formatNextChange(local(31, 8), NOW, 'fr-FR', false);
    expect(rendered).toMatch(/^\p{L}+ \d{2}:\d{2}$/u);
  });

  it('follows the interface locale rather than the environment', () => {
    // The same bug the live view had: a French screen printing "2:00 PM"
    // because the laptop happened to be American.
    const later = local(30, 14);
    expect(formatNextChange(later, NOW, 'fr-FR', false)).not.toMatch(/AM|PM/i);
    expect(formatNextChange(later, NOW, 'en-GB', true)).toMatch(/AM|PM/i);
  });
});
