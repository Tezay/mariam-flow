// @vitest-environment jsdom
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import RecordingTimeline from './RecordingTimeline.svelte';
import { type Portrait } from '$lib/api/portrait';

type Range = { min: number; max: number };
type Hooks = { setScale: ((chart: unknown, key: string) => void)[] };

const charts = vi.hoisted(() => ({ built: 0 }));

/* jsdom has no canvas, so the library cannot run here. Like it, the stand-in
   fires the scale hook a microtask after the call, not during it. */
vi.mock('uplot', () => ({
  default: class {
    scales: { x: Partial<Range> } = { x: {} };
    bbox = { left: 48, top: 0, width: 600, height: 150 };
    cursor = { left: undefined, idx: null };
    #hooks: Hooks;

    constructor(options: { hooks: Hooks }) {
      charts.built += 1;
      this.#hooks = options.hooks;
    }

    setScale(key: string, range: Range) {
      // Bounded, so a rebuild loop fails the assertion instead of hanging the run.
      if (charts.built > 20) {
        return;
      }
      queueMicrotask(() => {
        this.scales.x = range;
        for (const hook of this.#hooks.setScale) {
          hook(this, key);
        }
      });
    }

    setSize() {}
    destroy() {}
  },
}));

const PORTRAIT: Portrait = {
  session_id: 'kit-0042-20260802T120000Z',
  computed_at_us: 0,
  started_at_us: 0,
  ended_at_us: 60_000_000,
  bin_us: 250_000,
  bins: 240,
  window_us: 5_000_000,
  hop_us: 1_000_000,
  feature_ts_us: [5_000_000, 6_000_000, 7_000_000],
  truncated: false,
  nodes: [
    {
      node_id: 'rx-1',
      frames: 600,
      rate_hz: 100,
      subcarriers: 64,
      inconsistent: 0,
      amp_min: 0,
      amp_max: 1,
      gaps: [],
      features: { motion_energy: [0.1, 0.2, 0.3] },
    },
  ],
  labels: [{ ts_us: 0, class: 0 }],
};

describe('RecordingTimeline', () => {
  beforeEach(() => {
    charts.built = 0;
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe() {}
        disconnect() {}
      },
    );
  });

  it('builds the chart once, however the chart reports its own scale back', async () => {
    render(RecordingTimeline, {
      props: { portrait: PORTRAIT, pixels: null, feature: 'motion_energy' },
    });

    for (let round = 0; round < 10; round += 1) {
      await tick();
      await Promise.resolve();
    }

    expect(charts.built).toBe(1);
  });
});
