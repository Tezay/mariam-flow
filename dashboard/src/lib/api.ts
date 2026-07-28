/**
 * The appliance API, as the dashboard sees it.
 *
 * Every type here mirrors what `flow-edge` serializes; keeping them in one
 * place means a change on the Rust side surfaces as a type error in the
 * screens rather than as `undefined` at runtime.
 */

export type Stage = 'site' | 'nodes' | 'network' | 'calibration' | 'complete';

export type Phase = { phase: 'onboarding'; stage: Stage } | { phase: 'operational' };

export type RuntimeMode =
  { mode: 'idle' } | { mode: 'calibrating'; session_id: string } | { mode: 'live' };

export type Readiness = {
  site_named: boolean;
  nodes_paired: boolean;
  uplink_decided: boolean;
  model_ready: boolean;
};

export type SensingNode = {
  node_id: string;
  role: 'tx' | 'rx';
  mac: string;
  address?: string;
};

export type Uplink = {
  mode: 'undecided' | 'offline' | 'wifi' | 'ethernet';
  ssid?: string;
};

export type Status = {
  kit_id: string;
  site_name: string | null;
  phase: Phase;
  readiness: Readiness;
  runtime: RuntimeMode;
  model_installed: boolean;
  sensor_ap: { ssid: string; channel: number };
  uplink: Uplink;
  nodes: SensingNode[];
};

/** How a sign-in attempt ended. */
export type LoginOutcome =
  { kind: 'ok' } | { kind: 'invalid' } | { kind: 'throttled'; seconds: number } | { kind: 'error' };

/**
 * Turns a login response into an outcome the screen can act on.
 *
 * Split out from the request so the mapping — which is the part with rules
 * in it — can be tested without a server.
 */
export function interpretLogin(status: number, retryAfter: string | null): LoginOutcome {
  if (status === 204) {
    return { kind: 'ok' };
  }
  if (status === 429) {
    const seconds = Number.parseInt(retryAfter ?? '', 10);
    return { kind: 'throttled', seconds: Number.isFinite(seconds) && seconds > 0 ? seconds : 1 };
  }
  if (status === 401) {
    return { kind: 'invalid' };
  }
  return { kind: 'error' };
}

/** Exchanges the device secret for a session cookie. */
export async function login(secret: string): Promise<LoginOutcome> {
  try {
    const response = await fetch('/api/session', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ secret }),
    });
    return interpretLogin(response.status, response.headers.get('retry-after'));
  } catch {
    return { kind: 'error' };
  }
}

/** Ends the session held by this browser. */
export async function logout(): Promise<void> {
  try {
    await fetch('/api/session', { method: 'DELETE' });
  } catch {
    // The cookie is gone from the server's point of view either way; the
    // caller returns to the sign-in screen regardless.
  }
}

/** Reads the appliance status, or reports that no session is held. */
export async function fetchStatus(): Promise<Status | 'unauthorized' | 'unreachable'> {
  try {
    const response = await fetch('/api/status');
    if (response.status === 401) {
      return 'unauthorized';
    }
    if (!response.ok) {
      return 'unreachable';
    }
    return (await response.json()) as Status;
  } catch {
    return 'unreachable';
  }
}

/**
 * Reads a secret handed over by the label's QR code and clears it.
 *
 * The QR encodes the secret in the URL *fragment*, which browsers never
 * send to a server (ADR 0011). It is read here, used once, and wiped from
 * the address bar so it does not survive in history or in a screenshot.
 */
export function takeSecretFromFragment(): string | null {
  const hash = window.location.hash;
  if (!hash.startsWith('#')) {
    return null;
  }
  const secret = new URLSearchParams(hash.slice(1)).get('s');
  if (secret) {
    history.replaceState(null, '', window.location.pathname + window.location.search);
  }
  return secret;
}
