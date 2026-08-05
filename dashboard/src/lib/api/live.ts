/** What the appliance is seeing now, and what it saw. */

import type { ServiceState } from './status';

/** Density classes, in their canonical order. */
export const DENSITY_CLASSES = ['empty', 'low', 'medium', 'saturated'] as const;

export type DensityClass = (typeof DENSITY_CLASSES)[number];

/** One receiver's contribution to the stream. */
export type NodeHealth = {
  frames: number;
  frames_per_second: number;
  last_frame_us?: number;
};

/** How the frames are arriving. */
export type StreamHealth = {
  running: boolean;
  /** Separate from `running`: an appliance can read without estimating. */
  estimating: boolean;
  frames: number;
  estimates: number;
  last_frame_us?: number;
  /** Whether frame timestamps come from the appliance clock. */
  edge_stamped: boolean;
  nodes: Record<string, NodeHealth>;
};

/** The current estimate, as the administration surface reports it. */
export type Estimate = {
  ts_us: number;
  wait_minutes: number;
  people: number;
  level: number;
  class: DensityClass;
  confidence: number;
  reliable: boolean;
};

/** What the live stream sends on every tick. */
export type LiveSnapshot = {
  estimate?: Estimate;
  stream: StreamHealth;
  service: ServiceState;
  now_us: number;
  /**
   * Newest journal row.
   *
   * Carried here rather than polled for: this stream already ticks every
   * second, so a reader of the journal learns that something happened within
   * a second and at the cost of one integer.
   */
  journal_id?: number;
};

/** One folded minute of history. */
export type MinuteSummary = {
  minute_us: number;
  samples: number;
  reliable_samples: number;
  wait_minutes: number;
  level: number;
  /** Encoded as its integer value on the wire, as everywhere else. */
  class: number;
  confidence: number;
};

/**
 * Subscribes to the live stream, returning a function that closes it.
 *
 * `EventSource` reconnects on its own after a dropped connection, which is
 * the behaviour that matters on a phone carried across a service hall — so
 * there is no retry logic here, only a way to stop listening.
 */
export function subscribeLive(
  onSnapshot: (snapshot: LiveSnapshot) => void,
  onError?: () => void,
): () => void {
  const source = new EventSource('/api/live');
  source.onmessage = (event) => {
    try {
      onSnapshot(JSON.parse(event.data) as LiveSnapshot);
    } catch {
      // A malformed frame is not worth tearing the stream down for; the
      // next one arrives in a second.
    }
  };
  source.onerror = () => onError?.();
  return () => source.close();
}

/** Reads the folded minutes of the last `minutes` minutes, oldest first. */
export async function fetchHistory(minutes: number): Promise<MinuteSummary[]> {
  try {
    const response = await fetch(`/api/estimates?minutes=${minutes}`);
    if (!response.ok) {
      return [];
    }
    return (await response.json()) as MinuteSummary[];
  } catch {
    return [];
  }
}
