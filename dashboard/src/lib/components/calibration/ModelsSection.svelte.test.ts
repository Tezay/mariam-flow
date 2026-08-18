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

const BASE = { onupdated() {}, onchanged() {}, onopen() {}, oncompare() {} };

describe('ModelsSection', () => {
  it('heads the list with the model in service', () => {
    render(ModelsSection, {
      props: { ...BASE, models: [model('b', 'June'), model('a', 'September', true)] },
    });

    expect(screen.getByRole('heading', { name: 'September' })).toBeInTheDocument();
    // The one in service is not offered as something to switch to.
    expect(screen.getAllByRole('button', { name: /^use$/i })).toHaveLength(1);
  });

  it('says plainly when nothing is estimating', () => {
    render(ModelsSection, { props: { ...BASE, models: [] } });

    expect(screen.getByText(/no model yet/i)).toBeInTheDocument();
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

  it('cannot import until a bundle is chosen', () => {
    render(ModelsSection, { props: { ...BASE, models: [] } });

    expect(screen.getByRole('button', { name: /import and activate/i })).toBeDisabled();
  });
});
