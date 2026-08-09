// @vitest-environment jsdom
import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import ConfusionMatrix from './ConfusionMatrix.svelte';

const CONFUSION = [
  [612, 74, 11, 3],
  [88, 401, 118, 22],
  [14, 131, 356, 96],
  [6, 27, 142, 330],
];

describe('ConfusionMatrix', () => {
  it('is a table a screen reader can walk, not a picture of one', () => {
    render(ConfusionMatrix, { props: { confusion: CONFUSION } });

    // Truth heads the rows and prediction heads the columns; both directions
    // have to be reachable or the matrix cannot be read without sight.
    expect(screen.getAllByRole('rowheader')).toHaveLength(4);
    expect(screen.getAllByRole('columnheader')).toHaveLength(4);
  });

  it('places truth in rows and prediction in columns', () => {
    render(ConfusionMatrix, { props: { confusion: CONFUSION } });

    const saturated = screen.getByRole('rowheader', { name: /saturated/i }).closest('tr');
    const cells = within(saturated as HTMLElement).getAllByRole('cell');

    // Truly saturated, answered medium 142 times: the third column of the
    // last row. Read the other way round this would be 96.
    expect(cells.map((cell) => cell.textContent?.trim())).toEqual(['6', '27', '142', '330']);
  });

  it('renders every count, including the classes a model never answered', () => {
    render(ConfusionMatrix, {
      props: {
        confusion: [
          [0, 0, 0, 0],
          [0, 0, 0, 0],
          [0, 0, 0, 0],
          [0, 0, 0, 0],
        ],
      },
    });

    // A class nobody ever recorded still gets its row: an absent row would
    // read as a class the model handles, rather than one never tested.
    const rows = screen.getAllByRole('rowheader').map((header) => header.closest('tr'));
    expect(rows).toHaveLength(4);
    for (const row of rows) {
      expect(within(row as HTMLElement).getAllByRole('cell')).toHaveLength(4);
    }
  });
});
