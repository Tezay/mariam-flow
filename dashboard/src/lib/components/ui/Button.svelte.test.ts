// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import Button from './Button.svelte';
import { text } from '$lib/tests/harness';

describe('Button', () => {
  it('calls back when pressed', async () => {
    const onclick = vi.fn();
    render(Button, { props: { onclick, children: text('Save') } });

    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onclick).toHaveBeenCalledOnce();
  });

  it('emits nothing while disabled', async () => {
    const onclick = vi.fn();
    render(Button, { props: { onclick, disabled: true, children: text('Save') } });

    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    expect(onclick).not.toHaveBeenCalled();
  });
});
