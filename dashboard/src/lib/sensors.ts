/**
 * What the sensors screen has to work out before it can show anything.
 *
 * The appliance reports what it hears and what it is configured for; which of
 * those two disagree, and what that means, is decided here so it can be
 * tested without a browser.
 */

import type { Candidate, Discovery, NodeHealth, SensingNode } from './api';

/** A node lagging the stream by more than this is treated as silent. */
export const SILENT_AFTER_US = 10_000_000;

/** What a node is doing, as the screen phrases it. */
export type NodeState =
  /** Frames are arriving from it. */
  | { kind: 'streaming'; framesPerSecond: number }
  /** It was heard and has stopped. */
  | { kind: 'silent'; seconds: number }
  /** Nothing has ever arrived from it. */
  | { kind: 'never-heard' };

/** What the appliance says about the stream as a whole. */
export type StreamReference = {
  last_frame_us?: number;
  /** Whether frame timestamps come from the appliance clock. */
  edge_stamped?: boolean;
};

/**
 * What one receiver is doing.
 *
 * Silence is measured against the newest frame of the stream, and against the
 * appliance clock as well when the frames are edge-stamped. Against the stream
 * alone a node cannot lag itself, so one receiver could never be reported
 * silent and a stream where every receiver stopped would look healthy; against
 * the clock alone a replayed capture — whose timestamps come from the
 * recording — would report every node dead. Comparing between nodes still does
 * its work whenever some are alive, which is what tells one dead sensor from a
 * dead network.
 */
export function receiverState(
  health: NodeHealth | undefined,
  stream: StreamReference | undefined,
  nowUs?: number,
): NodeState {
  if (!health?.last_frame_us) {
    return { kind: 'never-heard' };
  }
  const clock = stream?.edge_stamped ? (nowUs ?? 0) : 0;
  const reference = Math.max(stream?.last_frame_us ?? 0, clock, health.last_frame_us);
  const lag = reference - health.last_frame_us;
  if (lag > SILENT_AFTER_US) {
    return { kind: 'silent', seconds: Math.round(lag / 1_000_000) };
  }
  return { kind: 'streaming', framesPerSecond: health.frames_per_second };
}

/**
 * The transmitter is never seen directly.
 *
 * It does not join the access point; it is known only as the MAC the
 * receivers report having sensed. So it is reported as heard when frames are
 * arriving at all, and there is nothing else to say about it.
 */
export function transmitterHeard(
  nodes: SensingNode[],
  health: Record<string, NodeHealth>,
): boolean {
  return nodes.some((node) => node.role === 'rx' && (health[node.node_id]?.frames ?? 0) > 0);
}

/**
 * Senders that could be the replacement for a failed node.
 *
 * Anything already paired is excluded: offering a working sibling as the
 * replacement for its neighbour is how an installation ends up with two nodes
 * pointing at one sensor.
 */
export function replacements(discovery: Discovery | null, nodes: SensingNode[]): Candidate[] {
  const taken = new Set(nodes.map((node) => node.address).filter(Boolean));
  return (discovery?.candidates ?? [])
    .filter((candidate) => !taken.has(candidate.address))
    .sort((a, b) => b.datagrams_per_second - a.datagrams_per_second);
}

/**
 * The transmitter the receivers now agree on, when it is not the configured
 * one.
 *
 * A replaced transmitter is never seen as a sender; it shows up as a new MAC
 * inside the frames the receivers report, which is the only evidence there is
 * that the hardware changed.
 */
export function transmitterReplacement(
  discovery: Discovery | null,
  transmitter: SensingNode | null,
): { mac: string; agreement: number } | null {
  const mac = discovery?.proposal.tx_mac;
  if (!mac || !transmitter || mac === transmitter.mac) {
    return null;
  }
  return { mac, agreement: discovery?.proposal.tx_agreement ?? 0 };
}

/** The positions the recording form starts from. */
export function knownPositions(nodes: SensingNode[]): Record<string, string> {
  return Object.fromEntries(nodes.map((node) => [node.node_id, node.position ?? '']));
}
