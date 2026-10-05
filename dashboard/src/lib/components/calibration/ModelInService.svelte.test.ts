// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import ModelInService from './ModelInService.svelte';
import { type StoredModel } from '$lib/api/models';

function model(id: string, name: string): StoredModel {
  return {
    id,
    manifest: { name, trained_at: '2026-06-24', sessions: 3 },
    imported_at_us: Date.UTC(2026, 7, 1, 10) * 1000,
    window_us: 5_000_000,
    receivers: 2,
    active: true,
    has_evaluation: true,
  };
}

const BASE = { onopen() {}, oncompare() {}, onchanged() {} };

describe('ModelInService', () => {
  it('names the model the appliance is estimating with, and where it came from', () => {
    render(ModelInService, { props: { ...BASE, model: model('a', 'September') } });

    expect(screen.getByRole('heading', { name: 'September' })).toBeInTheDocument();
    expect(screen.getByText(/from 3 recording/i)).toBeInTheDocument();
  });

  it('says plainly when nothing is estimating', () => {
    render(ModelInService, { props: { ...BASE, model: null } });

    expect(screen.getByText(/no model yet/i)).toBeInTheDocument();
  });

  it('offers the comparison only against a model it has just replaced', async () => {
    const oncompare = vi.fn();
    const { unmount } = render(ModelInService, {
      props: { ...BASE, model: model('a', 'September') },
    });
    expect(screen.queryByRole('button', { name: /compare/i })).not.toBeInTheDocument();
    unmount();

    render(ModelInService, {
      props: { ...BASE, oncompare, model: model('a', 'September'), replaced: model('b', 'June') },
    });
    await userEvent.click(screen.getByRole('button', { name: /compare/i }));

    expect(oncompare).toHaveBeenCalledWith('b', 'a');
  });
});
