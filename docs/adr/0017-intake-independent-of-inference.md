# Intake independent of inference

- Status: accepted
- Date: 2026-07-30

## Context and Problem Statement

The appliance had one thread doing two things: reading the frame stream, and
estimating from it. Starting it required a density model, a tuned site and a
paired receiver; without any of them it refused to start, and no socket was
opened.

That refusal made pairing from the observed stream (ADR 0016) impossible. An
appliance being installed has no model and no paired nodes — which is exactly
when it must listen, because listening is how the nodes to pair are found. The
feature and the gate could not both stand.

The conflation was already visible elsewhere. Outside service hours the
appliance reads its stream and does not estimate (ADR 0015), so "reading
without estimating" was an understood state; the appliance simply could not
*enter* it at startup.

## Considered Options

- **Attach a second reader** for discovery, alongside the inference pipeline.
- **Let the pipeline start degraded**, holding an optional estimator.
- **Separate the two concerns**: the intake always reads; estimation is a
  stage that may or may not be attached.

## Decision Outcome

**Separate the two concerns.**

A second reader was rejected outright: two things reading one UDP socket is a
contention problem invented to avoid naming the real one, and the appliance's
stream arbitration exists precisely to keep a single consumer.

Reading the stream depends only on having an input. Estimating depends on a
model, a tuned site and at least one receiver. So:

- the intake opens its source and reads, always;
- an estimator is assembled when the configuration allows one, and the loop
  skips that stage when there is none — the same way it already skips it
  outside service hours.

**Absence of a model or a paired node is no longer an error.** The installation
readiness already reports which step is outstanding; carrying the same
diagnosis in a second place would let the two drift apart. What *is* reported
is a model that exists but cannot be loaded: that is a fault to surface, and it
must not look like a step left to do.

Two further consequences were forced by the decision, and both fix latent
faults of their own.

**The intake must not depend on traffic to do its periodic work.** It
publishes stream health and the senders it has heard on a tick. Before any node
is paired, no frame is ever yielded, so a loop driven only by frame arrival
would never tick. A read timeout therefore bounds *how long a read call may
take*, not how long it waits for a datagram — datagrams that yield no frame are
consumed in an inner loop, so a busy stream would otherwise keep the caller
inside one call indefinitely. The same bound fixes an existing fault: when
every sensor went silent, stream health stopped being published at all.

**The intake is rebuilt when the configuration changes.** What it was built
from — the sender mapping, the model, the site tuning — is exactly what the
installation writes. The configuration carries a generation counter, the intake
retires when it changes, and a supervisor rebuilds it. Without this, pairing a
node would have had no visible effect until the appliance was restarted, and
the wizard would appear stuck at the moment it was working correctly.

## Consequences

Stream health now reports `running` and `estimating` separately: `running`
alone could not mean both "the intake is reading" and "estimates are coming".
Which of the two reasons applies — an unfinished installation or closed service
hours — is answered by the readiness and the service state that the status
already carries.

The read timeout is an explicit, defaulted-off field on the source
configuration rather than a change of shared behaviour: four other tools read
the same sources to the end of a stream, and a heartbeat would only interrupt
them.
