# Declared service hours

- Status: accepted
- Date: 2026-07-29

## Context and Problem Statement

The appliance estimates continuously. Nothing tells it that the hall it
watches closes at 14:00, so it keeps producing waiting times through the
afternoon, the night and the summer break.

Those estimates are not merely useless, they are wrong in a way that
propagates. A closed hall has no queue, but the sensing chain does not
measure people directly: it measures how the radio channel is disturbed.
An empty room still produces frames, still yields a density class, and the
smoothing that makes the live figure readable will carry a plausible-looking
number for minutes after the last person leaves. Those minutes then enter
the minute history (ADR 0014) as ordinary rows, and every later reading of
that history — a daily peak, a comparison between two weeks — averages real
service with hours that were never served.

There is a second cost. The estimate is the expensive part of the loop, and
on a Pi Zero 2 W the appliance spends the night computing answers to a
question nobody asked.

## Considered Options

- **Leave it to whoever reads the data.** Push everything and filter later,
  in the dashboard or downstream.
- **Detect closure from the signal.** Infer that the site is shut when the
  channel goes quiet, or when the density has been `empty` long enough.
- **Declare the hours.** Store a weekly schedule with the configuration and
  stop estimating outside it.

## Decision Outcome

**Declare the hours**, as an optional part of the appliance configuration:
a time zone, seven lists of intervals, and a list of closure date ranges.

Filtering downstream was rejected because it puts the same rule in every
consumer and leaves the journal itself untrue. The history is meant to be
the record; a record that needs a footnote to be read is a weaker artefact
than one that only contains served minutes.

Detecting closure from the signal was rejected for being a second inference
problem stacked on the first, and a poorly conditioned one: a long `empty`
stretch is exactly what a genuinely quiet Tuesday morning looks like. The
appliance would have to guess something the operator already knows for
certain. Opening hours are a fact about the site, so they are configured,
not inferred.

Consequences of the decision, each chosen deliberately:

- **Absent means always open.** An appliance whose hours have not yet been
  declared keeps estimating. The alternative — silence until configured —
  makes a forgotten field look identical to a broken sensor, which is the
  worse failure on a machine nobody monitors.
- **The schedule is replaced whole, never patched.** Its parts constrain one
  another: intervals of a day must not overlap, a closure must not end
  before it begins. Validating one field against a stored remainder checks
  half a thing, so `PUT /api/service-window` takes the entire schedule and
  `null` clears it.
- **Hours are local, and the time zone is stored with them.** A site that
  opens at 08:00 opens at 08:00 in both halves of the year. Resolving them
  against a fixed offset would shift the service by an hour at each daylight
  saving transition, so the schedule names an IANA zone and the system time
  zone database resolves it.
- **Closing is a journal event.** `service-opened` and `service-closed` join
  the lifecycle entries, so a gap in the history can be told apart from an
  outage — the question anyone reading a flat afternoon asks first.
- **The open minute is flushed on transition.** The minute in progress is
  written before the appliance goes quiet, so the last minute of service is
  not lost to the closure.
- **The stream keeps being read while closed.** Only estimation stops.
  Sensor health stays observable, so an installer working outside opening
  hours can still see that both nodes are alive.

## Consequences

The API reports service state alongside every status and live event, so the
dashboard can say *closed* rather than showing an estimate that has stopped
moving for reasons it cannot explain. Refusals name the day at fault, since
seven days are edited on one screen.

The appliance now depends on the system time zone database, which
Raspberry Pi OS ships; no copy travels in the binary.
