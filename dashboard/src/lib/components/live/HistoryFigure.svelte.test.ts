// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import HistoryFigure from './HistoryFigure.svelte';
import { type Estimate, type MinuteSummary } from '$lib/api/live';
import { MINUTE_US } from '$lib/history';

const NOON = Date.UTC(2026, 9, 6, 12) * 1000;

function minute(offset: number): MinuteSummary {
  return {
    minute_us: NOON + offset * MINUTE_US,
    samples: 60,
    reliable_samples: 60,
    wait_minutes: 4,
    level: 2,
    class: 2,
    confidence: 0.9,
  };
}

const CURRENT: Estimate = {
  ts_us: NOON + 10 * MINUTE_US,
  wait_minutes: 4,
  people: 4,
  level: 2,
  class: 'medium',
  confidence: 0.9,
  reliable: true,
};

const BASE = { range: 30 as const, nowUs: NOON + 10 * MINUTE_US, onrange() {} };

const gaps = (container: HTMLElement) => container.querySelectorAll('[data-gap]');

describe('HistoryFigure', () => {
  it('marks the minutes nothing was estimated in, and names them in its legend', () => {
    const minutes = [minute(5), minute(6), minute(9)];
    const { container } = render(HistoryFigure, {
      props: { ...BASE, minutes, current: CURRENT },
    });

    // Before the first stored minute, and between the second and the last.
    expect(gaps(container)).toHaveLength(2);
    expect(screen.getByText('No estimate')).toBeInTheDocument();
  });

  it('ends on a gap while nothing is being estimated', () => {
    const { container } = render(HistoryFigure, {
      props: { ...BASE, minutes: [minute(0), minute(1)], current: null },
    });

    expect(gaps(container)).toHaveLength(2);
  });

  it('leaves the minute in progress out of the band, whose colours must not change', () => {
    const { container } = render(HistoryFigure, {
      props: { ...BASE, minutes: [minute(8), minute(9)], current: CURRENT },
    });

    expect(container.querySelectorAll('rect[class*="fill-density"]')).toHaveLength(2);
  });

  it('graduates the waiting time above its peak', () => {
    render(HistoryFigure, {
      props: { ...BASE, minutes: [minute(8), minute(9)], current: CURRENT },
    });

    for (const value of ['0', '2', '4', '6']) {
      expect(screen.getByText(value, { selector: 'text' })).toBeInTheDocument();
    }
  });

  it('says so when there is nothing to draw at all', () => {
    render(HistoryFigure, { props: { ...BASE, minutes: [], current: null } });

    expect(screen.getByText(/no history yet/i)).toBeInTheDocument();
  });

  it('offers the periods, and marks the one shown', async () => {
    const onrange = vi.fn();
    render(HistoryFigure, { props: { ...BASE, onrange, minutes: [minute(9)], current: CURRENT } });

    expect(screen.getByRole('button', { name: '30min' })).toHaveAttribute('aria-pressed', 'true');
    await userEvent.click(screen.getByRole('button', { name: '3h' }));

    expect(onrange).toHaveBeenCalledWith(180);
  });
});
