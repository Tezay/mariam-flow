// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import QueueSection from './QueueSection.svelte';
import { type Status, type WaitTuning } from '$lib/api/status';

const DESCRIBED: WaitTuning = {
  people_per_class: [0, 4, 12, 25],
  service_rate_per_min: 6,
  smoothing_tau_s: 30,
  hysteresis_margin: 0.15,
  min_confidence: 0.5,
};

function status(wait?: WaitTuning): Status {
  return {
    kit_id: 'KIT-0042',
    site_name: 'RU',
    phase: { phase: 'onboarding', stage: 'queue' },
    readiness: {} as Status['readiness'],
    runtime: { mode: 'idle' },
    model_installed: false,
    service: { open: true },
    sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
    uplink: { mode: 'offline' },
    nodes: [],
    wait,
  } as Status;
}

const submit = (name: RegExp) => screen.getByRole('button', { name });

describe('QueueSection', () => {
  it('starts empty and refuses to save until every count is answered', async () => {
    // Nothing is defaulted: a head count nobody entered would produce waiting
    // times that look measured.
    render(QueueSection, { props: { status: status(), submitLabel: 'Save', onupdated() {} } });

    const counts = screen.getAllByRole('spinbutton');
    expect(counts.every((field) => (field as HTMLInputElement).value === '')).toBe(true);
    expect(submit(/save/i)).toBeDisabled();

    for (const [index, value] of ['0', '4', '12', '25', '6'].entries()) {
      await userEvent.type(counts[index], value);
    }
    expect(submit(/save/i)).toBeEnabled();
  });

  it('refuses a queue that is never served', async () => {
    render(QueueSection, {
      props: { status: status(DESCRIBED), submitLabel: 'Save', onupdated() {} },
    });
    expect(submit(/save/i)).toBeEnabled();

    // Dividing a head count by zero people per minute is not a long wait, it
    // is no answer at all — and the appliance refuses it too.
    const rate = screen.getAllByRole('spinbutton')[4];
    await userEvent.clear(rate);
    await userEvent.type(rate, '0');

    expect(submit(/save/i)).toBeDisabled();
  });

  it('keeps the display knobs out of the way unless asked for', () => {
    const { unmount } = render(QueueSection, {
      props: { status: status(DESCRIBED), submitLabel: 'Save', onupdated() {} },
    });
    expect(screen.queryByText(/advanced/i)).not.toBeInTheDocument();
    unmount();

    render(QueueSection, {
      props: { status: status(DESCRIBED), advanced: true, submitLabel: 'Save', onupdated() {} },
    });
    expect(screen.getByText(/advanced/i)).toBeInTheDocument();
  });
});
