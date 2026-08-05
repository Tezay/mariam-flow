/** Recording a labelled capture, and what becomes of it afterwards. */

import { put, rename, send, type WriteOutcome } from './http';
import type { ClassMapping } from './status';

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

/** Gives a recorded capture a new description. */
export async function renameSession(sessionId: string, name: string): Promise<boolean> {
  return rename(`/api/sessions/${encodeURIComponent(sessionId)}`, name);
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
