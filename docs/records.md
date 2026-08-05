# What the appliance retains

History lives in SQLite (`appliance.db`) rather than in files: it accumulates,
it is queried by time, and it has to be pruned (ADR 0012).

## Estimate history

Estimates are folded into one row per minute. Keeping each one would cost
roughly 1.9 GB a year against about 31 MB folded, and answers the same
questions — a chart of a day paints several minutes to a pixel.

The class of a minute is its most frequent class rather than an average:
density classes are ordinal labels, and the mean of `empty` and `saturated` is
not `medium`. Ties resolve towards the busier class. The mean level is kept
alongside for callers wanting a continuous curve, and reliability is a count
rather than a flag — a minute where two samples in sixty were trustworthy is
not a reliable minute.

Retention is bounded on two axes, two years and a row ceiling.

`GET /api/estimates?minutes=N` returns the most recent minutes in
chronological order.

Unlike the public estimate, the administration view shows an unreliable value
and marks it as such: an operator needs to see what the model produced and that
it is not trustworthy.

## Event journal

One table records four categories of event — access, appliance lifecycle,
installation progress and node connectivity.

Durability is split by kind. Access events are committed before the call
returns, being the ones an attacker would erase by pulling the power;
everything else is buffered and written in one transaction every few seconds,
at shutdown, or when the buffer fills. The database runs in WAL mode with
`synchronous=FULL`. The daemon handles SIGTERM and SIGINT in order to flush
rather than discard what is buffered.

Access events carry the client address — administration data, never anything
about the people in the monitored queue. Because an address is personal data,
retention is bounded on two axes: a 90-day window, and a ceiling on retained
rows. A journal write that fails is reported and the request carries on:
refusing to authenticate anyone because the card filled up would be the worse
failure.

### Flood resistance

A flood of refused logins cannot be used to erase the journal. A throttled
attempt is refused before the password hash is computed, so it costs the client
a round trip and nothing else. One row marks the start of a block and one
reports how many further attempts it refused — never drop silently, coalesce
and count. Refusals are the one access event that is buffered rather than
committed immediately, being implied by the failures that earned them.

### Reading it back

`GET /api/events` returns rows newest first with a clamped `limit`, paged on
the row identifier rather than on an offset: the journal is written while it is
read, and an offset would repeat some rows and skip others.

`GET /api/events.csv` returns the same selection as a file.
