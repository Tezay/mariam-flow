// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import ModelsSection from './ModelsSection.svelte';
import { type StoredModel } from '$lib/api/models';

function model(id: string, name: string, active = false): StoredModel {
  return {
    id,
    manifest: { name, trained_at: '2026-06-24', sessions: 3 },
    imported_at_us: Date.UTC(2026, 7, 1, 10) * 1000,
    window_us: 5_000_000,
    receivers: 2,
    active,
    has_evaluation: true,
  };
}

const BASE = { onupdated() {}, onchanged() {}, onopen() {} };

describe('ModelsSection', () => {
  it('lists the models that are kept, not the one in service', () => {
    render(ModelsSection, {
      props: { ...BASE, models: [model('b', 'June'), model('a', 'September', true)] },
    });

    expect(screen.getByText('June')).toBeInTheDocument();
    expect(screen.queryByText('September')).not.toBeInTheDocument();
    expect(screen.getAllByRole('button', { name: /^use$/i })).toHaveLength(1);
  });

  it('says so when no other model is kept', () => {
    render(ModelsSection, { props: { ...BASE, models: [model('a', 'September', true)] } });

    expect(screen.getByText(/no other model is kept/i)).toBeInTheDocument();
  });

  it('names a bundle that never said what it was', () => {
    render(ModelsSection, {
      props: {
        ...BASE,
        models: [model('a', 'September', true), { ...model('b', ''), manifest: undefined }],
      },
    });

    expect(screen.getByText(/unnamed bundle/i)).toBeInTheDocument();
  });
});
