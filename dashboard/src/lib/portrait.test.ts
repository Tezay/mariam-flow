import { describe, expect, it } from 'vitest';

import { type NodePortrait, type Portrait } from './api/portrait';
import { HOLE, viridis } from './colormap';
import {
  type ClassStat,
  classStatistics,
  concerns,
  durationSeconds,
  featureSeries,
  fractionIn,
  heatmapImage,
  isZoomed,
  labelAt,
  labelSegments,
  nodePixelRange,
  overlaps,
  seconds,
  visibleBins,
  wholeCapture,
} from './portrait';

const SECOND = 1_000_000;

function node(overrides: Partial<NodePortrait> = {}): NodePortrait {
  return {
    node_id: 'rx-1',
    frames: 6000,
    rate_hz: 100,
    subcarriers: 2,
    inconsistent: 0,
    amp_min: 1,
    amp_max: 9,
    gaps: [],
    features: { amp_mean: [1, 2, 3] },
    ...overrides,
  };
}

function portrait(overrides: Partial<Portrait> = {}): Portrait {
  return {
    session_id: 'kit-20260810T120000Z',
    computed_at_us: 0,
    started_at_us: 0,
    ended_at_us: 60 * SECOND,
    bin_us: 250_000,
    bins: 3,
    window_us: 5 * SECOND,
    hop_us: SECOND,
    feature_ts_us: [5 * SECOND, 6 * SECOND, 7 * SECOND],
    truncated: false,
    nodes: [node()],
    labels: [{ ts_us: 0, class: 0 }],
    ...overrides,
  };
}

describe('nodePixelRange', () => {
  it('places each node after the ones the portrait lists before it', () => {
    const two = portrait({ nodes: [node(), node({ node_id: 'rx-2', subcarriers: 4 })] });

    expect(nodePixelRange(two, 0)).toEqual([0, 6]);
    expect(nodePixelRange(two, 1)).toEqual([6, 18]);
  });

  it('gives a silent node no pixels at all', () => {
    // It sent nothing, so it has no subcarrier count and contributes no bytes.
    const silent = portrait({ nodes: [node({ frames: 0, subcarriers: 0 }), node()] });

    expect(nodePixelRange(silent, 0)).toEqual([0, 0]);
    expect(nodePixelRange(silent, 1)).toEqual([0, 6]);
  });
});

describe('heatmapImage', () => {
  /** The pixel a canvas row and column carry, as `[r, g, b]`. */
  function pixelAt(image: ReturnType<typeof heatmapImage>, row: number, bin: number) {
    const at = (row * image.width + bin) * 4;
    return Array.from(image.data.slice(at, at + 3));
  }

  function within(got: number[], want: number[]) {
    got.forEach((channel, index) => expect(channel).toBeCloseTo(want[index], -0.5));
  }

  it('transposes the appliance’s bin-major bytes into canvas rows', () => {
    // The appliance stores a bin's subcarriers together; a canvas wants a row
    // per subcarrier. Bin 0 holds [10, 200].
    const pixels = new Uint8Array([10, 200, 20, 210, 30, 220]);

    const image = heatmapImage(pixels, portrait(), 0);

    expect([image.width, image.height]).toEqual([3, 2]);
    within(pixelAt(image, 0, 0), viridis((10 - 1) / 254));
    // Read untransposed, this row would carry 200's colour.
    within(pixelAt(image, 0, 1), viridis((20 - 1) / 254));
    within(pixelAt(image, 1, 0), viridis((200 - 1) / 254));
  });

  it('paints a hole in a colour the amplitude ramp never reaches', () => {
    // Zero is reserved: rendered on the ramp it would read as an amplitude,
    // which is a measurement rather than the absence of one.
    const pixels = new Uint8Array([0, 255, 0, 255, 0, 255]);

    const image = heatmapImage(pixels, portrait(), 0);

    within(pixelAt(image, 0, 0), HOLE);
    within(pixelAt(image, 1, 0), viridis(1));
  });
});

describe('labelSegments', () => {
  it('runs each label until the next one, and the last to the end', () => {
    const labelled = portrait({
      labels: [
        { ts_us: 0, class: 0 },
        { ts_us: 20 * SECOND, class: 2 },
      ],
    });

    expect(labelSegments(labelled)).toEqual([
      { from_us: 0, to_us: 20 * SECOND, density: 'empty' },
      { from_us: 20 * SECOND, to_us: 60 * SECOND, density: 'medium' },
    ]);
  });

  it('drops a label pressed twice on the same instant', () => {
    const corrected = portrait({
      labels: [
        { ts_us: 10 * SECOND, class: 1 },
        { ts_us: 10 * SECOND, class: 2 },
      ],
    });

    expect(labelSegments(corrected)).toEqual([
      { from_us: 10 * SECOND, to_us: 60 * SECOND, density: 'medium' },
    ]);
  });
});

describe('labelAt', () => {
  const labelled = portrait({
    labels: [
      { ts_us: 10 * SECOND, class: 0 },
      { ts_us: 30 * SECOND, class: 3 },
    ],
  });

  it('answers the level in force at an instant, not the nearest mark', () => {
    expect(labelAt(labelled, 20 * SECOND)).toBe('empty');
    expect(labelAt(labelled, 45 * SECOND)).toBe('saturated');
  });

  it('answers nothing before the first mark, which training also discards', () => {
    expect(labelAt(labelled, 5 * SECOND)).toBeNull();
  });
});

describe('the shared time domain', () => {
  it('measures every instant in seconds from the start of the capture', () => {
    // The appliance stamps in epoch microseconds; every panel draws against
    // this, so the conversion happens once.
    expect(seconds(portrait(), 30 * SECOND)).toBe(30);
    expect(durationSeconds(portrait())).toBe(60);
  });

  it('places an instant across whatever stretch is on screen', () => {
    expect(fractionIn({ from: 0, to: 60 }, 30)).toBe(0.5);
    expect(fractionIn({ from: 20, to: 40 }, 30)).toBe(0.5);
  });

  it('reports a position outside the viewport rather than clamping it', () => {
    // A caller drawing a stretch needs to know how far outside it starts, or
    // a segment straddling the edge would be pinned to it.
    expect(fractionIn({ from: 20, to: 40 }, 10)).toBe(-0.5);
    expect(fractionIn({ from: 20, to: 40 }, 50)).toBe(1.5);
  });

  it('answers zero rather than dividing by an instantaneous capture', () => {
    expect(fractionIn({ from: 5, to: 5 }, 5)).toBe(0);
    expect(durationSeconds(portrait({ ended_at_us: 0 }))).toBe(0);
  });

  it('knows whether anything is hidden, so a reset can be offered', () => {
    const whole = portrait();
    expect(isZoomed(whole, wholeCapture(whole))).toBe(false);
    expect(isZoomed(whole, { from: 10, to: 20 })).toBe(true);
  });
});

describe('visibleBins', () => {
  it('rounds outwards, so a half-shown column is drawn rather than dropped', () => {
    // 250 ms bins: seconds 1.1 to 1.6 touch bins 4 through 6.
    const p = portrait({ bins: 240 });

    expect(visibleBins(p, { from: 1.1, to: 1.6 })).toEqual([4, 7]);
  });

  it('never reaches past the columns the capture holds', () => {
    const p = portrait({ bins: 8 });

    expect(visibleBins(p, { from: 0, to: 600 })).toEqual([0, 8]);
    expect(visibleBins(p, { from: -5, to: 1 })).toEqual([0, 4]);
  });
});

describe('featureSeries', () => {
  it('gives seconds from the start of the capture, not appliance timestamps', () => {
    const [seconds, values] = featureSeries(portrait(), node(), 'amp_mean');

    expect(seconds).toEqual([5, 6, 7]);
    expect(values).toEqual([1, 2, 3]);
  });

  it('answers an empty series for a feature a node never produced', () => {
    const [, values] = featureSeries(portrait(), node({ features: {} }), 'amp_mean');

    expect(values).toEqual([]);
  });
});

describe('classStatistics', () => {
  /** Four samples a second apart, from second 1. */
  function measured(values: (number | null)[], labels: Portrait['labels']) {
    return portrait({
      feature_ts_us: values.map((_, index) => (index + 1) * SECOND),
      nodes: [node({ features: { motion_energy: values } })],
      labels,
    });
  }

  it('summarises the measurement under each level that was marked', () => {
    const p = measured(
      [1, 1, 5, 5],
      [
        { ts_us: 0, class: 0 },
        { ts_us: 2.5 * SECOND, class: 3 },
      ],
    );

    expect(classStatistics(p, p.nodes[0], 'motion_energy')).toEqual([
      { density: 'empty', samples: 2, mean: 1, deviation: 0 },
      { density: 'saturated', samples: 2, mean: 5, deviation: 0 },
    ]);
  });

  it('leaves out what training leaves out: holes, and windows before the first mark', () => {
    const p = measured([9, null, 3, 3], [{ ts_us: 2.5 * SECOND, class: 2 }]);

    // Second 1 precedes the first mark and second 2 has no value; only the
    // two marked, measured windows count.
    expect(classStatistics(p, p.nodes[0], 'motion_energy')).toEqual([
      { density: 'medium', samples: 2, mean: 3, deviation: 0 },
    ]);
  });

  it('reports levels in density order, whatever order they were pressed', () => {
    const p = measured(
      [5, 1],
      [
        { ts_us: 0.5 * SECOND, class: 3 },
        { ts_us: 1.5 * SECOND, class: 0 },
      ],
    );

    expect(classStatistics(p, p.nodes[0], 'motion_energy').map((s) => s.density)).toEqual([
      'empty',
      'saturated',
    ]);
  });
});

describe('overlaps', () => {
  const stat = (mean: number, deviation: number): ClassStat => ({
    density: 'empty',
    samples: 10,
    mean,
    deviation,
  });

  it('sees two levels no threshold on this measurement can separate', () => {
    expect(overlaps(stat(3, 1), stat(4, 1))).toBe(true);
  });

  it('sees two levels that stand apart', () => {
    expect(overlaps(stat(1, 0.1), stat(5, 0.2))).toBe(false);
  });

  it('answers on the spreads, not on the distance between the means', () => {
    // Far apart yet overlapping: means alone would call this separated.
    expect(overlaps(stat(1, 5), stat(9, 5))).toBe(true);
  });
});

describe('concerns', () => {
  it('says nothing about a capture worth training on', () => {
    expect(concerns(portrait())).toEqual([]);
  });

  it('names the receiver that never streamed', () => {
    const half = portrait({
      nodes: [node(), node({ node_id: 'rx-2', frames: 0, subcarriers: 0 })],
    });

    expect(concerns(half)).toEqual([{ key: 'silent', node: 'rx-2' }]);
  });

  it('reports a hole separately from a silence, since they send you elsewhere', () => {
    const holed = portrait({ nodes: [node({ gaps: [[1, 2]] })] });

    expect(concerns(holed)).toEqual([{ key: 'gaps', node: 'rx-1', value: 1 }]);
  });

  it('flags a capture that was never labelled or never finished', () => {
    const bad = portrait({ truncated: true, labels: [] });

    expect(concerns(bad).map((c) => c.key)).toEqual(['truncated', 'unlabelled']);
  });

  it('flags a capture too short to fill an analysis window', () => {
    const brief = portrait({ ended_at_us: 4 * SECOND });

    expect(concerns(brief)).toContainEqual({ key: 'short', value: 4 });
  });
});
