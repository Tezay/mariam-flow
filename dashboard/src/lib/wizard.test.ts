import { describe, expect, it } from 'vitest';

import type { Proposal, Readiness, Stage, Status } from './api';
import {
  EXPECTED_RECEIVERS,
  canConfirmPairing,
  canSubmitUplink,
  completion,
  pairingPayload,
  receiverShortfall,
  revisitable,
  uplinkBody,
  visibleStage,
} from './wizard';

function status(readiness: Partial<Readiness>, stage: Stage): Status {
  const full: Readiness = {
    site_named: false,
    nodes_paired: false,
    uplink_decided: false,
    model_ready: false,
    ...readiness,
  };
  return {
    kit_id: 'KIT-0001',
    site_name: full.site_named ? 'RU EFREI' : null,
    phase: stage === 'complete' ? { phase: 'operational' } : { phase: 'onboarding', stage },
    readiness: full,
    runtime: { mode: 'idle' },
    model_installed: full.model_ready,
    service: { open: true },
    sensor_ap: { ssid: 'mariam-flow-0001', channel: 6 },
    uplink: { mode: full.uplink_decided ? 'offline' : 'undecided' },
    nodes: [],
  };
}

const OFFER: Proposal = {
  tx_mac: '1a:00:00:00:00:00',
  tx_agreement: 2,
  receivers: [
    { node_id: 'rx-1', address: '192.168.4.51' },
    { node_id: 'rx-2', address: '192.168.4.52' },
  ],
};

describe('visibleStage', () => {
  it('follows the appliance when nothing is being revisited', () => {
    expect(visibleStage(status({ site_named: true }, 'nodes'), null)).toBe('nodes');
  });

  it('shows a finished step the installer went back to', () => {
    expect(visibleStage(status({ site_named: true }, 'nodes'), 'site')).toBe('site');
  });

  it('refuses to revisit a step that is not finished', () => {
    // Otherwise "go back" could park the installer on a screen the appliance
    // has not reached, with nothing to correct.
    expect(visibleStage(status({ site_named: true }, 'nodes'), 'network')).toBe('nodes');
  });

  it('returns to the appliance once the revisited step is the current one', () => {
    expect(visibleStage(status({}, 'site'), 'site')).toBe('site');
  });

  it('reports completion when the installation is closed', () => {
    const done = status(
      { site_named: true, nodes_paired: true, uplink_decided: true, model_ready: true },
      'complete',
    );
    expect(visibleStage(done, null)).toBe('complete');
  });
});

describe('completion and revisitable', () => {
  it('reads each step off the readiness', () => {
    const done = completion(status({ site_named: true, uplink_decided: true }, 'nodes'));
    expect(done.site).toBe(true);
    expect(done.nodes).toBe(false);
    expect(done.network).toBe(true);
  });

  it('offers only finished steps to go back to, in order', () => {
    const state = status({ site_named: true, nodes_paired: true }, 'network');
    expect(revisitable(state)).toEqual(['site', 'nodes']);
  });

  it('offers nothing at the very start', () => {
    expect(revisitable(status({}, 'site'))).toEqual([]);
  });
});

describe('canConfirmPairing', () => {
  it('needs a transmitter and at least one receiver', () => {
    expect(canConfirmPairing(OFFER)).toBe(true);
    expect(canConfirmPairing({ ...OFFER, receivers: [OFFER.receivers[0]] })).toBe(true);
  });

  it('refuses an offer missing either role', () => {
    expect(canConfirmPairing({ ...OFFER, tx_mac: undefined })).toBe(false);
    expect(canConfirmPairing({ ...OFFER, receivers: [] })).toBe(false);
    expect(canConfirmPairing(null)).toBe(false);
  });
});

describe('receiverShortfall', () => {
  it('is silent when the designed pair is there', () => {
    expect(receiverShortfall(OFFER)).toBe(0);
  });

  it('counts what is missing without blocking', () => {
    expect(receiverShortfall({ ...OFFER, receivers: [OFFER.receivers[0]] })).toBe(1);
    expect(receiverShortfall({ ...OFFER, receivers: [] })).toBe(EXPECTED_RECEIVERS);
  });

  it('does not complain about more receivers than expected', () => {
    const extra = { ...OFFER, receivers: [...OFFER.receivers, { node_id: 'rx-3', address: 'x' }] };
    expect(receiverShortfall(extra)).toBe(0);
  });
});

describe('pairingPayload', () => {
  it('gives the transmitter a MAC and the receivers an address', () => {
    // How the appliance tells them apart: a transmitter never joins the
    // access point, so it has no address; a receiver is identified by one.
    const nodes = pairingPayload(OFFER);
    expect(nodes[0]).toEqual({ node_id: 'tx-1', role: 'tx', mac: '1a:00:00:00:00:00' });
    expect(nodes[1]).toEqual({ node_id: 'rx-1', role: 'rx', address: '192.168.4.51' });
    expect(nodes).toHaveLength(3);
  });

  it('sends only the receivers when no transmitter was agreed', () => {
    const nodes = pairingPayload({ ...OFFER, tx_mac: undefined });
    expect(nodes.every((node) => node.role === 'rx')).toBe(true);
  });

  it('keeps the identifiers the appliance offered', () => {
    // They skip any already in use, so overwriting them here would undo the
    // collision the appliance avoided.
    const replacement = { ...OFFER, receivers: [{ node_id: 'rx-3', address: '192.168.4.53' }] };
    expect(pairingPayload(replacement)[1].node_id).toBe('rx-3');
  });
});

describe('uplinkBody', () => {
  const wifi = { ssid: '  campus  ', passphrase: 'campus-secret' };

  it('states offline as a decision of its own', () => {
    expect(uplinkBody('offline', wifi)).toEqual({ mode: 'offline' });
  });

  it('trims the network name', () => {
    expect(uplinkBody('wifi', wifi)).toMatchObject({ mode: 'wifi', ssid: 'campus' });
  });

  it('sends an open network rather than an empty passphrase', () => {
    const open = uplinkBody('wifi', { ssid: 'guest', passphrase: '' }) as {
      security: { type: string };
    };
    expect(open.security).toEqual({ type: 'open' });
  });

  it('asks for DHCP on both connected forms', () => {
    expect(uplinkBody('ethernet', wifi)).toMatchObject({ addressing: { method: 'dhcp' } });
    expect(uplinkBody('wifi', wifi)).toMatchObject({ addressing: { method: 'dhcp' } });
  });
});

describe('canSubmitUplink', () => {
  it('needs nothing for offline or wired', () => {
    const empty = { ssid: '', passphrase: '' };
    expect(canSubmitUplink('offline', empty)).toBe(true);
    expect(canSubmitUplink('ethernet', empty)).toBe(true);
  });

  it('needs a network name for Wi-Fi', () => {
    expect(canSubmitUplink('wifi', { ssid: '   ', passphrase: 'campus-secret' })).toBe(false);
  });

  it('mirrors the passphrase length the appliance accepts', () => {
    // Refusing here saves a round trip; the appliance still decides.
    expect(canSubmitUplink('wifi', { ssid: 'campus', passphrase: 'short' })).toBe(false);
    expect(canSubmitUplink('wifi', { ssid: 'campus', passphrase: 'x'.repeat(64) })).toBe(false);
    expect(canSubmitUplink('wifi', { ssid: 'campus', passphrase: 'x'.repeat(8) })).toBe(true);
  });

  it('treats an empty passphrase as an open network, not an error', () => {
    expect(canSubmitUplink('wifi', { ssid: 'guest', passphrase: '' })).toBe(true);
  });
});
