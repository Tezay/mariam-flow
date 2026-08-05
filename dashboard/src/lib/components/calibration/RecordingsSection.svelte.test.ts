// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import RecordingsSection from './RecordingsSection.svelte';
import { type RecordedSession } from '$lib/api/calibration';
import { type SensingNode } from '$lib/api/status';

const NODES: SensingNode[] = [{ node_id: 'rx-1', role: 'rx', address: '192.168.4.51' }];
const SESSION: RecordedSession = {
  session_id: 'kit-0042-20260802T120000Z',
  environment: 'Midday service',
  bytes: 1_500_000,
  recorded_at_us: Date.UTC(2026, 7, 2, 12) * 1000,
  sealed: true,
};

const BASE = {
  nodes: NODES,
  sessions: [] as RecordedSession[],
  ready: true,
  streaming: true,
  environment: 'Midday',
  positions: {},
  onupdated() {},
  onchanged() {},
};

describe('RecordingsSection', () => {
  it('refuses to start while nothing is arriving from the sensors', () => {
    // A capture started here records labels against no frames at all.
    render(RecordingsSection, { props: { ...BASE, streaming: false } });

    expect(screen.getByRole('button', { name: /start recording/i })).toBeDisabled();
    expect(screen.getByText(/nothing is arriving/i)).toBeInTheDocument();
  });

  it('refuses to start before the sensors are paired', () => {
    render(RecordingsSection, { props: { ...BASE, ready: false } });

    expect(screen.getByRole('button', { name: /start recording/i })).toBeDisabled();
  });

  it('refuses to start without a description', () => {
    render(RecordingsSection, { props: { ...BASE, environment: '   ' } });

    expect(screen.getByRole('button', { name: /start recording/i })).toBeDisabled();
  });

  it('offers a download only for a capture that was sealed', () => {
    render(RecordingsSection, {
      props: { ...BASE, sessions: [SESSION, { ...SESSION, session_id: 'other', sealed: false }] },
    });

    expect(screen.getAllByRole('link', { name: /download/i })).toHaveLength(1);
    expect(screen.getByText(/never finished/i)).toBeInTheDocument();
  });
});
