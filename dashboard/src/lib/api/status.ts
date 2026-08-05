/** What the appliance says it is, and what the machine under it says. */

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
  /** Since when the journal has been unable to write, if it cannot. */
  journal_failure?: { since_us: number };
  nodes: SensingNode[];
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

/** Reads what the machine says about itself. */
export async function fetchSystem(): Promise<SystemReport | null> {
  try {
    const response = await fetch('/api/system');
    return response.ok ? ((await response.json()) as SystemReport) : null;
  } catch {
    return null;
  }
}
