import { describe, expect, it } from 'vitest';

import { interpretLogin } from './api';

describe('interpretLogin', () => {
  it('reads a granted session', () => {
    expect(interpretLogin(204, null)).toEqual({ kind: 'ok' });
  });

  it('reads a refused secret', () => {
    expect(interpretLogin(401, null)).toEqual({ kind: 'invalid' });
  });

  it('reads the wait imposed by the throttle', () => {
    expect(interpretLogin(429, '4')).toEqual({ kind: 'throttled', seconds: 4 });
  });

  it('still reports a wait when the header is missing or nonsense', () => {
    // The screen must always have something to display; a throttled attempt
    // without a usable delay still has to tell the operator to wait.
    expect(interpretLogin(429, null)).toEqual({ kind: 'throttled', seconds: 1 });
    expect(interpretLogin(429, 'soon')).toEqual({ kind: 'throttled', seconds: 1 });
    expect(interpretLogin(429, '-3')).toEqual({ kind: 'throttled', seconds: 1 });
  });

  it('treats anything else as a transport failure', () => {
    expect(interpretLogin(500, null)).toEqual({ kind: 'error' });
    expect(interpretLogin(404, null)).toEqual({ kind: 'error' });
  });
});
