/**
 * Reading a model's evaluation (ADR 0024).
 *
 * Every function here takes the confusion matrix in the orientation the
 * training run computes it: **rows are truth, columns are prediction**. Read
 * the other way round, every result below is inverted.
 */

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
