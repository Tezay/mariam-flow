// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import SensorsPanel from './SensorsPanel.svelte';
import { type LiveSnapshot } from '$lib/api/live';
import { type SensingNode, type Status } from '$lib/api/status';

const NOW = Date.UTC(2026, 7, 2, 12) * 1000;
const SILENT = 30_000_000;

const NODES: SensingNode[] = [
  { node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00', position: 'above the pass' },
  { node_id: 'rx-1', role: 'rx', address: '192.168.4.51', position: 'above the entrance' },
  { node_id: 'rx-2', role: 'rx', address: '192.168.4.52' },
];

function status(): Status {
  return {
    kit_id: 'KIT-0042',
    site_name: 'RU',
    phase: { phase: 'operational' },
    readiness: {} as Status['readiness'],
    runtime: { mode: 'live' } as Status['runtime'],
    model_installed: true,
    service: { open: true },
    sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
    uplink: { mode: 'offline' },
    nodes: NODES,
  } as Status;
}

/** A live stream where rx-1 is current and rx-2 stopped long ago. */
function snapshot(): LiveSnapshot {
  return {
    stream: {
      running: true,
      estimating: true,
      frames: 1000,
      estimates: 10,
      last_frame_us: NOW,
      edge_stamped: true,
      nodes: {
        'rx-1': { frames: 900, frames_per_second: 42, last_frame_us: NOW },
        'rx-2': { frames: 100, frames_per_second: 0, last_frame_us: NOW - SILENT },
      },
    },
    service: { open: true },
    now_us: NOW,
  };
}

function stubLive(snap: LiveSnapshot | null) {
  vi.stubGlobal(
    'EventSource',
    class {
      onmessage: ((e: { data: string }) => void) | null = null;
      constructor() {
        if (snap) queueMicrotask(() => this.onmessage?.({ data: JSON.stringify(snap) }));
      }
      close() {}
      set onerror(_: unknown) {}
    },
  );
  vi.stubGlobal(
    'fetch',
    vi.fn(() =>
      Promise.resolve(
        new Response('{"candidates":[],"proposal":{"tx_agreement":0,"receivers":[]}}', {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    ),
  );
}

describe('SensorsPanel', () => {
  beforeEach(() => stubLive(snapshot()));
  afterEach(() => vi.unstubAllGlobals());

  it('shows where each sensor sits, transmitter included', () => {
    render(SensorsPanel, { props: { status: status(), onupdated() {} } });

    expect(screen.getByText('above the pass')).toBeInTheDocument();
    expect(screen.getByText('above the entrance')).toBeInTheDocument();
    expect(screen.getByText(/position not described/i)).toBeInTheDocument();
  });

  it('offers to describe and to replace every node, transmitter included', () => {
    render(SensorsPanel, { props: { status: status(), onupdated() {} } });

    expect(screen.getAllByRole('button', { name: /where it sits/i })).toHaveLength(3);
    expect(screen.getAllByRole('button', { name: /^replace$/i })).toHaveLength(3);
  });

  it('reports a receiver that stopped while its sibling keeps streaming', async () => {
    render(SensorsPanel, { props: { status: status(), onupdated() {} } });

    await vi.waitFor(() => expect(screen.getByText(/silent for/i)).toBeInTheDocument());
    expect(screen.getByText(/42 frames/i)).toBeInTheDocument();
  });
});
