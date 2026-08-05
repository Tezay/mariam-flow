/** Which sensors the appliance has, and which hardware answers for each. */

import { put, send, type WriteOutcome } from './http';
import type { SensingNode } from './status';

/** A sender streaming to the appliance that no node mapping claims. */
export type Candidate = {
  address: string;
  datagrams: number;
  datagrams_per_second: number;
  tx_macs: string[];
  first_seen_us: number;
  last_seen_us: number;
};

/** The pairing the appliance offers, for the installer to confirm. */
export type Proposal = {
  tx_mac?: string;
  /** How many receivers reported that transmitter. */
  tx_agreement: number;
  receivers: { node_id: string; address: string }[];
};

export type Discovery = { candidates: Candidate[]; proposal: Proposal };

/** Reads what is streaming unpaired, with the pairing offered for it. */
export async function fetchDiscovery(): Promise<Discovery | null> {
  try {
    const response = await fetch('/api/discovery');
    return response.ok ? ((await response.json()) as Discovery) : null;
  } catch {
    return null;
  }
}

/** Replaces the paired nodes with the set the installer confirmed. */
export function saveNodes(nodes: SensingNode[]): Promise<WriteOutcome> {
  return put('/api/nodes', nodes);
}

/** Records where a node physically sits, or that it is unknown again. */
export async function describeNode(nodeId: string, position: string): Promise<WriteOutcome> {
  return send('PATCH', `/api/nodes/${encodeURIComponent(nodeId)}`, { position });
}

/** Points an existing node at the hardware that replaced it. */
export async function adoptHardware(
  nodeId: string,
  hardware: { address?: string; mac?: string },
): Promise<WriteOutcome> {
  return send('POST', `/api/nodes/${encodeURIComponent(nodeId)}/hardware`, hardware);
}
