/**
 * The live view's pure logic, kept out of the components so it can be
 * tested without a browser.
 *
 * Everything here is a function of its arguments: no clock read, no
 * locale guessed from the environment, no reactive state. The components
 * supply the current preferences and the current data, which is also what
 * makes each of these behaviours checkable in isolation.
 */

import { DENSITY_CLASSES, type DensityClass, type Estimate, type LiveSnapshot } from './api/live';
import { SILENT_AFTER_US } from './sensors';

/** What the appliance is doing with its stream, as a screen has to say it. */
export type Activity =
  | { kind: 'recording' }
  | { kind: 'closed' }
  /** No estimator is attached: no model, or a site not yet described. */
  | { kind: 'idle' }
  /** Frames are arriving and the first window has not filled. */
  | { kind: 'starting' }
  | { kind: 'estimating'; estimate: Estimate }
  /** An estimator is attached and nothing recent has come out of it. */
  | { kind: 'interrupted'; last: Estimate | null };

/**
 * Reads what the appliance is doing from what it reports each second.
 *
 * The appliance keeps its last estimate until another replaces it, so one
 * counts as current only while recent — by the threshold that calls a receiver
 * silent, for the two to turn together. Age is read on the appliance clock, or
 * on the stream's own for a replayed capture, whose timestamps are its own.
 */
export function activity(snapshot: LiveSnapshot): Activity {
  if (snapshot.recording) {
    return { kind: 'recording' };
  }
  if (!snapshot.service.open) {
    return { kind: 'closed' };
  }
  const { estimate, stream } = snapshot;
  if (!stream.estimating) {
    return { kind: 'idle' };
  }
  const clock = stream.edge_stamped ? snapshot.now_us : (stream.last_frame_us ?? 0);
  const recent = (us: number | undefined) => us !== undefined && clock - us <= SILENT_AFTER_US;
  if (estimate && recent(estimate.ts_us)) {
    return { kind: 'estimating', estimate };
  }
  if (!estimate && recent(stream.last_frame_us)) {
    return { kind: 'starting' };
  }
  return { kind: 'interrupted', last: estimate ?? null };
}

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
