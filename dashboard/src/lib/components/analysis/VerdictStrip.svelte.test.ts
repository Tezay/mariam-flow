// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import VerdictStrip from './VerdictStrip.svelte';

describe('VerdictStrip', () => {
  it('states each reading in words as well as a figure', () => {
    render(VerdictStrip, {
      props: {
        rows: [
          { key: 'presence', reading: { value: 0.98, verdict: 'good' } },
          { key: 'exact', reading: { value: 0.51, verdict: 'fair' } },
        ],
      },
    });

    expect(screen.getByText(/tells an empty zone/i)).toBeInTheDocument();
    expect(screen.getByText('98 %')).toBeInTheDocument();
    expect(screen.getByText('51 %')).toBeInTheDocument();
  });

  it('carries the verdict in text, so colour is never the only signal', () => {
    render(VerdictStrip, {
      props: {
        rows: [{ key: 'ordering', reading: { value: 0.4, verdict: 'poor' } }],
      },
    });

    expect(screen.getByText('poor')).toBeInTheDocument();
  });
});
