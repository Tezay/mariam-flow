/** What the appliance is seeing now, and what it saw. */

import type { ServiceState } from './status';

/** Density classes, in their canonical order. */
export const DENSITY_CLASSES = ['empty', 'low', 'medium', 'saturated'] as const;

export type DensityClass = (typeof DENSITY_CLASSES)[number];

/**
 * A density as the canonical format encodes it: `0..3`, on disk and on the
 * wire alike. Only the live view is given the name instead.
 */
export type DensityCode = 0 | 1 | 2 | 3;

/** The name a density code stands for. */
export function densityName(code: DensityCode): DensityClass {
  return DENSITY_CLASSES[code];
}

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

type Listener = (snapshot: LiveSnapshot) => void;

const listeners = new Set<Listener>();
let source: EventSource | null = null;
let latest: LiveSnapshot | null = null;

function open() {
  const opened = new EventSource('/api/live');
  opened.onmessage = (event) => {
    let snapshot: LiveSnapshot;
    try {
      snapshot = JSON.parse(event.data) as LiveSnapshot;
    } catch {
      // A malformed frame is not worth tearing the stream down for; the
      // next one arrives in a second.
      return;
    }
    latest = snapshot;
    for (const listener of listeners) {
      listener(snapshot);
    }
  };
  source = opened;
}

/**
 * Subscribes to the live stream, returning a function that stops listening.
 *
 * One connection serves every subscriber: a browser grants an origin only a
 * handful, and one per screen would leave nothing able to say that the
 * appliance as a whole had gone quiet. A late subscriber is handed the last
 * snapshot rather than made to wait a tick for the next.
 */
export function subscribeLive(onSnapshot: Listener): () => void {
  listeners.add(onSnapshot);
  if (source === null) {
    open();
  } else if (latest !== null) {
    onSnapshot(latest);
  }
  return () => {
    listeners.delete(onSnapshot);
    if (listeners.size === 0) {
      source?.close();
      source = null;
      latest = null;
    }
  };
}

/**
 * Replaces the connection, once the appliance is known to answer again.
 *
 * `EventSource` retries a dropped connection, but not a refused one, and it
 * cannot tell a connection nobody holds any more from one that is merely
 * idle: an appliance restarted, or unplugged and plugged back, leaves it
 * closed or waiting for ever.
 */
export function reopenLive(): void {
  if (source !== null) {
    source.close();
    open();
  }
}

/**
 * Reads the folded minutes of the last `minutes` minutes, oldest first.
 *
 * Nothing rather than an empty list when the appliance does not answer: an
 * hour with no estimate and an hour that could not be read are different
 * things to show.
 */
export async function fetchHistory(minutes: number): Promise<MinuteSummary[] | null> {
  try {
    const response = await fetch(`/api/estimates?minutes=${minutes}`);
    return response.ok ? ((await response.json()) as MinuteSummary[]) : null;
  } catch {
    return null;
  }
}
