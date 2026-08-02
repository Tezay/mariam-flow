/**
 * The live view's pure logic, kept out of the components so it can be
 * tested without a browser.
 *
 * Everything here is a function of its arguments: no clock read, no
 * locale guessed from the environment, no reactive state. The components
 * supply the current preferences and the current data, which is also what
 * makes each of these behaviours checkable in isolation.
 */

import { DENSITY_CLASSES, type DensityClass } from './api';

/**
 * Utility classes carrying each density colour.
 *
 * Written out rather than assembled from the class name at runtime. A
 * build-time optimiser only keeps what it can see in the source: a name
 * built by concatenation is invisible to it, and the colour it referred to
 * was once dropped from the stylesheet entirely, leaving an SVG fill to
 * fall back to black.
 *
 * Typing the map by [`DensityClass`] also makes it exhaustive — adding a
 * class to the output space fails compilation until its colour is declared
 * here, which no assertion on build output could guarantee.
 */
export const DENSITY_FILL: Record<DensityClass, string> = {
  empty: 'fill-density-empty',
  low: 'fill-density-low',
  medium: 'fill-density-medium',
  saturated: 'fill-density-saturated',
};

/** The same colours as a background, for swatches and legend keys. */
export const DENSITY_SWATCH: Record<DensityClass, string> = {
  empty: 'bg-density-empty',
  low: 'bg-density-low',
  medium: 'bg-density-medium',
  saturated: 'bg-density-saturated',
};

/** The class name of a stored integer, so translation keys stay checked. */
export function densityName(value: number): DensityClass {
  return DENSITY_CLASSES[value] ?? 'empty';
}

/**
 * Formats an appliance timestamp as a wall-clock time.
 *
 * The locale comes from the interface language rather than the browser's,
 * so a French dashboard never prints `11:41 PM` because the laptop happens
 * to be American.
 */
export function formatClock(us: number, locale: string, hour12: boolean): string {
  return new Date(us / 1000).toLocaleTimeString(locale, {
    hour: '2-digit',
    minute: '2-digit',
    hour12,
  });
}

/** Geometry of the history plot, in the SVG's own coordinates. */
export type PlotGeometry = {
  /** One point per minute, left to right. */
  points: { x: number; y: number }[];
  /** The value the vertical scale tops out at. */
  peak: number;
};

/**
 * Places the waiting-time series inside the plot box.
 *
 * The vertical scale is floored at one minute so a quiet hour does not
 * amplify rounding noise into a dramatic-looking curve, and the box starts
 * below the top edge so the peak's hover marker — a disc plus its surface
 * ring — is not clipped by the viewBox.
 */
export function plotGeometry(
  waits: number[],
  box: { width: number; top: number; height: number },
): PlotGeometry {
  const peak = Math.max(1, ...waits);
  const points = waits.map((wait, index) => ({
    x: (waits.length < 2 ? 0.5 : index / (waits.length - 1)) * box.width,
    y: box.top + box.height - (wait / peak) * box.height,
  }));
  return { points, peak };
}

/** An SVG path through the plotted points. */
export function linePath(points: { x: number; y: number }[]): string {
  return points
    .map((point, index) => `${index === 0 ? 'M' : 'L'}${point.x.toFixed(1)},${point.y.toFixed(1)}`)
    .join(' ');
}

/** The same path, closed onto the baseline to be filled as an area. */
export function areaPath(
  points: { x: number; y: number }[],
  box: { width: number; baseline: number },
): string {
  if (points.length === 0) {
    return '';
  }
  return `${linePath(points)} L${box.width},${box.baseline} L0,${box.baseline} Z`;
}

/** Index of the minute nearest a pointer position across the plot. */
export function nearestIndex(ratio: number, count: number): number {
  if (count === 0) {
    return 0;
  }
  return Math.max(0, Math.min(count - 1, Math.round(ratio * (count - 1))));
}
