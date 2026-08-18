// @vitest-environment jsdom
import { render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import ModelComparison from './ModelComparison.svelte';
import { type Evaluation, type StoredModel } from '$lib/api/models';

function model(id: string, name: string, active = false): StoredModel {
  return {
    id,
    manifest: { name, trained_at: '2026-06-24', sessions: 3 },
    imported_at_us: Date.UTC(2026, 7, 1, 10) * 1000,
    window_us: 5_000_000,
    receivers: 2,
    active,
    has_evaluation: true,
  };
}

/** Rows sum to 25 each, so the diagonal reads straight off as a percentage. */
const BEFORE = [
  [17, 8, 0, 0],
  [8, 17, 0, 0],
  [0, 0, 17, 8],
  [0, 0, 8, 17],
];

const AFTER = [
  [19, 6, 0, 0],
  [6, 19, 0, 0],
  [0, 0, 18, 7],
  [0, 0, 7, 18],
];

function evaluation(confusion: number[][], accuracy: number, captures: string[]): Evaluation {
  return {
    accuracy,
    baseline_accuracy: 0.25,
    confusion,
    windows: 100,
    splits: 3,
    receivers: ['rx-1', 'rx-2'],
    sessions: captures.map((session_id) => ({ session_id, windows: 25, support: [] })),
  };
}

function stubApi(reference: Evaluation, candidate: Evaluation) {
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      const held = String(url).endsWith('/before') ? reference : candidate;
      const detail = { ...model('id', 'x'), evaluation: held };
      return Promise.resolve(
        new Response(JSON.stringify(detail), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      );
    }),
  );
}

const SAME = ['monday', 'tuesday', 'wednesday'];
/** Every wording a difference can carry, so none is counted twice. */
const DIFFERENCE = /points better|points worse|no change/;

function open(reference: Evaluation, candidate: Evaluation) {
  stubApi(reference, candidate);
  render(ModelComparison, {
    props: {
      reference: model('before', 'June', true),
      candidate: model('after', 'September'),
      onback() {},
      onupdated() {},
      onchanged() {},
    },
  });
}

/** A row of one of the page's three tables, found by its own header. */
function row(table: RegExp, header: RegExp | string) {
  const scoped = within(screen.getByRole('table', { name: table }));
  return scoped.getByRole('rowheader', { name: header }).closest('tr') as HTMLElement;
}

const MEASURES = /^comparison$/i;
const RECORDINGS = /training recordings/i;

/**
 * Both renderings are in the document at once — a media query decides which is
 * drawn, and jsdom applies none — so every count has to name one of them.
 */
function table() {
  return within(screen.getByRole('table', { name: MEASURES }));
}

describe('ModelComparison', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('gives each measure one row, both models reading across it', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, SAME));

    await waitFor(() => expect(screen.getByRole('table', { name: MEASURES })).toBeTruthy());

    // Named once, then a cell per model: the reference, then the candidate
    // carrying the difference. Read as two columns this would be two labels.
    const cells = within(row(MEASURES, /accuracy/i)).getAllByRole('cell');
    expect(cells).toHaveLength(2);
    expect(cells[0]).toHaveTextContent('68 %');
    expect(cells[1]).toHaveTextContent('74 %');
    expect(within(cells[1]).getByText('6 points better')).toBeInTheDocument();
    expect(within(cells[0]).queryByText(DIFFERENCE)).not.toBeInTheDocument();
  });

  it('states each difference once, on the candidate', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, SAME));

    // Four scores and the four density levels, and nothing beside the
    // reference: a second set would mean both sides were being scored.
    await waitFor(() => expect(table().getAllByText(DIFFERENCE)).toHaveLength(8));

    // Presence rose four points, the ordering did not move: the wording
    // carries both, so neither reading depends on seeing an arrow.
    expect(within(row(MEASURES, /empty or occupied/i)).getByText('4 points better')).toBeTruthy();
    expect(within(row(MEASURES, /within one level/i)).getByText('no change')).toBeTruthy();
  });

  it('reads a level down its own row, not against the whole matrix', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, SAME));

    await waitFor(() => expect(screen.getByRole('table', { name: MEASURES })).toBeTruthy());

    // 17 of that level's 25 windows found, against 18 for the candidate.
    const cells = within(row(MEASURES, /saturated/i)).getAllByRole('cell');
    expect(cells[0]).toHaveTextContent('68 %');
    expect(cells[1]).toHaveTextContent('72 %');
  });

  it('names both models against every figure where a table would not fit', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, SAME));

    await waitFor(() => expect(screen.getByRole('list', { name: /scores/i })).toBeTruthy());

    // The stacked rendering names the models rather than relying on a column
    // position.
    const accuracy = within(screen.getByRole('list', { name: /scores/i }))
      .getByText(/^accuracy$/i)
      .closest('li') as HTMLElement;
    expect(within(accuracy).getByText('June')).toBeInTheDocument();
    expect(within(accuracy).getByText('September')).toBeInTheDocument();
    expect(within(accuracy).getAllByText(DIFFERENCE)).toHaveLength(1);
  });

  it('lists the recordings each run used, and marks the ones it never saw', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, [...SAME, 'thursday']));

    await waitFor(() => expect(screen.getByRole('table', { name: RECORDINGS })).toBeTruthy());

    const cells = within(row(RECORDINGS, 'thursday')).getAllByRole('cell');
    expect(cells[0]).toHaveTextContent(/not used/i);
    expect(cells[1]).toHaveTextContent('25');
  });

  it('warns when the two models were not scored on the same recordings', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, [...SAME, 'thursday']));

    await waitFor(() =>
      expect(screen.getByText(/scored on different recordings/i)).toBeInTheDocument(),
    );
    expect(screen.getByText(/3 of 4 in common/i)).toBeInTheDocument();
  });

  it('stays quiet when both runs were scored on the same recordings', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, SAME));

    await waitFor(() => expect(table().getAllByText(DIFFERENCE)).toHaveLength(8));

    expect(screen.queryByText(/scored on different recordings/i)).not.toBeInTheDocument();
  });

  it('offers to switch only to the model that is not already running', async () => {
    open(evaluation(BEFORE, 0.68, SAME), evaluation(AFTER, 0.74, SAME));

    await waitFor(() => expect(table().getAllByText(DIFFERENCE)).toHaveLength(8));

    expect(table().getAllByRole('button', { name: /^use$/i })).toHaveLength(1);
    expect(table().getByText(/in service/i)).toBeInTheDocument();
  });
});
