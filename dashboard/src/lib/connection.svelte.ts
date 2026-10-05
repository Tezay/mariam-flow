/**
 * Whether the appliance can still be heard, and how it is found again.
 *
 * Judged by the live stream going quiet rather than by the connection: a
 * socket whose other end vanished stays open, saying nothing.
 */

import { reopenLive, subscribeLive } from './api/live';
import { type Status, fetchStatus } from './api/status';

/** Silence after which the appliance counts as gone. */
export const LOST_AFTER_MS = 5_000;

/** How often a silent appliance is asked whether it is back. */
export const PROBE_EVERY_MS = 3_000;

let heardAt = $state<number | null>(null);
let lost = $state(false);

export function connectionLost(): boolean {
  return lost;
}

/** When the appliance last spoke, on this device's clock. */
export function lastHeardAt(): number | null {
  return heardAt;
}

/**
 * Watches the appliance until the returned function is called.
 *
 * `onunauthorized` is what a restart leaves: an appliance that answers again
 * without knowing the session.
 */
export function watchConnection(handlers: {
  onanswer: (status: Status) => void;
  onunauthorized: () => void;
}): () => void {
  let watching = true;
  let since = Date.now();
  let probedAt = 0;
  let probing = false;

  const close = subscribeLive(() => {
    heardAt = Date.now();
    lost = false;
  });

  async function probe() {
    probing = true;
    probedAt = Date.now();
    // Given up before the next one is due: a request to a machine that is
    // off waits on the network for minutes, long after the machine is back.
    const controller = new AbortController();
    const expiry = setTimeout(() => controller.abort(), PROBE_EVERY_MS);
    const status = await fetchStatus(controller.signal);
    clearTimeout(expiry);
    probing = false;
    if (!watching) {
      return;
    }
    if (status === 'unauthorized') {
      handlers.onunauthorized();
    } else if (status !== 'unreachable') {
      handlers.onanswer(status);
      reopenLive();
    }
  }

  const timer = setInterval(() => {
    const now = Date.now();
    if (now - Math.max(heardAt ?? 0, since) <= LOST_AFTER_MS) {
      return;
    }
    lost = true;
    if (!probing && now - probedAt >= PROBE_EVERY_MS) {
      void probe();
    }
  }, 1_000);

  // A tab in the background is told nothing, by the browser's choice and not
  // the appliance's: counted from its return, or every unlocked phone would
  // open on an alarm.
  const resumed = () => {
    if (document.visibilityState === 'visible') {
      since = Date.now();
      lost = false;
    }
  };
  document.addEventListener('visibilitychange', resumed);

  return () => {
    watching = false;
    close();
    clearInterval(timer);
    document.removeEventListener('visibilitychange', resumed);
    heardAt = null;
    lost = false;
  };
}
