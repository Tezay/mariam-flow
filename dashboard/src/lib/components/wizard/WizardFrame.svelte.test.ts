// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import WizardFrame from './WizardFrame.svelte';
import { type SensingNode, type Stage, type Status } from '$lib/api/status';

const NODES: SensingNode[] = [
  { node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00' },
  { node_id: 'rx-1', role: 'rx', address: '192.168.4.51' },
];

function status(stage: Stage): Status {
  return {
    kit_id: 'KIT-0042',
    site_name: 'RU',
    phase: { phase: 'onboarding', stage },
    readiness: {
      site_named: true,
      nodes_paired: true,
      uplink_decided: true,
      site_captured: false,
      model_ready: false,
    },
    runtime: { mode: 'idle' },
    model_installed: false,
    service: { open: true },
    sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
    uplink: { mode: 'offline' },
    nodes: NODES,
  } as Status;
}

function stubTransport() {
  vi.stubGlobal(
    'EventSource',
    class {
      set onmessage(_: unknown) {}
      set onerror(_: unknown) {}
      close() {}
    },
  );
  vi.stubGlobal(
    'fetch',
    vi.fn(() => Promise.resolve(new Response('null', { status: 200 }))),
  );
}

describe('WizardFrame', () => {
  afterEach(() => vi.unstubAllGlobals());

  // Named one by one rather than counted: the stepper's own buttons are on
  // screen at every stage, so a count would pass for a stage showing nothing.
  const ASKS: [Stage, RegExp][] = [
    ['site', /continue/i],
    ['nodes', /pair these sensors/i],
    ['network', /continue/i],
    ['calibration', /start recording/i],
    ['complete', /finish installation/i],
  ];

  it.each(ASKS)('gives the %s step something to act on', (stage, control) => {
    // A step with no branch renders an empty card, which reads as a screen
    // still loading rather than as a wizard that cannot be finished.
    stubTransport();
    render(WizardFrame, { props: { status: status(stage), onupdated() {} } });

    expect(screen.getByRole('button', { name: control })).toBeInTheDocument();
  });
});
