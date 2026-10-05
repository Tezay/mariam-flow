// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import SystemSection from './SystemSection.svelte';
import { type Status } from '$lib/api/status';
import { SNAPSHOT, Stream } from '$lib/tests/stream';

const STATUS = {
  version: '0.1.0+44e2acdffd',
  runtime: { mode: 'idle' },
  model_installed: true,
  sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
} as Status;

describe('SystemSection', () => {
  beforeEach(() => {
    Stream.opened = [];
    vi.stubGlobal('EventSource', Stream);
    vi.stubGlobal(
      'fetch',
      vi.fn(() => Promise.resolve(new Response('{}', { status: 200 }))),
    );
  });
  afterEach(() => vi.unstubAllGlobals());

  it('reports storage and load as what is used of what the machine has', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(() =>
        Promise.resolve(
          new Response(
            JSON.stringify({
              load_1m: 0.2,
              cpus: 4,
              storage_total_kb: 30_500_000,
              storage_available_kb: 28_700_000,
            }),
            { status: 200 },
          ),
        ),
      ),
    );
    render(SystemSection, { props: { status: STATUS } });

    expect(await screen.findByText('1.7 GB / 29.1 GB (6%)')).toBeInTheDocument();
    expect(screen.getByText('0.20 / 4 cores (5%)')).toBeInTheDocument();
    expect(screen.getByText('0.1.0+44e2acdffd')).toBeInTheDocument();
  });

  it('reports what the appliance is doing now, not a mode read when the page loaded', async () => {
    render(SystemSection, { props: { status: STATUS } });

    Stream.current.say({ ...SNAPSHOT, recording: true });
    expect(await screen.findByText('Recording')).toBeInTheDocument();

    Stream.current.say({ ...SNAPSHOT, service: { open: false } });
    expect(await screen.findByText('Closed')).toBeInTheDocument();
  });
});
