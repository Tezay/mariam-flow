// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import NameDialog from './NameDialog.svelte';

describe('NameDialog', () => {
  it('refuses a blank name where one is required', async () => {
    const onrename = vi.fn();
    render(NameDialog, {
      props: { title: 'Rename', initial: 'campaign', onrename, oncancel() {} },
    });

    await userEvent.clear(screen.getByRole('textbox'));

    expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
    expect(onrename).not.toHaveBeenCalled();
  });

  it('accepts a blank one where clearing is an answer', async () => {
    // A sensor whose position is cleared is saying "I do not know where it
    // is", which is different from declining to name a recording.
    const onrename = vi.fn();
    render(NameDialog, {
      props: {
        title: 'Where it sits',
        initial: 'by the door',
        allowEmpty: true,
        onrename,
        oncancel() {},
      },
    });

    await userEvent.clear(screen.getByRole('textbox'));
    await userEvent.click(screen.getByRole('button', { name: /save/i }));

    expect(onrename).toHaveBeenCalledWith('');
  });

  it('trims what it hands back', async () => {
    const onrename = vi.fn();
    render(NameDialog, { props: { title: 'Rename', initial: '', onrename, oncancel() {} } });

    await userEvent.type(screen.getByRole('textbox'), '  midday service  ');
    await userEvent.click(screen.getByRole('button', { name: /save/i }));

    expect(onrename).toHaveBeenCalledWith('midday service');
  });
});
