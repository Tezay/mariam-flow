import { describe, expect, it } from 'vitest';

import {
  adjacentErrorShare,
  comparison,
  delta,
  formatPoints,
  formatShare,
  presenceAccuracy,
  readings,
  recall,
  recordingOverlap,
  recordingRows,
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

function trainedOn(ids: string[]): Pick<Evaluation, 'sessions'> {
  return { sessions: ids.map((session_id) => ({ session_id, windows: 100, support: [] })) };
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

describe('delta', () => {
  it('points the arrow at the difference the reader is shown', () => {
    expect(delta(0.74, 0.68)).toEqual({ value: expect.closeTo(0.06, 10), direction: 'up' });
    expect(delta(0.61, 0.65).direction).toBe('down');
  });

  it('calls a difference too small to print level, so no arrow contradicts a +0', () => {
    const change = delta(0.6812, 0.68);

    expect(formatPoints(change.value)).toBe('+0');
    expect(change.direction).toBe('level');
  });
});

describe('recordingRows', () => {
  const before = evaluation(PERFECT, trainedOn(['charlie', 'alpha', 'bravo']));
  const after = evaluation(PERFECT, trainedOn(['bravo', 'charlie', 'delta']));

  it('merges both corpora into one ordered list, holes and all', () => {
    const rows = recordingRows(before, after);

    // Ordered on the identifier rather than on either run's order, so the two
    // columns are read across; a recording one of them never saw is a hole.
    expect(rows.map((row) => row.session_id)).toEqual(['alpha', 'bravo', 'charlie', 'delta']);
    expect(rows[0]).toEqual({ session_id: 'alpha', reference: 100, candidate: null });
    expect(rows[3]).toEqual({ session_id: 'delta', reference: null, candidate: 100 });
  });

  it('counts what both saw apart from what only one of them did', () => {
    expect(recordingOverlap(recordingRows(before, after))).toEqual({
      shared: 2,
      onlyReference: 1,
      onlyCandidate: 1,
    });
  });
});

describe('recall', () => {
  it('reads one level down its own row, so a rare level is judged on its own', () => {
    const confusion = [
      [90, 10, 0, 0],
      [0, 100, 0, 0],
      [0, 0, 100, 0],
      [0, 0, 2, 2],
    ];

    // `saturated` was seen four times and found twice: half, not the 1 % of
    // the whole matrix those two windows represent.
    expect(recall(confusion, 3)).toBe(0.5);
    expect(recall(confusion, 0)).toBe(0.9);
  });
});

describe('comparison', () => {
  const CORPUS = trainedOn(['morning', 'noon', 'evening']);

  it('reads every difference as the candidate against the reference', () => {
    const before = evaluation(
      [
        [20, 10, 0, 0],
        [10, 20, 0, 0],
        [0, 0, 20, 10],
        [0, 0, 10, 20],
      ],
      { accuracy: 0.5, baseline_accuracy: 0.25, ...CORPUS },
    );
    const after = evaluation(PERFECT, { accuracy: 0.9, baseline_accuracy: 0.25, ...CORPUS });

    const result = comparison(before, after);

    expect(result.accuracy.direction).toBe('up');
    expect(formatPoints(result.accuracy.value)).toBe('+40');
    // The candidate never confuses an empty zone for a busy one, where the
    // reference did so on half of two rows.
    expect(result.presence.direction).toBe('up');
  });

  it('marks a candidate that gained accuracy on an easier corpus as no better', () => {
    // Both are 40 points clear of their own baseline; only the baselines
    // differ, which accuracy alone would report as a gain.
    const before = evaluation(PERFECT, { accuracy: 0.65, baseline_accuracy: 0.25, ...CORPUS });
    const after = evaluation(PERFECT, { accuracy: 0.85, baseline_accuracy: 0.45, ...CORPUS });

    const result = comparison(before, after);

    expect(result.accuracy.direction).toBe('up');
    expect(result.lift.direction).toBe('level');
  });

  it('refuses to call two runs like for like when they were scored differently', () => {
    const shared = comparison(evaluation(PERFECT, CORPUS), evaluation(PERFECT, CORPUS));
    const widened = comparison(
      evaluation(PERFECT, CORPUS),
      evaluation(PERFECT, trainedOn(['morning', 'noon', 'evening', 'late'])),
    );

    expect(shared.likeForLike).toBe(true);
    expect(widened.likeForLike).toBe(false);
  });

  it('reports a level the candidate lost, even when it gained overall', () => {
    // Saturated fell from every window found to half of them, while the three
    // commoner levels held: the headline accuracy barely moves.
    const before = evaluation(PERFECT, { accuracy: 0.9, ...CORPUS });
    const after = evaluation(
      [
        [10, 0, 0, 0],
        [0, 10, 0, 0],
        [0, 0, 10, 0],
        [0, 0, 5, 5],
      ],
      { accuracy: 0.92, ...CORPUS },
    );

    const result = comparison(before, after);

    expect(result.accuracy.direction).toBe('up');
    expect(result.perClass[3].direction).toBe('down');
    expect(formatPoints(result.perClass[3].value)).toBe('−50');
  });

  it('treats two runs that list no recording at all as unproven, not identical', () => {
    // An empty intersection is not evidence of a shared corpus, and reading it
    // as one would silence the caveat exactly when it is least deserved.
    expect(comparison(evaluation(PERFECT), evaluation(PERFECT)).likeForLike).toBe(false);
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
