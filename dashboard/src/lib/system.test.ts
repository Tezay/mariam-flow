import { describe, expect, it } from 'vitest';

import { asMegabytes, formatUptime } from './system';

describe('formatUptime', () => {
  it('counts seconds only in the first minute', () => {
    expect(formatUptime(0)).toBe('0 s');
    expect(formatUptime(59)).toBe('59 s');
  });

  it('steps up a unit at each threshold', () => {
    expect(formatUptime(60)).toBe('1 min');
    expect(formatUptime(3600)).toBe('1 h');
    expect(formatUptime(86_400)).toBe('1 d');
  });

  it('rounds down rather than flattering the figure', () => {
    expect(formatUptime(86_399)).toBe('23 h');
  });

  it('keeps the hours a day count would hide', () => {
    // An appliance up for 47 hours is nearly at two days, which "1 d" alone
    // would not say.
    expect(formatUptime(47 * 3600)).toBe('1 d 23 h');
  });

  it('drops the hours when there are none', () => {
    expect(formatUptime(120 * 86_400)).toBe('120 d');
  });
});

describe('asMegabytes', () => {
  it('converts from the kibibytes the kernel reports', () => {
    expect(asMegabytes(444_444)).toBe(434);
    expect(asMegabytes(0)).toBe(0);
  });
});
