// @vitest-environment jsdom
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import JournalSection from './JournalSection.svelte';
import { type RecordedEvent } from '$lib/api/journal';

function event(id: number, kind: RecordedEvent['kind'] = 'service-opened'): RecordedEvent {
  return { id, ts_us: Date.UTC(2026, 7, 2, 12) * 1000, category: 'lifecycle', kind };
}

/** Answers `/api/events`, and never opens a live stream. */
function stubApi(pages: RecordedEvent[][]) {
  let page = 0;
  vi.stubGlobal(
    'EventSource',
    class {
      close() {}
      set onmessage(_: unknown) {}
      set onerror(_: unknown) {}
    },
  );
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string) => {
      const rows = String(url).includes('after=')
        ? []
        : (pages[Math.min(page++, pages.length - 1)] ?? []);
      return Promise.resolve(
        new Response(JSON.stringify(rows), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      );
    }),
  );
}

describe('JournalSection', () => {
  beforeEach(() => stubApi([[event(9), event(8)]]));
  afterEach(() => vi.unstubAllGlobals());

  it('lists what the appliance recorded', async () => {
    render(JournalSection);

    await waitFor(() => expect(screen.getAllByText(/service opened/i).length).toBe(2));
  });

  it('offers a CSV export that carries the active filter', async () => {
    render(JournalSection);
    await waitFor(() => expect(screen.getAllByText(/service opened/i).length).toBe(2));

    await userEvent.click(screen.getByRole('button', { name: /^access$/i }));

    await waitFor(() =>
      expect(screen.getByRole('link', { name: /export/i })).toHaveAttribute(
        'href',
        '/api/events.csv?category=access',
      ),
    );
  });

  it('shows nothing but an empty state when the journal is empty', async () => {
    vi.unstubAllGlobals();
    stubApi([[]]);
    render(JournalSection);

    await waitFor(() => expect(screen.getByText(/nothing recorded yet/i)).toBeInTheDocument());
  });
});
