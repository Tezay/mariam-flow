/**
 * The network interview's pure logic.
 *
 * The installer is asked what a phone is asked when it joins the site's
 * network, not which protocol the site runs: the first is something they can
 * answer on the spot, the second is what the answer means. Everything below
 * translates between the two.
 */

import type { NetworkSurvey, SiteAuthentication, Status } from './api';

/** The answers offered, in the order they are shown. */
export const AUTHENTICATION_CHOICES: SiteAuthentication[] = [
  'shared-password',
  'account',
  'sign-in-page',
  'certificate',
  'nothing',
  'unknown',
];

/** Whether the appliance can join a network that asks this, today. */
export function joinable(authentication: SiteAuthentication): boolean {
  return authentication === 'nothing' || authentication === 'shared-password';
}

/** What the appliance can do with what the survey found. */
export type Verdict =
  /** It can join, once the passphrase is known. */
  | { kind: 'joinable'; needsPassphrase: boolean }
  /** The site must act before the appliance can join at all. */
  | { kind: 'needs-administrator'; reason: SiteAuthentication }
  /** Nothing has been answered yet. */
  | { kind: 'unanswered' };

export function verdict(survey: NetworkSurvey | null): Verdict {
  if (!survey) {
    return { kind: 'unanswered' };
  }
  if (joinable(survey.authentication)) {
    return { kind: 'joinable', needsPassphrase: survey.authentication === 'shared-password' };
  }
  return { kind: 'needs-administrator', reason: survey.authentication };
}

/**
 * Whether the site's network team has to be involved at all.
 *
 * An ordinary password-protected network needs nothing from them: the
 * installer types the password and the appliance joins. Producing a request
 * anyway would train people to ignore it on the occasions it matters.
 */
export function requiresAdministrator(survey: NetworkSurvey | null, offline: boolean): boolean {
  if (!survey || offline) {
    return false;
  }
  return !joinable(survey.authentication) || survey.registration_required || survey.fixed_address;
}

/** One thing the site's network administrator is asked for. */
export type Ask =
  | 'wifi-access'
  | 'wired-port'
  | 'allow-address'
  | 'reserve-address'
  | 'fixed-address'
  | 'exempt-from-authentication'
  | 'exempt-from-portal';

/**
 * The request to put to the site, derived rather than stored so it can never
 * describe a survey that has since been corrected.
 *
 * Two of these are keyed on the same hardware address without being the same
 * request: allowing the appliance onto the network at all, and giving it a
 * stable address once it is on.
 */
export function asksFor(survey: NetworkSurvey | null, offline: boolean): Ask[] {
  if (!survey || offline) {
    return [];
  }
  const asks: Ask[] = [];

  if (survey.authentication === 'sign-in-page') {
    // A sign-in page cannot be got through by an appliance: it has no browser
    // and nobody stands in front of it.
    asks.push('exempt-from-portal', 'wired-port');
  } else if (!joinable(survey.authentication)) {
    asks.push('exempt-from-authentication', 'wired-port');
  } else {
    asks.push('wifi-access');
  }

  if (survey.registration_required) {
    asks.push('allow-address');
  }
  asks.push(survey.fixed_address ? 'fixed-address' : 'reserve-address');

  return asks;
}

/** One line of the request, its label already in the reader's language. */
export type HandoutRow = { label: string; value: string };

/** The facts the appliance knows about itself, named by translation key. */
export type FactLabel = 'kit' | 'site' | 'sensorAp' | 'uplinkMac';

/**
 * The facts the appliance contributes to the request.
 *
 * The uplink adapter's hardware address is what both address requests are
 * keyed on, and the appliance cannot read it before the adapter is fitted —
 * so the row is present and blank, to be filled in on site, rather than
 * absent or guessed.
 */
export function applianceFacts(status: Status): { label: FactLabel; value: string }[] {
  return [
    { label: 'kit', value: status.kit_id },
    { label: 'site', value: status.site_name ?? '' },
    { label: 'sensorAp', value: `${status.sensor_ap.ssid} (${status.sensor_ap.channel})` },
    { label: 'uplinkMac', value: '' },
  ];
}

/** The handout, with every string already in the reader's language. */
export type Handout = {
  title: string;
  facts: HandoutRow[];
  asks: string[];
  sections: { heading: string; body: string }[];
};

/**
 * The handout as Markdown, for sending on to the site's network team.
 *
 * Markdown rather than a rendered document: the appliance is asked for a file
 * someone can paste into an email or a ticket, and producing a PDF would mean
 * carrying a renderer and fonts on a machine with 512 MB.
 */
export function handoutMarkdown(doc: Handout): string {
  const lines = [`# ${doc.title}`, ''];

  for (const row of doc.facts) {
    // An empty value is a blank to fill in, not a missing row: the hardware
    // address is unknown until the adapter is fitted, and the request has to
    // show that it is expected.
    lines.push(`- **${row.label}**: ${row.value || '_______'}`);
  }

  if (doc.asks.length > 0) {
    lines.push('');
    for (const ask of doc.asks) {
      lines.push(`- [ ] ${ask}`);
    }
  }

  for (const section of doc.sections) {
    lines.push('', `## ${section.heading}`, '', section.body);
  }

  return `${lines.join('\n')}\n`;
}

/**
 * Why the site Wi-Fi cannot be saved, or nothing when it can.
 *
 * The appliance stores an uplink it can act on or nothing at all. The choice
 * on screen used to be quietly rewritten to "offline" when the network turned
 * out to need an administrator, which stored something the reader never
 * picked; it is refused with a reason instead.
 */
export function uplinkBlocked(
  choice: 'offline' | 'wifi' | 'ethernet',
  answer: Verdict,
): 'unanswered' | 'needs-administrator' | null {
  if (choice !== 'wifi' || answer.kind === 'joinable') {
    return null;
  }
  return answer.kind;
}
