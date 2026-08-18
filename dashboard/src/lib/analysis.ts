/**
 * Reading a model's evaluation (ADR 0024).
 *
 * Every function here takes the confusion matrix in the orientation the
 * training run computes it: **rows are truth, columns are prediction**. Read
 * the other way round, every result below is inverted.
 */

import { DENSITY_CLASSES } from './api/live';
import { type Evaluation } from './api/models';

export type Verdict = 'good' | 'fair' | 'poor';

export type Reading = {
  value: number;
  verdict: Verdict;
};

/** Readings a model is judged on, named as their message keys are. */
export type ReadingKey = 'presence' | 'ordering' | 'exact';

export type Readings = {
  presence: Reading;
  ordering: Reading;
  exact: Reading;
  /** Points of accuracy above always answering the commonest class. */
  lift: number;
};

/*
 * Bands rather than a pass mark, set against what the four-class problem
 * allows: chance sits at 25 %, and the levels either side of a boundary
 * genuinely overlap as a zone fills.
 */
const PRESENCE = { good: 0.9, fair: 0.75 };
const ORDERING = { good: 0.8, fair: 0.6 };
const EXACT = { good: 0.6, fair: 0.45 };

export function totalWindows(confusion: number[][]): number {
  return confusion.reduce((running, row) => running + row.reduce((sum, cell) => sum + cell, 0), 0);
}

/** Share of windows on which the model agreed that anyone was there. */
export function presenceAccuracy(confusion: number[][]): number {
  const total = totalWindows(confusion);
  if (total === 0) {
    return 0;
  }
  let agreed = 0;
  for (let truth = 0; truth < confusion.length; truth += 1) {
    for (let predicted = 0; predicted < confusion[truth].length; predicted += 1) {
      if ((truth === 0) === (predicted === 0)) {
        agreed += confusion[truth][predicted];
      }
    }
  }
  return agreed / total;
}

/**
 * Share of the mistakes that land on a neighbouring class.
 *
 * With no mistakes at all there is none to fall outside the neighbourhood, so
 * the share is complete rather than undefined.
 */
export function adjacentErrorShare(confusion: number[][]): number {
  let adjacent = 0;
  let errors = 0;
  for (let truth = 0; truth < confusion.length; truth += 1) {
    for (let predicted = 0; predicted < confusion[truth].length; predicted += 1) {
      if (truth === predicted) {
        continue;
      }
      errors += confusion[truth][predicted];
      if (Math.abs(truth - predicted) === 1) {
        adjacent += confusion[truth][predicted];
      }
    }
  }
  return errors === 0 ? 1 : adjacent / errors;
}

/**
 * Share of one truth row a cell holds.
 *
 * Per row rather than per matrix: a class a capture spent little time in would
 * otherwise render pale throughout, reading as one the model never gets right
 * rather than one it was rarely shown.
 */
export function rowShare(confusion: number[][], truth: number, predicted: number): number {
  const row = confusion[truth];
  if (!row) {
    return 0;
  }
  const total = row.reduce((sum, cell) => sum + cell, 0);
  return total === 0 ? 0 : row[predicted] / total;
}

/** Share of one level's windows the model found. */
export function recall(confusion: number[][], level: number): number {
  return rowShare(confusion, level, level);
}

/** Windows a level actually held. */
export function levelWindows(confusion: number[][], level: number): number {
  return (confusion[level] ?? []).reduce((sum, cell) => sum + cell, 0);
}

/** The three readings, each against its own band. */
export function readings(evaluation: Evaluation): Readings {
  return {
    presence: reading(presenceAccuracy(evaluation.confusion), PRESENCE),
    ordering: reading(adjacentErrorShare(evaluation.confusion), ORDERING),
    exact: reading(evaluation.accuracy, EXACT),
    lift: evaluation.accuracy - evaluation.baseline_accuracy,
  };
}

function reading(value: number, band: { good: number; fair: number }): Reading {
  if (value >= band.good) {
    return { value, verdict: 'good' };
  }
  return { value, verdict: value >= band.fair ? 'fair' : 'poor' };
}

/** A share as whole percent, which is the only precision these numbers earn. */
export function formatShare(value: number): string {
  return `${Math.round(value * 100)} %`;
}

/** A difference in percentage points, always signed. */
export function formatPoints(value: number): string {
  const points = Math.round(value * 100);
  return `${points >= 0 ? '+' : '−'}${Math.abs(points)}`;
}

/* Every difference below is the candidate minus the reference. */

export type Direction = 'up' | 'down' | 'level';

export type Delta = {
  value: number;
  direction: Direction;
};

/** Rounded before its direction is read, so no arrow contradicts a printed `+0`. */
export function delta(candidate: number, reference: number): Delta {
  const value = candidate - reference;
  const points = Math.round(value * 100);
  return { value, direction: points === 0 ? 'level' : points > 0 ? 'up' : 'down' };
}

/** One recording, and what it weighed in each run that used it. */
export type RecordingRow = {
  session_id: string;
  reference: number | null;
  candidate: number | null;
};

/**
 * The two runs' recordings merged into one list, ordered by identifier.
 *
 * A recording only one of them held is kept, `null` against the other: that
 * gap is what makes a difference between their scores worth doubting.
 */
export function recordingRows(reference: Evaluation, candidate: Evaluation): RecordingRow[] {
  const held = new Map(reference.sessions.map((session) => [session.session_id, session.windows]));
  const other = new Map(candidate.sessions.map((session) => [session.session_id, session.windows]));
  return [...new Set([...held.keys(), ...other.keys()])].sort().map((session_id) => ({
    session_id,
    reference: held.get(session_id) ?? null,
    candidate: other.get(session_id) ?? null,
  }));
}

export type Overlap = {
  shared: number;
  onlyReference: number;
  onlyCandidate: number;
};

export function recordingOverlap(rows: RecordingRow[]): Overlap {
  const counted = { shared: 0, onlyReference: 0, onlyCandidate: 0 };
  for (const row of rows) {
    if (row.reference !== null && row.candidate !== null) {
      counted.shared += 1;
    } else if (row.reference !== null) {
      counted.onlyReference += 1;
    } else {
      counted.onlyCandidate += 1;
    }
  }
  return counted;
}

export type Comparison = {
  accuracy: Delta;
  lift: Delta;
  presence: Delta;
  ordering: Delta;
  exact: Delta;
  /** One per density class, in `empty..saturated` order. */
  perClass: Delta[];
  recordings: Overlap;
  /**
   * Whether both runs were scored on the same recordings.
   *
   * Two models measured on different captures answer different questions, so a
   * difference between their scores indicates rather than ranks.
   */
  likeForLike: boolean;
};

export function comparison(reference: Evaluation, candidate: Evaluation): Comparison {
  const before = readings(reference);
  const after = readings(candidate);
  const recordings = recordingOverlap(recordingRows(reference, candidate));
  return {
    accuracy: delta(candidate.accuracy, reference.accuracy),
    lift: delta(after.lift, before.lift),
    presence: delta(after.presence.value, before.presence.value),
    ordering: delta(after.ordering.value, before.ordering.value),
    exact: delta(after.exact.value, before.exact.value),
    perClass: DENSITY_CLASSES.map((_, level) =>
      delta(recall(candidate.confusion, level), recall(reference.confusion, level)),
    ),
    recordings,
    likeForLike:
      recordings.shared > 0 && recordings.onlyReference === 0 && recordings.onlyCandidate === 0,
  };
}
