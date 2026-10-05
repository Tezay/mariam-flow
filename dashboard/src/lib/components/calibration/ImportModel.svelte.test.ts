// @vitest-environment jsdom
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import ImportModel from './ImportModel.svelte';

const opener = () => screen.getByRole('button', { name: 'Import a model' });

describe('ImportModel', () => {
  it('asks for nothing until it is opened', () => {
    render(ImportModel, { props: { onimported() {} } });

    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('says what an import does before it does it', async () => {
    render(ImportModel, { props: { onimported() {} } });

    await userEvent.click(opener());

    expect(screen.getByRole('dialog')).toHaveTextContent(/goes into service at once/i);
  });

  it('cannot import until a bundle is chosen', async () => {
    render(ImportModel, { props: { onimported() {} } });

    await userEvent.click(opener());

    expect(screen.getByRole('button', { name: /import and put in service/i })).toBeDisabled();
  });
});
