# SQLite appliance journal with bounded retention

- Status: accepted
- Date: 2026-07-28

## Context and Problem Statement

An installed appliance runs unattended for months, and when something goes
wrong the first questions are always the same: when did this unit last
reboot, what changed before it broke, and who has been reaching it. None of
that can be answered today — the daemon holds its current state and forgets
everything else.

Configuration already lives in a validated JSON file (ADR 0008), and that is
the right shape for it: small, current, occasionally read off a card by
hand. History is the opposite. It accumulates, it is queried by time, and it
must be pruned, which is exactly what a file is bad at.

Three constraints shape the answer. The deployment target boots from an SD
card, where every committed write costs endurance. Nothing may grow without
bound on a 32 GB card. And an authentication journal that identifies who
reached the appliance touches personal data, on a project whose central
claim is that it collects none.

## Considered Options

For the store: append-only files alongside the configuration, or SQLite.

For durability: commit every event, buffer everything, or split by kind.

For client addresses: record them, record a truncated form, or record none.

## Decision Outcome

**SQLite**, in `appliance.db` beside the rest of the appliance data. The
operations wanted here — "the last hundred events", "everything older than
90 days, deleted", "keep only the newest N rows" — are queries, and
reimplementing them over append-only files means writing a small database
badly. The `bundled` build compiles SQLite from source, so the cross-compiled
target needs no system library.

One table holds four categories of event: access, appliance lifecycle,
installation progress, and node connectivity. Access and lifecycle have
emitters today; the other two are part of the vocabulary and are written as
their features land.

**Durability is split by kind.** Access events are committed before the call
returns, because those are precisely the ones an attacker would erase by
pulling the power. Everything else is buffered and written in a single
transaction every few seconds, or when the daemon shuts down. Committing
every event would multiply physical writes for entries whose loss on a power
cut costs nothing; buffering everything would make the journal useless for
the one purpose that most needs it. The database runs in WAL mode with
`synchronous=FULL`, since a weaker setting would make "immediate" a promise
the journal could not keep.

The daemon handles SIGTERM and SIGINT so that shutdown flushes what is
buffered rather than discarding it.

**Client addresses are recorded** on access events. This is administration
data — an installer's phone, a technician's laptop — and is what makes the
difference between a journal that reports an attack and one that reports it
and says where it came from. It concerns the people who administer the
appliance, never the people in the monitored queue, of whom nothing is
collected. Because an address is personal data under GDPR, retention is
bounded and the handout given to site network administrators states that the
appliance keeps them.

**Retention has two bounds**, because either alone fails. A 90-day window
covers the need to investigate a problem reported late, and the ceiling on
retained rows guarantees that a runaway loop cannot fill the card in an
afternoon however recent its output is.

Finally, a journal write that fails is reported and the request carries on.
An appliance that refused to authenticate anyone because its card filled up
would be worse than one that loses an audit line.

### Consequences

- Good: the questions that start every remote diagnosis are answerable, and
  the security-relevant subset survives an abrupt power loss.
- Good: disk use is bounded from both directions, and physical writes stay
  proportional to security events rather than to total event volume.
- Good: the same store is where estimate and node-health series will land,
  with retention and downsampling already the natural operations.
- Bad: a new dependency that compiles C at build time. The cross-compilation
  check already installs a cross C toolchain for the inference runtime, so
  the cost is a longer build rather than a new capability.
- Bad: lifecycle, installation and node events can be lost on a power cut —
  accepted deliberately, and pinned by a test so the trade-off stays
  explicit rather than becoming folklore.
- Bad: recording addresses obliges the product to disclose it and to honour
  the retention it claims. That obligation is documented rather than
  discovered later.
