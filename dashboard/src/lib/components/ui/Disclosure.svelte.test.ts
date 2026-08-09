// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';

import Disclosure from './Disclosure.svelte';
import { text } from '$lib/tests/harness';

describe('Disclosure', () => {
  it('is closed unless a section is worth opening on arrival', () => {
    render(Disclosure, { props: { title: 'Recordings', children: text('a capture') } });

    expect(screen.getByRole('group')).not.toHaveAttribute('open');
  });

  it('opens on arrival when asked, so evidence is not hidden behind a summary', () => {
    render(Disclosure, {
      props: { title: 'Where the mistakes fall', open: true, children: text('the matrix') },
    });

    expect(screen.getByRole('group')).toHaveAttribute('open');
  });

  it('leaves the summary to carry focus rather than reimplementing a button', () => {
    render(Disclosure, { props: { title: 'Recordings', children: text('a capture') } });

    expect(screen.getByText('Recordings').closest('summary')).not.toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
  });
});
