import { describe, expect, it } from 'vitest';

import type { Candidate, Discovery, NodeHealth, SensingNode } from './api';
import {
  knownPositions,
  receiverState,
  replacements,
  transmitterHeard,
  transmitterReplacement,
  SILENT_AFTER_US,
} from './sensors';

const NOW = 1_785_600_000_000_000;

function health(lastFrameUs?: number, rate = 40): NodeHealth {
  return { frames: lastFrameUs ? 1000 : 0, frames_per_second: rate, last_frame_us: lastFrameUs };
}

function candidate(address: string, rate: number): Candidate {
  return {
    address,
    datagrams: 500,
    datagrams_per_second: rate,
    tx_macs: ['1a:00:00:00:00:00'],
    first_seen_us: NOW - 60_000_000,
    last_seen_us: NOW,
  };
}

const NODES: SensingNode[] = [
  { node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00' },
  { node_id: 'rx-1', role: 'rx', address: '192.168.4.51', position: 'above the entrance' },
  { node_id: 'rx-2', role: 'rx', address: '192.168.4.52' },
];

describe('receiverState', () => {
  it('reports a node that has never been heard apart from one that stopped', () => {
    expect(receiverState(undefined, NOW)).toEqual({ kind: 'never-heard' });
    expect(receiverState(health(undefined), NOW)).toEqual({ kind: 'never-heard' });
  });

  it('reports the rate while frames are arriving', () => {
    expect(receiverState(health(NOW, 38), NOW)).toEqual({
      kind: 'streaming',
      framesPerSecond: 38,
    });
  });

  it('measures silence against the stream, not the clock', () => {
    // All sensors stopping is a different fault from one stopping, and only
    // the comparison between them tells the two apart.
    const stopped = health(NOW - 5 * SILENT_AFTER_US);
    expect(receiverState(stopped, NOW)).toEqual({ kind: 'silent', seconds: 50 });
    // The same node, when nothing else is arriving either, is not singled out.
    expect(receiverState(stopped, stopped.last_frame_us)).toEqual({
      kind: 'streaming',
      framesPerSecond: 40,
    });
  });
});

describe('transmitterHeard', () => {
  it('is heard through the receivers, never directly', () => {
    expect(transmitterHeard(NODES, { 'rx-1': health(NOW) })).toBe(true);
    expect(transmitterHeard(NODES, {})).toBe(false);
  });
});

describe('replacements', () => {
  const discovery: Discovery = {
    candidates: [
      candidate('192.168.4.51', 40),
      candidate('192.168.4.57', 12),
      candidate('192.168.4.58', 41),
    ],
    proposal: { tx_agreement: 0, receivers: [] },
  };

  it('never offers a sender that is already paired', () => {
    // Offering a working sibling is how an installation ends up with two
    // nodes pointing at one sensor.
    expect(replacements(discovery, NODES).map((c) => c.address)).toEqual([
      '192.168.4.58',
      '192.168.4.57',
    ]);
  });

  it('puts the busiest sender first, since that is the one just powered on', () => {
    expect(replacements(discovery, NODES)[0].datagrams_per_second).toBe(41);
  });

  it('has nothing to offer before anything has been heard', () => {
    expect(replacements(null, NODES)).toEqual([]);
  });
});

describe('knownPositions', () => {
  it('starts a recording from what the installation already recorded', () => {
    expect(knownPositions(NODES)).toEqual({
      'tx-1': '',
      'rx-1': 'above the entrance',
      'rx-2': '',
    });
  });
});

describe('transmitterReplacement', () => {
  const offering = (mac?: string): Discovery => ({
    candidates: [],
    proposal: mac
      ? { tx_mac: mac, tx_agreement: 2, receivers: [] }
      : { tx_agreement: 0, receivers: [] },
  });
  const transmitter = NODES[0];

  it('says nothing while the receivers still agree on the configured one', () => {
    expect(transmitterReplacement(offering('1a:00:00:00:00:00'), transmitter)).toBeNull();
  });

  it('offers the address the receivers now report instead', () => {
    // A replaced transmitter is never seen as a sender; a new MAC inside the
    // frames is the only evidence the hardware changed.
    expect(transmitterReplacement(offering('1a:00:00:00:00:99'), transmitter)).toEqual({
      mac: '1a:00:00:00:00:99',
      agreement: 2,
    });
  });

  it('has nothing to offer before anything has been sensed', () => {
    expect(transmitterReplacement(offering(), transmitter)).toBeNull();
    expect(transmitterReplacement(null, transmitter)).toBeNull();
  });
});
