// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import Page from './+page.svelte';
import { type Status } from '$lib/api/status';
import { LOST_AFTER_MS } from '$lib/connection.svelte';
import { SNAPSHOT, Stream } from '$lib/tests/stream';

// Loaded by a chart several screens down, and it queries the display the
// moment it is imported, which jsdom has no answer for.
vi.mock('uplot', () => ({ default: class {} }));

const STATUS = {
  kit_id: 'KIT-0042',
  site_name: null,
  phase: { phase: 'onboarding', stage: 'site' },
  readiness: {},
  runtime: { mode: 'idle' },
  model_installed: false,
  service: { open: true },
  sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
  uplink: { mode: 'undecided' },
  nodes: [],
} as unknown as Status;

/** Answers the status with each of `codes` in turn, then keeps to the last. */
function appliance(...codes: number[]) {
  let asked = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(() => {
      const code = codes[Math.min(asked++, codes.length - 1)];
      return Promise.resolve(new Response(JSON.stringify(STATUS), { status: code }));
    }),
  );
}

const pass = (ms: number) => vi.advanceTimersByTimeAsync(ms);
/** Read as a property: jsdom keeps `inert` there and reflects no attribute. */
function outOfReach(): boolean {
  let element: HTMLElement | null = screen.getByRole('heading', { level: 1 });
  while (element && !element.inert) {
    element = element.parentElement;
  }
  return element !== null;
}

describe('the page', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    Stream.opened = [];
    vi.stubGlobal('EventSource', Stream);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('says when the appliance stops answering, and keeps what it showed out of reach', async () => {
    appliance(200);
    render(Page);
    await pass(0);
    Stream.current.say(SNAPSHOT);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();

    await pass(LOST_AFTER_MS + 1_000);

    expect(screen.getByRole('alert')).toHaveTextContent(/stopped responding/i);
    expect(outOfReach()).toBe(true);
  });

  it('carries on by itself once the appliance speaks again', async () => {
    appliance(200);
    render(Page);
    await pass(0);
    await pass(LOST_AFTER_MS + 1_000);
    expect(screen.getByRole('alert')).toBeInTheDocument();

    Stream.current.say(SNAPSHOT);
    await pass(0);

    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(outOfReach()).toBe(false);
  });

  it('asks to sign in again when the appliance came back without the session', async () => {
    appliance(200, 401);
    render(Page);
    await pass(0);

    await pass(LOST_AFTER_MS + 1_000);

    expect(screen.getByRole('button', { name: /sign in/i })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('keeps asking for an appliance that was not there at first', async () => {
    appliance(503, 200);
    render(Page);
    await pass(0);
    expect(screen.getByText(/not responding/i)).toBeInTheDocument();

    await pass(LOST_AFTER_MS);

    expect(screen.getByRole('heading', { level: 1 })).toBeInTheDocument();
  });
});
