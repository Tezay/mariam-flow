// @vitest-environment jsdom
import { fireEvent, render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import UplinkForm from './UplinkForm.svelte';

/** What the appliance answered, and what it was asked to store. */
function stubFetch() {
  const calls: { url: string; body: unknown }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string, init?: RequestInit) => {
      calls.push({ url, body: JSON.parse(String(init?.body ?? 'null')) });
      return Promise.resolve(
        new Response(JSON.stringify({ nodes: [], uplink: { mode: 'offline' } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      );
    }),
  );
  return calls;
}

const BASE = {
  initialMode: 'offline' as const,
  initialSsid: null,
  submitLabel: 'Save',
  onupdated: () => {},
};

describe('UplinkForm', () => {
  let calls: { url: string; body: unknown }[];
  beforeEach(() => (calls = stubFetch()));
  afterEach(() => vi.unstubAllGlobals());

  it('never stores something other than the choice on screen', async () => {
    // Submitted directly rather than through the button: an empty SSID
    // disables it too, so a click would pass whatever the guard does.
    const { container } = render(UplinkForm, {
      props: { ...BASE, verdict: { kind: 'needs-administrator', reason: 'account' } },
    });
    await userEvent.click(screen.getByRole('radio', { name: /wi-fi/i }));

    await fireEvent.submit(container.querySelector('form')!);

    for (const call of calls) {
      expect(call.body).not.toMatchObject({ mode: 'offline' });
    }
  });

  it('keeps the guard on the button, so the refusal is visible', async () => {
    render(UplinkForm, {
      props: { ...BASE, verdict: { kind: 'joinable', needsPassphrase: true } },
    });
    await userEvent.click(screen.getByRole('radio', { name: /wi-fi/i }));
    await userEvent.type(screen.getByLabelText(/network name/i), 'efrei-guest');
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();

    await userEvent.click(screen.getByRole('radio', { name: /ethernet/i }));
    await userEvent.click(screen.getByRole('radio', { name: /wi-fi/i }));
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();
  });

  it('says why it will not save, rather than refusing in silence', async () => {
    render(UplinkForm, {
      props: { ...BASE, verdict: { kind: 'unanswered' } },
    });

    await userEvent.click(screen.getByRole('radio', { name: /wi-fi/i }));

    expect(screen.getByText(/answer the questions/i)).toBeInTheDocument();
  });

  it('stores what is on screen once the network can be joined', async () => {
    render(UplinkForm, {
      props: { ...BASE, verdict: { kind: 'joinable', needsPassphrase: true } },
    });

    await userEvent.click(screen.getByRole('radio', { name: /wi-fi/i }));
    await userEvent.type(screen.getByLabelText(/network name/i), 'efrei-guest');
    await userEvent.type(screen.getByLabelText(/passphrase/i), 'correct-horse');
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(calls).toHaveLength(1);
    expect(calls[0].body).toMatchObject({ mode: 'wifi', ssid: 'efrei-guest' });
  });
});
