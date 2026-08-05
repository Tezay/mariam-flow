// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import RecordingsSection from './RecordingsSection.svelte';
import { type RecordedSession } from '$lib/api/calibration';

const SESSION: RecordedSession = {
  session_id: 'kit-0042-20260802T120000Z',
  environment: 'Midday service',
  bytes: 1_500_000,
  recorded_at_us: Date.UTC(2026, 7, 2, 12) * 1000,
  sealed: true,
};

describe('RecordingsSection', () => {
  it('offers a download only for a capture that was sealed', () => {
    render(RecordingsSection, {
      props: {
        sessions: [SESSION, { ...SESSION, session_id: 'other', sealed: false }],
        onchanged() {},
      },
    });

    expect(screen.getAllByRole('link', { name: /download/i })).toHaveLength(1);
    expect(screen.getByText(/never finished/i)).toBeInTheDocument();
  });
});
