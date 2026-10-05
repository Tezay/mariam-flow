// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import SystemSection from './SystemSection.svelte';
import { type Status } from '$lib/api/status';
import { SNAPSHOT, Stream } from '$lib/tests/stream';

const STATUS = {
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

  it('reports what the appliance is doing now, not a mode read when the page loaded', async () => {
    render(SystemSection, { props: { status: STATUS } });

    Stream.current.say({ ...SNAPSHOT, recording: true });
    expect(await screen.findByText('Recording')).toBeInTheDocument();

    Stream.current.say({ ...SNAPSHOT, service: { open: false } });
    expect(await screen.findByText('Closed')).toBeInTheDocument();
  });
});
