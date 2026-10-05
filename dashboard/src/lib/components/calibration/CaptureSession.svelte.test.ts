// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

import CaptureSession from './CaptureSession.svelte';
import { type SessionRequest } from '$lib/api/calibration';
import { type LiveSnapshot } from '$lib/api/live';
import { type SensingNode, type Status } from '$lib/api/status';

const NOW = Date.UTC(2026, 7, 2, 12) * 1000;

const NODES: SensingNode[] = [
  { node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00' },
  { node_id: 'rx-1', role: 'rx', address: '192.168.4.51' },
];

const PLACED: SensingNode[] = [
  { ...NODES[0], position: 'wall A, mid-zone' },
  { ...NODES[1], position: 'wall B, head of the queue' },
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

function stubStart(answer: Status): SessionRequest[] {
  const requests: SessionRequest[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn((_path: string, init: RequestInit) => {
      requests.push(JSON.parse(init.body as string) as SessionRequest);
      return Promise.resolve(new Response(JSON.stringify(answer), { status: 200 }));
    }),
  );
  return requests;
}

const started = () => screen.getByRole('button', { name: /start recording/i });
const position = (nodeId: string) => screen.queryByRole('textbox', { name: nodeId });

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

  it('shows where the sensors are instead of asking again', () => {
    stubLive(snapshot(true));
    render(CaptureSession, { props: { status: status({ nodes: PLACED }), onupdated() {} } });

    expect(screen.getByText('wall B, head of the queue')).toBeInTheDocument();
    expect(position('rx-1')).not.toBeInTheDocument();
  });

  it('asks where the sensors are while one is undescribed', () => {
    stubLive(snapshot(true));
    render(CaptureSession, {
      props: { status: status({ nodes: [PLACED[0], NODES[1]] }), onupdated() {} },
    });

    expect(position('tx-1')).toHaveValue('wall A, mid-zone');
    expect(position('rx-1')).toHaveValue('');
  });

  it('sends no position the operator did not edit', async () => {
    stubLive(snapshot(true));
    const requests = stubStart(status({ nodes: PLACED }));
    render(CaptureSession, { props: { status: status({ nodes: PLACED }), onupdated() {} } });

    await vi.waitFor(() => expect(started()).toBeEnabled());
    await userEvent.click(started());

    await vi.waitFor(() => expect(requests).toHaveLength(1));
    expect(requests[0].positions).toEqual({});
  });

  it('warns before a stored position is changed, then sends the edit', async () => {
    stubLive(snapshot(true));
    const requests = stubStart(status({ nodes: PLACED }));
    render(CaptureSession, { props: { status: status({ nodes: PLACED }), onupdated() {} } });
    await vi.waitFor(() => expect(started()).toBeEnabled());

    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    expect(screen.getByRole('note')).toHaveTextContent(/only if the sensor was moved/i);
    await userEvent.clear(position('rx-1')!);
    await userEvent.type(position('rx-1')!, 'wall B, tail of the queue');
    await userEvent.click(started());

    await vi.waitFor(() => expect(requests).toHaveLength(1));
    expect(requests[0].positions).toEqual({
      'tx-1': 'wall A, mid-zone',
      'rx-1': 'wall B, tail of the queue',
    });
  });

  it('gives up an edit without sending any of it', async () => {
    stubLive(snapshot(true));
    const requests = stubStart(status({ nodes: PLACED }));
    render(CaptureSession, { props: { status: status({ nodes: PLACED }), onupdated() {} } });
    await vi.waitFor(() => expect(started()).toBeEnabled());

    await userEvent.click(screen.getByRole('button', { name: 'Edit' }));
    await userEvent.clear(position('rx-1')!);
    await userEvent.click(screen.getByRole('button', { name: 'Keep the stored positions' }));
    expect(screen.getByText('wall B, head of the queue')).toBeInTheDocument();
    await userEvent.click(started());

    await vi.waitFor(() => expect(requests).toHaveLength(1));
    expect(requests[0].positions).toEqual({});
  });

  it('names a receiver that is not answering before anything is recorded', async () => {
    // Its measurements would be missing from the whole recording.
    const nodes: SensingNode[] = [...PLACED, { node_id: 'rx-2', role: 'rx', position: 'tail' }];
    stubLive(snapshot(true));
    render(CaptureSession, { props: { status: status({ nodes }), onupdated() {} } });

    await vi.waitFor(() => expect(screen.getByText(/not answering: rx-2/i)).toBeInTheDocument());
    expect(started()).toBeEnabled();
  });

  it('opens the form rather than recording when it is one action among others', async () => {
    stubLive(snapshot(true));
    const requests = stubStart(status({ nodes: PLACED }));
    render(CaptureSession, {
      props: { status: status({ nodes: PLACED }), launcher: true, onupdated() {} },
    });
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'New recording' }));

    expect(screen.getByRole('dialog')).toHaveTextContent(/estimate is suspended/i);
    expect(requests).toHaveLength(0);
  });

  it('closes the form once the recording has started', async () => {
    stubLive(snapshot(true));
    stubStart(status({ nodes: PLACED }));
    const onupdated = vi.fn();
    render(CaptureSession, {
      props: { status: status({ nodes: PLACED }), launcher: true, onupdated },
    });

    await userEvent.click(screen.getByRole('button', { name: 'New recording' }));
    await vi.waitFor(() => expect(started()).toBeEnabled());
    await userEvent.click(started());

    await vi.waitFor(() => expect(onupdated).toHaveBeenCalledOnce());
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
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
