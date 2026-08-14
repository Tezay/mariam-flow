/** What the appliance can say about a capture it recorded. */

import { type DensityCode } from './live';

/** One receiver's account of a capture. */
export type NodePortrait = {
  node_id: string;
  frames: number;
  rate_hz: number;
  subcarriers: number;
  /** Frames dropped for disagreeing with the node's own subcarrier count. */
  inconsistent: number;
  amp_min: number;
  amp_max: number;
  /** Silences longer than a second, as `[from_us, to_us]`. */
  gaps: [number, number][];
  /**
   * The v1 features over time, one series per name.
   *
   * `null` where the window was too sparse to measure — a hole in the series,
   * which is also what training does with such a window.
   */
  features: Record<string, (number | null)[]>;
};

/**
 * A label as the appliance stamped it, straight out of `labels.ndjson`.
 *
 * The class is the canonical integer, not a name: the portrait mirrors the
 * recording rather than dressing it for display.
 */
export type PortraitLabel = { ts_us: number; class: DensityCode; count?: number };

/** Everything the appliance can say about one capture. */
export type Portrait = {
  session_id: string;
  computed_at_us: number;
  started_at_us: number;
  ended_at_us: number;
  bin_us: number;
  bins: number;
  window_us: number;
  hop_us: number;
  feature_ts_us: number[];
  /** A capture killed mid-write; everything before the break is described. */
  truncated: boolean;
  nodes: NodePortrait[];
  labels: PortraitLabel[];
};

/**
 * Describing a capture means parsing tens of megabytes, so the appliance
 * answers that it has started rather than holding the request open.
 */
export type PortraitOutcome =
  { kind: 'ready'; portrait: Portrait } | { kind: 'computing' } | { kind: 'missing' };

/** Asks for a capture's description, starting it if it does not exist yet. */
export async function fetchPortrait(sessionId: string): Promise<PortraitOutcome> {
  try {
    const response = await fetch(`/api/sessions/${encodeURIComponent(sessionId)}/portrait`);
    if (response.status === 202) {
      return { kind: 'computing' };
    }
    if (!response.ok) {
      return { kind: 'missing' };
    }
    return { kind: 'ready', portrait: (await response.json()) as Portrait };
  } catch {
    return { kind: 'missing' };
  }
}

/** The heatmap pixels of every node, concatenated in the portrait's order. */
export async function fetchHeatmap(sessionId: string): Promise<Uint8Array | null> {
  try {
    const response = await fetch(`/api/sessions/${encodeURIComponent(sessionId)}/portrait/heatmap`);
    return response.ok ? new Uint8Array(await response.arrayBuffer()) : null;
  } catch {
    return null;
  }
}
