# The appliance publishes the estimate itself

- Status: accepted
- Date: 2026-08-03
- Supersedes, in part: [ADR 0008](0008-single-process-edge-appliance-daemon.md)

## Context and Problem Statement

ADR 0008 kept the public estimate contract in `flow-api`, a small crate that
was deliberately narrow, and made `flow-edge` the daemon an installed site
runs. The reasoning was about ownership: diluting the contract crate with
network configuration, file management and an embedded web application would
leave neither concern clearly owned.

What that left is an appliance that measures a waiting time and has no way to
tell anyone. Every route `flow-edge` serves is administration behind a session;
the one route the product exists for lives in a binary the appliance does not
run. A hall display, or the push to the menu system later, has nothing to read.

## Considered Options

1. Run `flow-api` alongside `flow-edge` on the appliance.
2. Move the contract into `flow-edge` and keep `flow-api` as a laboratory tool.
3. Move the contract into `flow-edge` and remove `flow-api`.

## Decision Outcome

**Option 3.** `flow-edge` serves `GET /estimate`, and `flow-api` is removed.

Option 1 was rejected for the reason ADR 0008 gave for rejecting a
multi-process appliance in the first place: two processes sharing the one
frame stream and the one estimate would need the coordination that decision
exists to avoid.

Option 2 is the one worth arguing about, and it was rejected on evidence rather
than taste. A contract with two implementations drifts, and this codebase has
just paid for that: the rule deciding whether a sensor is silent existed twice,
and the older copy — still driving two screens — could never report a silent
sensor on a single-receiver installation. A public contract is exactly the kind
of thing where the second copy is discovered by the consumer, not by us.

The narrowness ADR 0008 wanted to protect is preserved by the route rather than
by the crate: it takes no credential, reads no request body, and returns four
fields.

## Three states, because two would lie

The contract distinguishes **closed** from **unavailable**. A site outside its
service hours is not a fault; collapsing the two would leave every display in
every hall announcing a breakdown all night, and an audience that has learned
to ignore the message will ignore the real one too.

An estimate is published only when it is reliable and fresh. Freshness is not a
refinement: an estimate outlives the window it was computed from, so a stream
that has died leaves the last good number looking true for as long as nobody
replaces it. Ninety seconds is the bound — a queue changes on the scale of a
minute, and anything older describes a hall that has since emptied or filled.

A refusal carries no reason. This surface is read by a display, and why the
appliance cannot estimate is an operator's question that the dashboard and the
journal both answer.

## Consequences

This is the only route on the appliance that answers without a session, and the
only one carrying `Access-Control-Allow-Origin`. A display is not hosted by the
appliance, so a browser reading it comes from another origin; the route takes no
credential and publishes what is meant to be on a screen. The dashboard stays
strictly same-origin, and no other route gains a CORS header.

The privacy invariant is unchanged and now asserted rather than assumed: a test
pins the exact field set of the payload, so a measurement cannot be added to it
without a test failing.

`csi-capture`, `csi-infer` and `csi-replay` remain what ADR 0008 said they were
— laboratory tools that exercise one stage of the chain in isolation. Only the
crate that duplicated a contract is gone.
