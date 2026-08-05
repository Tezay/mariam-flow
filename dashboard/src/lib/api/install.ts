/** Bringing an appliance into service, and declaring it done. */

import { put, type WriteOutcome } from './http';
import type { ClassMapping, NetworkSurvey, WaitTuning } from './status';

/** Names the site this appliance is installed at. */
export function saveSite(siteName: string): Promise<WriteOutcome> {
  return put('/api/site', { site_name: siteName });
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
 * Records what the queue looks like and how fast it is served.
 *
 * The words and the counts go together: an appliance that could label a queue
 * it cannot time, or the reverse, is half described.
 */
export function saveQueue(classes: ClassMapping | null, wait: WaitTuning): Promise<WriteOutcome> {
  return put('/api/queue', { classes, wait });
}

/** Closes the installation, or reopens it. */
export function setInstallation(completed: boolean): Promise<WriteOutcome> {
  return put('/api/installation', { completed });
}
