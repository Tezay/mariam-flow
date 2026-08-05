// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import CaptureSession from './CaptureSession.svelte';
import { type LiveSnapshot } from '$lib/api/live';
import { type SensingNode, type Status } from '$lib/api/status';

const NOW = Date.UTC(2026, 7, 2, 12) * 1000;

const NODES: SensingNode[] = [
  { node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00' },
  { node_id: 'rx-1', role: 'rx', address: '192.168.4.51' },
];

function status(overrides: Partial<Status> = {}): Status {
  return {
    kit_id: 'KIT-0042',
    site_name: 'RU',
    phase: { phase: 'onboarding', stage: 'calibration' },
    readiness: { nodes_paired: true } as Status['readiness'],
    runtime: { mode: 'idle' },
    model_installed: false,
    service: { open: true },
    sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
    uplink: { mode: 'offline' },
    nodes: NODES,
    ...overrides,
  } as Status;
}

function snapshot(running: boolean): LiveSnapshot {
  return {
    stream: {
      running,
      estimating: false,
      frames: 1000,
      estimates: 0,
      last_frame_us: NOW,
      edge_stamped: true,
      nodes: { 'rx-1': { frames: 1000, frames_per_second: 40, last_frame_us: NOW } },
    },
    service: { open: true },
    now_us: NOW,
  };
}

function stubLive(snap: LiveSnapshot) {
  vi.stubGlobal(
    'EventSource',
    class {
      onmessage: ((e: { data: string }) => void) | null = null;
      constructor() {
        queueMicrotask(() => this.onmessage?.({ data: JSON.stringify(snap) }));
      }
      close() {}
      set onerror(_: unknown) {}
    },
  );
}

const started = () => screen.getByRole('button', { name: /start recording/i });

describe('CaptureSession', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('refuses to start while nothing is arriving from the sensors', async () => {
    // A capture started here records labels against no frames at all.
    stubLive(snapshot(false));
    render(CaptureSession, { props: { status: status(), onupdated() {} } });

    await vi.waitFor(() => expect(screen.getByText(/nothing is arriving/i)).toBeInTheDocument());
    expect(started()).toBeDisabled();
  });

  it('refuses to start before the sensors are paired', () => {
    stubLive(snapshot(true));
    render(CaptureSession, {
      props: {
        status: status({ readiness: { nodes_paired: false } as Status['readiness'] }),
        onupdated() {},
      },
    });

    expect(started()).toBeDisabled();
  });

  it('refuses to start once the description is cleared', async () => {
    stubLive(snapshot(true));
    render(CaptureSession, { props: { status: status(), onupdated() {} } });

    // Enabled first, so clearing the field is what the second assertion catches
    // — every other guard also disables this button.
    await vi.waitFor(() => expect(started()).toBeEnabled());
    await userEvent.clear(screen.getByRole('textbox', { name: /what is being recorded/i }));
    expect(started()).toBeDisabled();
  });

  it('labels full frame instead of the form once the appliance is recording', () => {
    stubLive(snapshot(true));
    render(CaptureSession, {
      props: {
        status: status({ runtime: { mode: 'calibrating', session_id: 's', started_us: NOW } }),
        onupdated() {},
      },
    });

    expect(screen.queryByRole('button', { name: /start recording/i })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /stop/i })).toBeInTheDocument();
  });

  it('says nothing extra unless the caller supplies a lead', () => {
    stubLive(snapshot(true));
    const { unmount } = render(CaptureSession, {
      props: { status: status(), onupdated() {} },
    });
    expect(screen.queryByText(/mark the queue as it happens/i)).not.toBeInTheDocument();
    unmount();

    render(CaptureSession, {
      props: {
        status: status(),
        lead: 'You are about to mark the queue as it happens.',
        onupdated() {},
      },
    });
    expect(screen.getByText(/mark the queue as it happens/i)).toBeInTheDocument();
  });
});
