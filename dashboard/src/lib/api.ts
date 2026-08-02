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
  | { mode: 'idle' }
  | { mode: 'calibrating'; session_id: string; started_us: number }
  | { mode: 'live' };

export type Readiness = {
  site_named: boolean;
  nodes_paired: boolean;
  uplink_decided: boolean;
  /** A calibration session has been recorded — what finishes an installation. */
  site_captured: boolean;
  /** Reported, but not required to finish installing: the model is trained
   *  off site and imported days later. */
  model_ready: boolean;
};

export type SensingNode = {
  node_id: string;
  role: 'tx' | 'rx';
  /** Known from a DHCP lease; a receiver may be paired without one. */
  mac?: string;
  address?: string;
  /** Where it physically sits, in the operator's own words. */
  position?: string;
};

/** What joining the site's network asks of a device. */
export type SiteAuthentication =
  'nothing' | 'shared-password' | 'account' | 'certificate' | 'sign-in-page' | 'unknown';

/** What each density class means at this site. */
export type ClassMapping = {
  empty: string;
  low: string;
  medium: string;
  saturated: string;
};

/** One capture recorded at this site. */
export type RecordedSession = {
  session_id: string;
  environment: string;
  bytes: number;
  /** Read back from the identifier; absent for a directory not named by the appliance. */
  recorded_at_us?: number;
  /** An unsealed capture never finished and cannot be trained on. */
  sealed: boolean;
};

/** What the operator supplies when starting a capture. */
export type SessionRequest = {
  environment: string;
  positions: Record<string, string>;
};

/** What the machine the appliance runs on says about itself. */
export type SystemReport = {
  model?: string;
  os?: string;
  kernel?: string;
  uptime_s?: number;
  load_1m?: number;
  memory_total_kb?: number;
  memory_available_kb?: number;
  temperature_c?: number;
};

/** What the installer found out about the site's network. */
export type NetworkSurvey = {
  authentication: SiteAuthentication;
  /** Devices must be declared before they are allowed on. */
  registration_required: boolean;
  /** The site hands out a fixed address rather than using DHCP. */
  fixed_address: boolean;
};

export type Uplink = {
  mode: 'undecided' | 'offline' | 'wifi' | 'ethernet';
  ssid?: string;
};

/** The days of the week, in the order they are stored and shown. */
export const WEEKDAYS = [
  'monday',
  'tuesday',
  'wednesday',
  'thursday',
  'friday',
  'saturday',
  'sunday',
] as const;

export type Weekday = (typeof WEEKDAYS)[number];

/** One serving interval, `HH:MM`, with `to` exclusive. */
export type Interval = { from: string; to: string };

/** A closed period, `YYYY-MM-DD`, both ends inclusive. */
export type Closure = { from: string; to: string };

/** When a site serves. */
export type ServiceWindow = {
  timezone: string;
  weekly: Record<Weekday, Interval[]>;
  closures: Closure[];
};

/**
 * Whether the site is serving, and when that next changes.
 *
 * `changes_at_us` is absent when no change falls within the appliance's
 * horizon — a schedule that never reopens, for instance.
 */
export type ServiceState = { open: boolean; changes_at_us?: number };

export type Status = {
  kit_id: string;
  site_name: string | null;
  phase: Phase;
  readiness: Readiness;
  runtime: RuntimeMode;
  model_installed: boolean;
  service: ServiceState;
  sensor_ap: { ssid: string; channel: number };
  uplink: Uplink;
  survey?: NetworkSurvey;
  classes?: ClassMapping;
  /** Which stored model is estimating, if any. */
  active_model?: string;
  nodes: SensingNode[];
};

/** A sender streaming to the appliance that no node mapping claims. */
export type Candidate = {
  address: string;
  datagrams: number;
  datagrams_per_second: number;
  tx_macs: string[];
  first_seen_us: number;
  last_seen_us: number;
};

/** The pairing the appliance offers, for the installer to confirm. */
export type Proposal = {
  tx_mac?: string;
  /** How many receivers reported that transmitter. */
  tx_agreement: number;
  receivers: { node_id: string; address: string }[];
};

export type Discovery = { candidates: Candidate[]; proposal: Proposal };

/** How a write to the appliance ended. */
export type WriteOutcome =
  { kind: 'ok'; status: Status } | { kind: 'refused'; message: string } | { kind: 'error' };

/**
 * Sends a configuration change and reports what came back.
 *
 * A refusal carries the appliance's own words: it is the authority on what it
 * will store, and rewording its answer here would be inventing a second
 * opinion.
 */
async function put(path: string, body: unknown): Promise<WriteOutcome> {
  return send('PUT', path, body);
}

async function send(method: string, path: string, body: unknown): Promise<WriteOutcome> {
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

/** Reads what the machine says about itself. */
export async function fetchSystem(): Promise<SystemReport | null> {
  try {
    const response = await fetch('/api/system');
    return response.ok ? ((await response.json()) as SystemReport) : null;
  } catch {
    return null;
  }
}

/** Reads what is streaming unpaired, with the pairing offered for it. */
export async function fetchDiscovery(): Promise<Discovery | null> {
  try {
    const response = await fetch('/api/discovery');
    return response.ok ? ((await response.json()) as Discovery) : null;
  } catch {
    return null;
  }
}

/** Names the site this appliance is installed at. */
export function saveSite(siteName: string): Promise<WriteOutcome> {
  return put('/api/site', { site_name: siteName });
}

/** Replaces the paired nodes with the set the installer confirmed. */
export function saveNodes(nodes: SensingNode[]): Promise<WriteOutcome> {
  return put('/api/nodes', nodes);
}

/** Records what the site's network was found to ask for. */
export function saveNetworkSurvey(survey: NetworkSurvey | null): Promise<WriteOutcome> {
  return put('/api/network-survey', survey);
}

/** Records how the appliance reaches the site network, or that it will not. */
export function saveUplink(uplink: unknown): Promise<WriteOutcome> {
  return put('/api/uplink', uplink);
}

/**
 * Takes a trained model bundle into service.
 *
 * Sent as raw bytes rather than a form: the appliance reads one archive, and
 * a multipart envelope would add a parser for nothing.
 */
export async function importModel(file: File): Promise<WriteOutcome> {
  try {
    const response = await fetch('/api/model', {
      method: 'POST',
      headers: { 'content-type': 'application/gzip' },
      body: file,
    });
    if (response.ok) {
      return { kind: 'ok', status: (await response.json()) as Status };
    }
    const failure = (await response.json().catch(() => null)) as { error?: string } | null;
    return { kind: 'refused', message: failure?.error ?? '' };
  } catch {
    return { kind: 'error' };
  }
}

/** What a model says about itself. */
export type ModelManifest = { name: string; trained_at: string; sessions: number };

/** One model the appliance holds. */
export type StoredModel = {
  id: string;
  manifest?: ModelManifest;
  imported_at_us: number;
  window_us: number;
  receivers: number;
  active: boolean;
};

/** Lists every model the appliance holds, newest first. */
export async function fetchModels(): Promise<StoredModel[]> {
  try {
    const response = await fetch('/api/models');
    return response.ok ? ((await response.json()) as StoredModel[]) : [];
  } catch {
    return [];
  }
}

/** Puts one of the stored models back into service. */
export async function useModel(id: string): Promise<WriteOutcome> {
  return send('POST', `/api/models/${encodeURIComponent(id)}`, undefined);
}

/** Removes a stored model. */
export async function forgetModel(id: string): Promise<boolean> {
  try {
    const response = await fetch(`/api/models/${encodeURIComponent(id)}`, { method: 'DELETE' });
    return response.ok;
  } catch {
    return false;
  }
}

/** Records where a node physically sits, or that it is unknown again. */
export async function describeNode(nodeId: string, position: string): Promise<WriteOutcome> {
  return send('PATCH', `/api/nodes/${encodeURIComponent(nodeId)}`, { position });
}

/** Points an existing node at the hardware that replaced it. */
export async function adoptHardware(
  nodeId: string,
  hardware: { address?: string; mac?: string },
): Promise<WriteOutcome> {
  return send('POST', `/api/nodes/${encodeURIComponent(nodeId)}/hardware`, hardware);
}

/** Gives a stored model a new name. */
export async function renameModel(id: string, name: string): Promise<boolean> {
  return rename(`/api/models/${encodeURIComponent(id)}`, name);
}

/** Lists the captures recorded at this site, newest first. */
export async function fetchSessions(): Promise<RecordedSession[]> {
  try {
    const response = await fetch('/api/sessions');
    return response.ok ? ((await response.json()) as RecordedSession[]) : [];
  } catch {
    return [];
  }
}

/** Where a capture is downloaded from, for training elsewhere. */
export function archiveUrl(sessionId: string): string {
  return `/api/sessions/${encodeURIComponent(sessionId)}/archive`;
}

/** Removes a recorded capture. */
export async function deleteSession(sessionId: string): Promise<boolean> {
  try {
    const response = await fetch(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: 'DELETE',
    });
    return response.ok;
  } catch {
    return false;
  }
}

/** Records what each density class means at this site. */
export function saveClasses(classes: ClassMapping | null): Promise<WriteOutcome> {
  return put('/api/classes', classes);
}

/** Starts recording a labeled capture. */
export async function startCalibration(request: SessionRequest): Promise<WriteOutcome> {
  return send('POST', '/api/calibration', request);
}

/** Ends the capture and seals its directory. */
export async function stopCalibration(): Promise<WriteOutcome> {
  return send('DELETE', '/api/calibration', undefined);
}

/**
 * Marks what is being observed right now.
 *
 * A label is a state marker, not a tally: training reads the latest label at
 * or before each window, so one press stands until the next.
 */
export async function addLabel(density: number): Promise<boolean> {
  try {
    const response = await fetch('/api/calibration/label', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ class: density }),
    });
    return response.ok;
  } catch {
    return false;
  }
}

/** Closes the installation, or reopens it. */
export function setInstallation(completed: boolean): Promise<WriteOutcome> {
  return put('/api/installation', { completed });
}

/** Density classes, in their canonical order. */
export const DENSITY_CLASSES = ['empty', 'low', 'medium', 'saturated'] as const;

export type DensityClass = (typeof DENSITY_CLASSES)[number];

/** One receiver's contribution to the stream. */
export type NodeHealth = {
  frames: number;
  frames_per_second: number;
  last_frame_us?: number;
};

/** How the frames are arriving. */
export type StreamHealth = {
  running: boolean;
  /** Separate from `running`: an appliance can read without estimating. */
  estimating: boolean;
  frames: number;
  estimates: number;
  last_frame_us?: number;
  nodes: Record<string, NodeHealth>;
};

/** The current estimate, as the administration surface reports it. */
export type Estimate = {
  ts_us: number;
  wait_minutes: number;
  people: number;
  level: number;
  class: DensityClass;
  confidence: number;
  reliable: boolean;
};

/** What the live stream sends on every tick. */
export type LiveSnapshot = {
  estimate?: Estimate;
  stream: StreamHealth;
  service: ServiceState;
  now_us: number;
};

/** One folded minute of history. */
export type MinuteSummary = {
  minute_us: number;
  samples: number;
  reliable_samples: number;
  wait_minutes: number;
  level: number;
  /** Encoded as its integer value on the wire, as everywhere else. */
  class: number;
  confidence: number;
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

/** Reads the folded minutes of the last `minutes` minutes, oldest first. */
export async function fetchHistory(minutes: number): Promise<MinuteSummary[]> {
  try {
    const response = await fetch(`/api/estimates?minutes=${minutes}`);
    if (!response.ok) {
      return [];
    }
    return (await response.json()) as MinuteSummary[];
  } catch {
    return [];
  }
}

/** Reads the stored service schedule, or `null` when none is declared. */
export async function fetchServiceWindow(): Promise<ServiceWindow | null | 'unreachable'> {
  try {
    const response = await fetch('/api/service-window');
    if (!response.ok) {
      return 'unreachable';
    }
    return (await response.json()) as ServiceWindow | null;
  } catch {
    return 'unreachable';
  }
}

/**
 * How a save ended.
 *
 * `refused` carries the appliance's own words. The schedule is validated in
 * the browser before it is sent, so reaching this means the two disagree —
 * and the appliance is the one that decides. Rewording its answer here would
 * be inventing a second opinion.
 */
export type SaveOutcome = { kind: 'ok' } | { kind: 'refused'; message: string } | { kind: 'error' };

/** Replaces the stored schedule, or clears it when given `null`. */
export async function saveServiceWindow(window: ServiceWindow | null): Promise<SaveOutcome> {
  try {
    const response = await fetch('/api/service-window', {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(window),
    });
    if (response.ok) {
      return { kind: 'ok' };
    }
    if (response.status === 400) {
      const body = (await response.json().catch(() => null)) as { error?: string } | null;
      return { kind: 'refused', message: body?.error ?? '' };
    }
    return { kind: 'error' };
  } catch {
    return { kind: 'error' };
  }
}

/**
 * Subscribes to the live stream, returning a function that closes it.
 *
 * `EventSource` reconnects on its own after a dropped connection, which is
 * the behaviour that matters on a phone carried across a service hall — so
 * there is no retry logic here, only a way to stop listening.
 */
export function subscribeLive(
  onSnapshot: (snapshot: LiveSnapshot) => void,
  onError?: () => void,
): () => void {
  const source = new EventSource('/api/live');
  source.onmessage = (event) => {
    try {
      onSnapshot(JSON.parse(event.data) as LiveSnapshot);
    } catch {
      // A malformed frame is not worth tearing the stream down for; the
      // next one arrives in a second.
    }
  };
  source.onerror = () => onError?.();
  return () => source.close();
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

/** Gives a recorded capture a new description. */
export async function renameSession(sessionId: string, name: string): Promise<boolean> {
  return rename(`/api/sessions/${encodeURIComponent(sessionId)}`, name);
}

async function rename(path: string, name: string): Promise<boolean> {
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
