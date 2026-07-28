# Minute-resolution estimate history

- Status: accepted
- Date: 2026-07-29

## Context and Problem Statement

The appliance now runs the inference chain itself and produces an estimate
roughly every second. Two questions follow immediately: how a browser learns
of a new estimate, and what — if anything — is kept once it is superseded.

The second question is the one with consequences. A site manager wants to
know how busy last Tuesday was, whether the queue is worse this term than
last, and when the peaks fall. Those questions cannot be answered from a
current value; they need a record. But the record accumulates forever on a
32 GB card already shared with capture sessions, on a machine nobody
monitors.

## Considered Options

For delivery: polling, WebSockets, or server-sent events.

For the record: keeping every estimate, keeping one row per minute, or
keeping nothing and computing summaries from capture sessions.

## Decision Outcome

**Server-sent events.** The traffic is entirely one-way — the browser
listens and never pushes — and `EventSource` reconnects on its own, which is
the behaviour that matters on a phone whose Wi-Fi drops as its owner walks
across a service hall. A WebSocket would buy a return channel nothing needs,
paid for with heartbeats and reconnection logic written by hand. Polling
would trade latency for requests on a machine with little to spare.

Each event carries the estimate **and** the stream health together, because
the screen needs both to say anything true: a missing estimate means one
thing while the nodes are streaming and quite another once they have gone
silent.

**One row per minute.** Keeping every estimate costs roughly 1.9 GB a year;
folding to the minute costs about 31 MB, and answers the same questions —
any chart of a day paints several minutes to a pixel. Aggregating as the
stream arrives rather than at query time also pays the cost once, on the
machine least able to afford it.

Two details of the fold are deliberate:

- The class of a minute is its **most frequent** class, not the mean of the
  four. They are ordinal labels: the mean of `empty` and `saturated` is not
  `medium`, it is nothing at all. The mean *level* is stored alongside for
  callers wanting a continuous curve. Ties resolve towards the busier class,
  because understating a queue costs more trust than overstating it.
- Reliability is stored as a **count**, not a flag. A minute in which two
  samples of sixty met the confidence threshold is not a reliable minute,
  and only the ratio can express that.

**Retention is bounded on two axes**, as the event journal already is: two
years — long enough to compare a term against the same term a year earlier —
and a hard row ceiling so nothing can fill the card whatever the estimate
rate becomes.

**The pipeline runs on its own thread** and publishes through a `watch`
channel. Reading frames is a blocking loop; the async runtime must stay free
to answer requests, and a slow browser must never apply back-pressure to
sensing. A client that misses intermediate values loses nothing it wanted:
it is after the current estimate, not the history of every one.

Starting requires a model, a tuned site and a receiver — exactly the facts
the installation readiness already tracks. An appliance that cannot estimate
is therefore not an error to report but an installation that has not reached
calibration; the daemon says which piece is missing and serves regardless.

### Consequences

- Good: the whole chain — frames, features, model, Little's Law, smoothing —
  runs inside the product, and can be exercised end to end against a
  recorded capture on a machine with no sensors attached.
- Good: history costs tens of megabytes a year and is bounded from both
  directions; the questions a manager asks are answerable without keeping
  raw measurements, which never leave the site anyway.
- Bad: per-second detail is lost, so a post-mortem of a specific thirty
  seconds is not possible from the history alone. The recorded capture
  sessions remain the instrument for that, and they keep everything.
- Bad: an estimate arriving out of order is folded into the open minute
  rather than its own. It is at most a second out of place, and rewinding
  would emit rows out of order and collide with one already written.
- Bad: server-sent events hold a connection open per viewing browser. On an
  appliance serving a handful of people this is negligible, but it is a
  reason not to expose the dashboard broadly.
