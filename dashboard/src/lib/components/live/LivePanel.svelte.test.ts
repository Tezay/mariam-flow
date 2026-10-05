// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import LivePanel from './LivePanel.svelte';
import { type LiveSnapshot } from '$lib/api/live';
import { type SensingNode } from '$lib/api/status';
import { SILENT_AFTER_US } from '$lib/sensors';
import { Stream } from '$lib/tests/stream';

const NOW = Date.UTC(2026, 9, 6, 12) * 1000;

const PAIRED: SensingNode[] = [
  { node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00' },
  { node_id: 'rx-1', role: 'rx', address: '192.168.4.51' },
  { node_id: 'rx-2', role: 'rx', address: '192.168.4.52' },
];

/** Both receivers streaming and an estimate of 3.2 minutes, `ago` old. */
function reported(ago: number, changes: Partial<LiveSnapshot> = {}): LiveSnapshot {
  return {
    estimate: {
      ts_us: NOW - ago,
      wait_minutes: 3.2,
      people: 4,
      level: 1.4,
      class: 'low',
      confidence: 0.8,
      reliable: true,
    },
    stream: {
      running: true,
      estimating: true,
      frames: 1000,
      estimates: 10,
      last_frame_us: NOW,
      edge_stamped: true,
      nodes: {
        'rx-1': { frames: 500, frames_per_second: 99, last_frame_us: NOW },
        'rx-2': { frames: 500, frames_per_second: 99, last_frame_us: NOW },
      },
    },
    service: { open: true },
    recording: false,
    now_us: NOW,
    ...changes,
  };
}

function show(snapshot: LiveSnapshot) {
  render(LivePanel, { props: { paired: PAIRED } });
  Stream.current.say(snapshot);
}

describe('LivePanel', () => {
  beforeEach(() => {
    Stream.opened = [];
    vi.stubGlobal('EventSource', Stream);
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(new Response('[]', { status: 200 }))),
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  it('shows the waiting time while it is current', async () => {
    show(reported(0));

    expect(await screen.findByText('3.2')).toBeInTheDocument();
    expect(screen.queryByText(/interrupted/i)).not.toBeInTheDocument();
  });

  it('stops presenting an estimate the appliance merely still holds', async () => {
    const quiet = reported(SILENT_AFTER_US + 1);
    quiet.stream.nodes['rx-2'].last_frame_us = NOW - SILENT_AFTER_US - 1;
    show(quiet);

    expect(await screen.findByText(/estimation interrupted/i)).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Not answering: rx-2.');
    expect(screen.getByText(/last estimate: 3\.2 min/i)).toBeInTheDocument();
    expect(screen.queryByText('3.2')).not.toBeInTheDocument();
  });

  it('says so when the sensors answer and nothing comes out', async () => {
    show(reported(SILENT_AFTER_US + 1));

    expect(await screen.findByRole('status')).toHaveTextContent(/no estimate is coming out/i);
  });

  it('says that a recording suspends the estimate', async () => {
    show(reported(0, { recording: true }));

    expect(await screen.findByText('Recording')).toBeInTheDocument();
    expect(screen.getByText(/estimate is suspended/i)).toBeInTheDocument();
    expect(screen.queryByText('3.2')).not.toBeInTheDocument();
  });
});
