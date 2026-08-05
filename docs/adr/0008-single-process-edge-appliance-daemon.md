# Single-process edge appliance daemon

- Status: accepted
- Date: 2026-07-28

## Context and Problem Statement

Until now the edge was a set of laboratory tools: `csi-capture` records a
labeled session, `csi-infer` runs the pipeline over a stream, `flow-api`
serves the live estimate. Each is started by hand, from a shell, with the
right flags — which suits development and suits nothing else.

An installed site needs the opposite: one thing that boots when the unit is
powered on, holds the configuration of that site, walks a non-technical
installer through setting the system up, and then runs unattended. That
software has to own state the existing tools deliberately do not: which
nodes are paired, how far the installation has got, which model is active,
how the unit reaches the site network.

The question is how to package it — and it is constrained by the deployment
target, a single-board computer with 512 MB of RAM and an SD card for
storage.

## Considered Options

1. One daemon that embeds the existing crates as libraries
2. Grow `flow-api` into that daemon
3. Two services — a pipeline engine and a dashboard process — talking over
   local IPC
4. Orchestrate the existing binaries from a supervisor, one process per
   activity

## Decision Outcome

Chosen option 1: a new `flow-edge` crate, the product daemon, which depends
on `flow-ingest`, `flow-infer` and `flow-capture` as libraries and adds what
only a deployed appliance needs — configuration, installation lifecycle,
dashboard, network integration.

The decisive argument is shared, contended state rather than resource
frugality. Guided installation, calibration and live inference all consume
the *same* UDP stream from the receivers, and only one may hold it at a
time. Arbitrating that across process boundaries (options 3 and 4) means
inventing a protocol to express "the stream is busy" and keeping two copies
of the appliance's state in agreement — real complexity bought for no gain.
In one process it is a guard over one value, enforced by the type system.
Option 4 additionally makes lifecycle management a shell-scripting problem
on a machine nobody logs into.

Option 2 was rejected on naming and scope rather than architecture:
`flow-api` is the crate that defines the *public estimate contract*, small
and deliberately narrow. Diluting it with network configuration, file
management and an embedded web application would leave neither concern
clearly owned. `flow-api`, `csi-capture`, `csi-infer` and `csi-replay`
remain as they are — development and laboratory tools, still the fastest
way to exercise one stage of the chain in isolation.

Two structural choices inside the daemon follow from the same reasoning:

- **Configuration is one validated JSON file; history is not.** Recovering
  an appliance sometimes means reading or repairing its configuration from
  a serial console, or from the SD card pulled out of a unit that no longer
  boots — a text file survives that, a database does not. Time series
  (node health, estimates, events) have the opposite needs, so they go to
  SQLite where queries and retention belong. Writes are atomic
  (temporary file, then rename) and validation happens before the write, so
  neither a power cut nor an invalid value can leave an appliance unable to
  boot.
- **Installation progress is derived, not stored.** The wizard's current
  step is computed from facts — is the site named, are nodes paired, has
  the uplink question been answered, is a model ready — rather than kept as
  a cursor that can drift away from what is actually configured. An
  appliance interrupted mid-installation resumes exactly where reality says
  it is. One bit is stored, `onboarding_completed`, so that a finished
  installation does not fall back into the wizard because a node is
  temporarily unplugged.

### Consequences

- Good: one unit to start, supervise, update and reason about; stream
  exclusivity enforced in the type system rather than by convention; a
  single memory footprint on a 512 MB target; the domain crates stay
  library-shaped and independently testable.
- Good: a truncated or invalid configuration is a loud startup failure
  naming the file and the offending value, not a half-configured system.
- Bad: a panic takes the whole appliance down rather than one component,
  making the process supervisor's restart policy load-bearing. The blast
  radius of a change is wider, which raises the bar on tests around the
  shared state.
- Bad: some duplication between the daemon's persisted configuration and
  the ad-hoc `site.json` the laboratory tools read. Kept deliberately
  field-for-field identical so a tuning produced in the lab can be moved
  into an appliance unchanged. Removed by ADR 0023: the two no longer hold
  the same fields.
