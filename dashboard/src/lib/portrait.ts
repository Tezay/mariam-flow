/**
 * Turning a capture's description into what a screen draws.
 *
 * The appliance sends the heatmap as one byte per `bin × subcarrier`, all
 * nodes concatenated in the order it lists them, with **zero reserved for a
 * bin no frame landed in**. Everything here assumes that encoding.
 */

import { type NodePortrait, type Portrait } from './api/portrait';
import { DENSITY_CLASSES, type DensityClass, densityName } from './api/live';
import { HOLE, viridis } from './colormap';

/** The byte the appliance writes where a capture has a hole. */
const NO_DATA = 0;

/** Where one node's pixels sit in the concatenated heatmap. */
export function nodePixelRange(portrait: Portrait, index: number): [number, number] {
  let start = 0;
  for (let node = 0; node < index; node += 1) {
    start += portrait.bins * (portrait.nodes[node]?.subcarriers ?? 0);
  }
  return [start, start + portrait.bins * (portrait.nodes[index]?.subcarriers ?? 0)];
}

/**
 * One node's heatmap as canvas pixels, transposed to `subcarrier × bin`.
 *
 * The appliance stores a bin's subcarriers together, which is the order it
 * accumulates them in; a canvas wants a row per subcarrier.
 */
export function heatmapImage(
  pixels: Uint8Array,
  portrait: Portrait,
  index: number,
  bins: [number, number] = [0, portrait.bins],
): { width: number; height: number; data: Uint8ClampedArray<ArrayBuffer> } {
  const [firstBin, lastBin] = bins;
  const width = Math.max(0, lastBin - firstBin);
  const height = portrait.nodes[index]?.subcarriers ?? 0;
  const [start] = nodePixelRange(portrait, index);
  const data = new Uint8ClampedArray(new ArrayBuffer(width * height * 4));

  for (let column = 0; column < width; column += 1) {
    const bin = firstBin + column;
    for (let row = 0; row < height; row += 1) {
      const value = pixels[start + bin * height + row] ?? NO_DATA;
      const at = (row * width + column) * 4;
      const [r, g, b] = value === NO_DATA ? HOLE : viridis((value - 1) / 254);
      data[at] = r;
      data[at + 1] = g;
      data[at + 2] = b;
      data[at + 3] = 255;
    }
  }
  return { width, height, data };
}

/** One stretch during which the operator said the zone held one density. */
export type LabelSegment = { from_us: number; to_us: number; density: DensityClass };

/**
 * The label track as drawable stretches.
 *
 * A label declares a state until the next one, so the last runs to the end of
 * the capture rather than stopping where it was pressed.
 */
export function labelSegments(portrait: Portrait): LabelSegment[] {
  return portrait.labels
    .map((label, index) => ({
      from_us: label.ts_us,
      to_us: portrait.labels[index + 1]?.ts_us ?? portrait.ended_at_us,
      density: densityName(label.class),
    }))
    .filter((segment) => segment.to_us > segment.from_us);
}

/** The level marked at an instant, or nothing before the first mark. */
export function labelAt(portrait: Portrait, ts_us: number): DensityClass | null {
  const segment = labelSegments(portrait).findLast((it) => it.from_us <= ts_us);
  return segment ? segment.density : null;
}

/** Seconds from the start of the capture — the domain every panel shares. */
export function seconds(portrait: Portrait, ts_us: number): number {
  return (ts_us - portrait.started_at_us) / 1_000_000;
}

/** Seconds a capture spans. */
export function durationSeconds(portrait: Portrait): number {
  return Math.max(0, seconds(portrait, portrait.ended_at_us));
}

/** The stretch of the capture on screen, in seconds from its start. */
export type Viewport = { from: number; to: number };

/** The whole capture. */
export function wholeCapture(portrait: Portrait): Viewport {
  return { from: 0, to: durationSeconds(portrait) };
}

/** Whether a viewport shows anything less than the whole capture. */
export function isZoomed(portrait: Portrait, viewport: Viewport): boolean {
  const whole = wholeCapture(portrait);
  return viewport.from > whole.from + 1e-6 || viewport.to < whole.to - 1e-6;
}

/**
 * Where an instant sits across a viewport, as a fraction of its width.
 *
 * Unclamped on purpose: a caller drawing a stretch needs to know how far
 * outside it starts, not that it starts at the edge.
 */
export function fractionIn(viewport: Viewport, at: number): number {
  const span = viewport.to - viewport.from;
  return span <= 0 ? 0 : (at - viewport.from) / span;
}

/**
 * The heatmap columns a viewport covers, as `[first, lastExclusive]`.
 *
 * Rounded outwards so a partially visible column is drawn rather than
 * dropped, which would leave a sliver of blank at either edge.
 */
export function visibleBins(portrait: Portrait, viewport: Viewport): [number, number] {
  const perBin = portrait.bin_us / 1_000_000;
  if (perBin <= 0) {
    return [0, portrait.bins];
  }
  const first = Math.max(0, Math.floor(viewport.from / perBin));
  const last = Math.min(portrait.bins, Math.ceil(viewport.to / perBin));
  return [first, Math.max(first, last)];
}

/** How much time one heatmap column covers — the resolution actually held. */
export function binSeconds(portrait: Portrait): number {
  return portrait.bin_us / 1_000_000;
}

/** One feature series against the seconds it was sampled at, ready for a plot. */
export function featureSeries(
  portrait: Portrait,
  node: NodePortrait,
  name: string,
): [number[], (number | null)[]] {
  const at = portrait.feature_ts_us.map((ts) => seconds(portrait, ts));
  return [at, node.features[name] ?? []];
}

/** What one marked level looked like on one measurement. */
export type ClassStat = {
  density: DensityClass;
  samples: number;
  mean: number;
  deviation: number;
};

/**
 * The selected measurement summarised per marked level.
 *
 * This is the protocol's question made numeric — do the levels separate on
 * this measurement — and it can be read before any model exists. Windows with
 * no value and windows before the first mark are left out, exactly as
 * training leaves them out.
 */
export function classStatistics(portrait: Portrait, node: NodePortrait, name: string): ClassStat[] {
  const values = new Map<DensityClass, number[]>();
  const series = node.features[name] ?? [];
  portrait.feature_ts_us.forEach((ts, index) => {
    const value = series[index];
    const density = labelAt(portrait, ts);
    if (value === null || value === undefined || density === null) {
      return;
    }
    values.set(density, [...(values.get(density) ?? []), value]);
  });

  return DENSITY_CLASSES.filter((density) => values.has(density)).map((density) => {
    const samples = values.get(density) ?? [];
    const mean = samples.reduce((sum, value) => sum + value, 0) / samples.length;
    const variance = samples.reduce((sum, value) => sum + (value - mean) ** 2, 0) / samples.length;
    return { density, samples: samples.length, mean, deviation: Math.sqrt(variance) };
  });
}

/**
 * Whether two levels' spreads run into each other on this measurement.
 *
 * Means alone cannot answer it: two levels a long way apart whose spreads
 * overlap are not told apart by any threshold on this measurement.
 */
export function overlaps(a: ClassStat, b: ClassStat): boolean {
  return Math.abs(a.mean - b.mean) < a.deviation + b.deviation;
}

/** What a reader has to know before deciding a capture is worth training on. */
export type Concern = { key: ConcernKey; node?: string; value?: number };

export type ConcernKey = 'silent' | 'gaps' | 'truncated' | 'unlabelled' | 'inconsistent' | 'short';

/** A capture too brief to fill one analysis window yields nothing to train on. */
const MIN_USEFUL_SECONDS = 30;

/**
 * Everything wrong with a capture, worst first.
 *
 * Reported rather than scored: a capture with a silent receiver and one with a
 * hole in the middle are both unusable, and for different reasons that lead to
 * different things to go and check.
 */
export function concerns(portrait: Portrait): Concern[] {
  const found: Concern[] = [];
  for (const node of portrait.nodes) {
    if (node.frames === 0) {
      found.push({ key: 'silent', node: node.node_id });
    } else if (node.gaps.length > 0) {
      found.push({ key: 'gaps', node: node.node_id, value: node.gaps.length });
    }
    if (node.inconsistent > 0) {
      found.push({ key: 'inconsistent', node: node.node_id, value: node.inconsistent });
    }
  }
  if (portrait.truncated) {
    found.push({ key: 'truncated' });
  }
  if (portrait.labels.length === 0) {
    found.push({ key: 'unlabelled' });
  }
  if (durationSeconds(portrait) < MIN_USEFUL_SECONDS) {
    found.push({ key: 'short', value: Math.round(durationSeconds(portrait)) });
  }
  return found;
}
