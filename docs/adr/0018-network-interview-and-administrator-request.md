# Network interview and administrator request

- Status: accepted
- Date: 2026-07-31

## Context and Problem Statement

The appliance has to reach the site's network, and sites differ enormously: a
shared password at one, 802.1X with per-user accounts at the next, a sign-in
page at a third, hardware addresses registered in advance at a fourth.

Two people stand between the appliance and that network, and neither can do the
other's job. The **installer** is on site with the box and does not know what
WPA2-Enterprise is. The **site's network administrator** knows exactly what
their network requires but is not present, and will act on a request only if it
states precisely what is wanted.

Asking the installer to pick a protocol fails on the first point. Leaving the
appliance to fail silently fails on the second: nobody learns what to ask for.

## Considered Options

- **Offer protocol choices** — WPA2-Personal, WPA2-Enterprise with PEAP, EAP-TLS
  — and let the installer pick.
- **Detect the network** by scanning and reading what it advertises.
- **Interview the installer** about what they can observe, and derive the rest.

## Decision Outcome

**Interview the installer**, in terms of what joining the network *does* rather
than what it *is*: what a phone is asked for when it joins. Nothing, one shared
password, a personal account, a certificate installed beforehand, a sign-in page
— or "I do not know", which is an answer rather than a failure to give one.

Protocol choices were rejected because they ask the one person present the one
question they cannot answer. Detection was rejected as unreliable in exactly the
cases that matter: a network advertises its authentication method, not whether
this particular appliance will be allowed on it, nor whether its address must be
registered first.

Three consequences follow.

**The survey is stored apart from the uplink.** One says what the site demands,
the other what the appliance will do. A site that requires 802.1X leaves the
appliance offline — and the reason has to survive that decision, because it is
what the request to the network administrator is rebuilt from. Folding the two
together would lose it at the moment it becomes useful.

**Answers are recorded as they are given, not saved.** They are statements about
the site rather than a value being composed, so there is nothing to review before
committing. It also leaves exactly one Save button on the screen — the one for
the credentials, where a value genuinely is being composed.

**"I do not know" is not joinable.** Guessing would have the appliance sit
failing to connect for reasons nobody can see. Treating it as an unknown that
the administrator must resolve is the honest reading, and it produces the
request that resolves it.

## The request to the network administrator

Derived from the survey rather than stored, so it can never describe a survey
since corrected, and **produced only when the site actually has to act**: an
ordinary password-protected network needs nothing from them, and issuing a
request anyway would train people to ignore the ones that matter.

It is rendered by the dashboard, not the appliance. A 512 MB appliance has no
business carrying a document renderer and fonts; and the screen version and the
Markdown download must come from one source, which puts them on the same side.
Printing is isolated with a CSS rule rather than a library.

The hardware address of the uplink adapter is carried as a **blank to fill in**.
Both address requests are keyed on it, and the appliance cannot read it before
the adapter is fitted — a blank states that it is expected, where an absent row
would not and a guessed one would be wrong.

## Consequences

Enterprise authentication is **surveyed but not implemented**: the appliance
records that the site requires it and asks the administrator for a derogation —
a wired port on an equipment VLAN, or an exemption for its address — which is
the fallback the site handout already described. Storing EAP credentials that no
code can act on would invite an installer to type a secret that goes nowhere.

Implementing it belongs with the hardware, where it can be tried against a real
supplicant rather than designed against a specification. The survey's shape does
not change when it arrives: the answer that means "802.1X" is already recorded.
