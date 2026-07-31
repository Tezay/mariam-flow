/**
 * The installation wizard's pure logic.
 *
 * Progress is derived from the appliance's readiness, never from a cursor the
 * browser keeps: an installation interrupted mid-step resumes where the stored
 * facts say it stands, whatever the browser was showing.
 */

import type { Proposal, SensingNode, Stage, Status } from './api';

/** The guided steps, in order. `complete` is the end, not a step. */
export const STEPS: Stage[] = ['site', 'nodes', 'network', 'calibration'];

/** Receivers the system is designed around: two, framing the queue. */
export const EXPECTED_RECEIVERS = 2;

/** Whether each step's facts are satisfied. */
export function completion(status: Status): Record<Stage, boolean> {
  return {
    site: status.readiness.site_named,
    nodes: status.readiness.nodes_paired,
    network: status.readiness.uplink_decided,
    calibration: status.readiness.site_captured,
    complete: false,
  };
}

/**
 * The step to show: the one being revisited, or the appliance's own.
 *
 * A revisit is refused once its facts stop being satisfied, so correcting a
 * step cannot strand the installer on a screen the appliance has moved past.
 */
export function visibleStage(status: Status, revisiting: Stage | null): Stage {
  const current = status.phase.phase === 'onboarding' ? status.phase.stage : 'complete';
  if (revisiting && revisiting !== current && completion(status)[revisiting]) {
    return revisiting;
  }
  return current;
}

/** Steps already satisfied, which an installer may go back and correct. */
export function revisitable(status: Status): Stage[] {
  const done = completion(status);
  return STEPS.filter((step) => done[step]);
}

/** Whether the offer holds enough to pair: a transmitter and a receiver. */
export function canConfirmPairing(proposal: Proposal | null): boolean {
  return Boolean(proposal?.tx_mac) && (proposal?.receivers.length ?? 0) > 0;
}

/**
 * How many receivers short of the designed pair the offer is.
 *
 * Reported, never enforced: a site may be commissioned with one receiver while
 * the second is installed later, and refusing to continue would also block
 * repairing an installation that has lost one.
 */
export function receiverShortfall(proposal: Proposal | null): number {
  return Math.max(0, EXPECTED_RECEIVERS - (proposal?.receivers.length ?? 0));
}

/**
 * The node list to send from an accepted offer.
 *
 * The transmitter carries a MAC and no address — it never joins the access
 * point — and each receiver the reverse, which is how the appliance tells them
 * apart at intake.
 */
export function pairingPayload(proposal: Proposal): SensingNode[] {
  const nodes: SensingNode[] = [];
  if (proposal.tx_mac) {
    nodes.push({ node_id: 'tx-1', role: 'tx', mac: proposal.tx_mac });
  }
  for (const receiver of proposal.receivers) {
    nodes.push({ node_id: receiver.node_id, role: 'rx', address: receiver.address });
  }
  return nodes;
}

/** The uplink body for each choice the network step offers. */
export function uplinkBody(
  choice: 'offline' | 'wifi' | 'ethernet',
  wifi: { ssid: string; passphrase: string },
): unknown {
  if (choice === 'offline') {
    return { mode: 'offline' };
  }
  if (choice === 'ethernet') {
    return { mode: 'ethernet', addressing: { method: 'dhcp' } };
  }
  return {
    mode: 'wifi',
    ssid: wifi.ssid.trim(),
    security: wifi.passphrase
      ? { type: 'wpa-personal', passphrase: wifi.passphrase }
      : { type: 'open' },
    addressing: { method: 'dhcp' },
  };
}

/** Whether the network step holds enough to be sent. */
export function canSubmitUplink(
  choice: 'offline' | 'wifi' | 'ethernet',
  wifi: { ssid: string; passphrase: string },
): boolean {
  if (choice !== 'wifi') {
    return true;
  }
  // The appliance rejects a passphrase outside 8..=63; an open network is a
  // deliberate choice rather than an empty field.
  const length = wifi.passphrase.length;
  return wifi.ssid.trim().length > 0 && (length === 0 || (length >= 8 && length <= 63));
}
