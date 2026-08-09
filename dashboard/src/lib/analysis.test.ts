import { describe, expect, it } from 'vitest';

import {
  adjacentErrorShare,
  formatPoints,
  formatShare,
  presenceAccuracy,
  readings,
  rowShare,
  totalWindows,
} from './analysis';
import { type Evaluation } from './api/models';

/** Rows are truth, columns are prediction. */
function evaluation(confusion: number[][], overrides: Partial<Evaluation> = {}): Evaluation {
  return {
    accuracy: 0.5,
    baseline_accuracy: 0.25,
    confusion,
    windows: totalWindows(confusion),
    splits: 3,
    receivers: ['rx-1'],
    sessions: [],
    ...overrides,
  };
}

const PERFECT = [
  [10, 0, 0, 0],
  [0, 10, 0, 0],
  [0, 0, 10, 0],
  [0, 0, 0, 10],
];

describe('presenceAccuracy', () => {
  it('merges the three occupied classes into one question', () => {
    // Confused between low and medium throughout, yet never wrong about
    // whether anyone was there.
    const confusion = [
      [10, 0, 0, 0],
      [0, 5, 5, 0],
      [0, 5, 5, 0],
      [0, 0, 5, 5],
    ];

    expect(presenceAccuracy(confusion)).toBe(1);
  });

  it('counts an occupied zone called empty as a miss', () => {
    const confusion = [
      [8, 2, 0, 0],
      [4, 6, 0, 0],
      [0, 0, 10, 0],
      [0, 0, 0, 10],
    ];

    // 8 + 6 + 10 + 10 agreed out of 40.
    expect(presenceAccuracy(confusion)).toBe(0.85);
  });

  it('answers zero rather than dividing by nothing', () => {
    expect(presenceAccuracy([])).toBe(0);
  });
});

describe('adjacentErrorShare', () => {
  it('separates hesitating on a boundary from understanding nothing', () => {
    const confusion = [
      [8, 2, 0, 0],
      [0, 8, 1, 1],
      [0, 0, 10, 0],
      [0, 0, 0, 10],
    ];

    // Three errors after the diagonal, of which the saturated-for-low one is
    // two classes away.
    expect(adjacentErrorShare(confusion)).toBeCloseTo(3 / 4, 10);
  });

  it('treats a model that never errs as fully ordered', () => {
    // No mistake can lie outside the neighbourhood when there is none.
    expect(adjacentErrorShare(PERFECT)).toBe(1);
  });
});

describe('rowShare', () => {
  it('normalises within the truth row, not the whole matrix', () => {
    // `saturated` seen four times against `empty` a hundred.
    const confusion = [
      [100, 0, 0, 0],
      [0, 0, 0, 0],
      [0, 0, 0, 0],
      [0, 0, 1, 3],
    ];

    expect(rowShare(confusion, 3, 3)).toBe(0.75);
    expect(rowShare(confusion, 0, 0)).toBe(1);
  });

  it('answers zero for a class no window ever held', () => {
    expect(rowShare(PERFECT, 1, 1)).toBe(1);
    expect(rowShare([[0, 0, 0, 0]], 0, 0)).toBe(0);
    expect(rowShare([], 2, 2)).toBe(0);
  });
});

describe('readings', () => {
  it('reports a model that is right for the right reasons as good throughout', () => {
    const result = readings(evaluation(PERFECT, { accuracy: 1, baseline_accuracy: 0.25 }));

    expect(result.presence.verdict).toBe('good');
    expect(result.ordering.verdict).toBe('good');
    expect(result.exact.verdict).toBe('good');
    expect(result.lift).toBe(0.75);
  });

  it('tells a model that hesitates on levels from one that understands nothing', () => {
    const hesitant = readings(
      evaluation(
        [
          [25, 5, 0, 0],
          [5, 15, 10, 0],
          [0, 10, 15, 5],
          [0, 0, 5, 25],
        ],
        { accuracy: 0.5, baseline_accuracy: 0.25 },
      ),
    );

    expect(hesitant.presence.verdict).toBe('good');
    expect(hesitant.ordering.verdict).toBe('good');
    expect(hesitant.exact.verdict).toBe('fair');
  });

  it('marks a model that cannot see presence as poor', () => {
    const blind = readings(
      evaluation(
        [
          [5, 5, 5, 5],
          [5, 5, 5, 5],
          [5, 5, 5, 5],
          [5, 5, 5, 5],
        ],
        { accuracy: 0.25, baseline_accuracy: 0.25 },
      ),
    );

    expect(blind.presence.verdict).toBe('poor');
    expect(blind.exact.verdict).toBe('poor');
    expect(blind.lift).toBe(0);
  });
});

describe('formatting', () => {
  it('gives shares the only precision they earn', () => {
    expect(formatShare(0.6738)).toBe('67 %');
    expect(formatShare(1)).toBe('100 %');
  });

  it('signs a difference in points, so a model that learned nothing shows it', () => {
    expect(formatPoints(0.362)).toBe('+36');
    expect(formatPoints(-0.04)).toBe('−4');
    expect(formatPoints(0)).toBe('+0');
  });
});
