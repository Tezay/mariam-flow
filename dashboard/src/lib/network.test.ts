import { describe, expect, it } from 'vitest';

import { type NetworkSurvey, type SiteAuthentication, type Status } from './api/status';
import {
  AUTHENTICATION_CHOICES,
  applianceFacts,
  asksFor,
  handoutMarkdown,
  joinable,
  requiresAdministrator,
  verdict,
  uplinkBlocked,
} from './network';

function survey(
  authentication: SiteAuthentication,
  extra: Partial<NetworkSurvey> = {},
): NetworkSurvey {
  return { authentication, registration_required: false, fixed_address: false, ...extra };
}

const STATUS: Status = {
  kit_id: 'KIT-0042',
  site_name: 'RU EFREI',
  phase: { phase: 'onboarding', stage: 'network' },
  readiness: {
    site_named: true,
    nodes_paired: true,
    uplink_decided: false,
    queue_described: true,
    site_captured: false,
    model_ready: false,
  },
  runtime: { mode: 'idle' },
  model_installed: false,
  service: { open: true },
  sensor_ap: { ssid: 'mariam-flow-0042', channel: 6 },
  uplink: { mode: 'undecided' },
  nodes: [],
};

describe('joinable', () => {
  it('accepts the two the appliance can handle unaided', () => {
    expect(joinable('nothing')).toBe(true);
    expect(joinable('shared-password')).toBe(true);
  });

  it('refuses the ones that need the site to act', () => {
    // Each of these needs something the appliance cannot supply on its own:
    // an account, a certificate, or someone at a browser.
    expect(joinable('account')).toBe(false);
    expect(joinable('certificate')).toBe(false);
    expect(joinable('sign-in-page')).toBe(false);
  });

  it('treats "do not know" as not joinable', () => {
    // Guessing here would have the appliance sit silently failing to connect;
    // asking the administrator is the honest outcome.
    expect(joinable('unknown')).toBe(false);
  });

  it('offers every answer the survey can hold', () => {
    const offered = new Set(AUTHENTICATION_CHOICES);
    for (const answer of [
      'nothing',
      'shared-password',
      'account',
      'certificate',
      'sign-in-page',
      'unknown',
    ] as const) {
      expect(offered.has(answer)).toBe(true);
    }
  });
});

describe('verdict', () => {
  it('says nothing before the interview is answered', () => {
    expect(verdict(null)).toEqual({ kind: 'unanswered' });
  });

  it('asks for a passphrase only when there is one to give', () => {
    expect(verdict(survey('shared-password'))).toEqual({ kind: 'joinable', needsPassphrase: true });
    expect(verdict(survey('nothing'))).toEqual({ kind: 'joinable', needsPassphrase: false });
  });

  it('names what the administrator has to be told about', () => {
    expect(verdict(survey('account'))).toEqual({
      kind: 'needs-administrator',
      reason: 'account',
    });
  });
});

describe('requiresAdministrator', () => {
  it('leaves an ordinary network alone', () => {
    // The installer types the password and the appliance joins; a request
    // here would train people to ignore the ones that matter.
    expect(requiresAdministrator(survey('shared-password'), false)).toBe(false);
    expect(requiresAdministrator(survey('nothing'), false)).toBe(false);
  });

  it('is needed for anything the appliance cannot join unaided', () => {
    expect(requiresAdministrator(survey('account'), false)).toBe(true);
    expect(requiresAdministrator(survey('sign-in-page'), false)).toBe(true);
    expect(requiresAdministrator(survey('unknown'), false)).toBe(true);
  });

  it('is needed when the site has to let the appliance on or address it', () => {
    expect(
      requiresAdministrator(survey('shared-password', { registration_required: true }), false),
    ).toBe(true);
    expect(requiresAdministrator(survey('shared-password', { fixed_address: true }), false)).toBe(
      true,
    );
  });

  it('asks nothing of a site the appliance never joins', () => {
    expect(requiresAdministrator(survey('account'), true)).toBe(false);
    expect(requiresAdministrator(null, false)).toBe(false);
  });
});

describe('asksFor', () => {
  it('asks for nothing when the appliance stays offline', () => {
    // An offline appliance makes no request of the site at all, whatever the
    // survey found.
    expect(asksFor(survey('account'), true)).toEqual([]);
  });

  it('asks for nothing before the interview is answered', () => {
    expect(asksFor(null, false)).toEqual([]);
  });

  it('asks for Wi-Fi access on an ordinary network', () => {
    expect(asksFor(survey('shared-password'), false)).toEqual(['wifi-access', 'reserve-address']);
  });

  it('falls back to a wired port when the network needs an account', () => {
    expect(asksFor(survey('account'), false)).toEqual([
      'exempt-from-authentication',
      'wired-port',
      'reserve-address',
    ]);
  });

  it('treats a sign-in page as its own kind of exemption', () => {
    // Not the same request as an authentication exemption: the administrator
    // has a different lever for each.
    expect(asksFor(survey('sign-in-page'), false)).toContain('exempt-from-portal');
    expect(asksFor(survey('sign-in-page'), false)).not.toContain('exempt-from-authentication');
  });

  it('separates being let on from being given a stable address', () => {
    const asks = asksFor(survey('shared-password', { registration_required: true }), false);
    expect(asks).toContain('allow-address');
    expect(asks).toContain('reserve-address');
  });

  it('asks for the address itself when the site assigns fixed ones', () => {
    const asks = asksFor(survey('shared-password', { fixed_address: true }), false);
    expect(asks).toContain('fixed-address');
    expect(asks).not.toContain('reserve-address');
  });
});

describe('applianceFacts', () => {
  it('carries what the appliance knows about itself', () => {
    const facts = applianceFacts(STATUS);
    expect(facts.find((row) => row.label === 'kit')?.value).toBe('KIT-0042');
    expect(facts.find((row) => row.label === 'sensorAp')?.value).toContain('mariam-flow-0042');
  });

  it('leaves the uplink address blank rather than guessing it', () => {
    // It is what both address requests are keyed on, and it cannot be read
    // before the USB adapter is fitted.
    expect(applianceFacts(STATUS).find((row) => row.label === 'uplinkMac')?.value).toBe('');
  });

  it('survives a site that has not been named yet', () => {
    const unnamed = { ...STATUS, site_name: null };
    expect(applianceFacts(unnamed).find((row) => row.label === 'site')?.value).toBe('');
  });
});

describe('handoutMarkdown', () => {
  const doc = {
    title: 'Request for the site network team',
    facts: [
      { label: 'Kit', value: 'KIT-0042' },
      { label: 'Hardware address', value: '' },
    ],
    asks: ['Access to the site Wi-Fi.', 'A DHCP reservation.'],
    sections: [{ heading: 'Network flows', body: 'No inbound flow is required.' }],
  };

  it('leads with the title and the facts', () => {
    const markdown = handoutMarkdown(doc);
    expect(markdown.startsWith('# Request for the site network team')).toBe(true);
    expect(markdown).toContain('- **Kit**: KIT-0042');
  });

  it('shows a blank to fill in rather than dropping an unknown value', () => {
    // The hardware address cannot be read before the adapter is fitted, and
    // the request has to show that it is expected.
    expect(handoutMarkdown(doc)).toContain('- **Hardware address**: _______');
  });

  it('writes the asks as a checklist the network team can work through', () => {
    expect(handoutMarkdown(doc)).toContain('- [ ] Access to the site Wi-Fi.');
  });

  it('keeps the standing sections after the request', () => {
    const markdown = handoutMarkdown(doc);
    expect(markdown.indexOf('## Network flows')).toBeGreaterThan(markdown.indexOf('- [ ]'));
  });

  it('produces a document with no asks when nothing is required', () => {
    const markdown = handoutMarkdown({ ...doc, asks: [] });
    expect(markdown).not.toContain('- [ ]');
    expect(markdown).toContain('## Network flows');
  });

  it('ends with a newline, as a file should', () => {
    expect(handoutMarkdown(doc).endsWith('\n')).toBe(true);
  });
});

describe('uplinkBlocked', () => {
  it('lets the site Wi-Fi through once the interview says the appliance can join', () => {
    expect(uplinkBlocked('wifi', { kind: 'joinable', needsPassphrase: true })).toBeNull();
  });

  it('refuses the site Wi-Fi rather than quietly storing offline', () => {
    // The defect this replaces: the form saved "offline" when the network
    // needed an administrator, so the choice on screen was never stored and
    // came back reset.
    expect(uplinkBlocked('wifi', { kind: 'needs-administrator', reason: 'account' })).toBe(
      'needs-administrator',
    );
    expect(uplinkBlocked('wifi', { kind: 'unanswered' })).toBe('unanswered');
  });

  it('never blocks the choices that need no network to be understood', () => {
    for (const answer of [
      { kind: 'unanswered' } as const,
      { kind: 'needs-administrator', reason: 'certificate' } as const,
    ]) {
      expect(uplinkBlocked('offline', answer)).toBeNull();
      expect(uplinkBlocked('ethernet', answer)).toBeNull();
    }
  });
});
