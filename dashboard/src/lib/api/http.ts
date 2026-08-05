/** Sending to the appliance, and reading what it answers. */

import type { Status } from './status';

/** How a write to the appliance ended. */
export type WriteOutcome =
  { kind: 'ok'; status: Status } | { kind: 'refused'; message: string } | { kind: 'error' };

/**
 * How a save ended.
 *
 * `refused` carries the appliance's own words. The schedule is validated in
 * the browser before it is sent, so reaching this means the two disagree —
 * and the appliance is the one that decides. Rewording its answer here would
 * be inventing a second opinion.
 */
export type SaveOutcome = { kind: 'ok' } | { kind: 'refused'; message: string } | { kind: 'error' };

/**
 * Sends a configuration change and reports what came back.
 *
 * A refusal carries the appliance's own words: it is the authority on what it
 * will store, and rewording its answer here would be inventing a second
 * opinion.
 */
export async function put(path: string, body: unknown): Promise<WriteOutcome> {
  return send('PUT', path, body);
}

export async function send(method: string, path: string, body: unknown): Promise<WriteOutcome> {
  try {
    const response = await fetch(path, {
      method,
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (response.ok) {
      return { kind: 'ok', status: (await response.json()) as Status };
    }
    if (response.status === 400 || response.status === 409) {
      const failure = (await response.json().catch(() => null)) as { error?: string } | null;
      return { kind: 'refused', message: failure?.error ?? '' };
    }
    return { kind: 'error' };
  } catch {
    return { kind: 'error' };
  }
}

export async function rename(path: string, name: string): Promise<boolean> {
  try {
    const response = await fetch(path, {
      method: 'PATCH',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name }),
    });
    return response.ok;
  } catch {
    return false;
  }
}
