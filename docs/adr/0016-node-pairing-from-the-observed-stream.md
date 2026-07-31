# Node pairing from the observed stream

- Status: accepted
- Date: 2026-07-30

## Context and Problem Statement

An installer arrives with a kit of pre-flashed nodes and an appliance that
knows nothing about them. The appliance has to learn which nodes exist, which
role each plays, and which address each streams from — with no keyboard, no
screen on the nodes, and firmware that only streams.

The two roles are not symmetric, and that is the crux. A receiver sends UDP
datagrams, so it is visible as a source address. The **transmitter never joins
the access point at all**: it only radiates the packets the receivers sense. It
cannot be seen directly by anything on the network.

## Considered Options

- **Type it in.** The installer enters each node's address and MAC by hand
  from the kit's paperwork.
- **Provision the nodes.** Add a protocol — BLE, or a serial handshake — by
  which each node announces itself.
- **Read it off the stream.** Derive identity from the traffic the nodes
  already produce.

## Decision Outcome

**Read it off the stream**, with the two roles found in different places:

- a **receiver** is a source address sending CSI datagrams;
- the **transmitter** is the MAC that appears *inside* those datagrams, found
  by which MAC several receivers agree on.

Agreement is what makes the second half work. A receiver reports every
transmitter it sensed, so any single receiver's list may contain a passing
laptop. The MAC that several receivers have in common is the one lighting up
the room they both watch. The number of receivers that agree travels with the
offer, so a screen can distinguish a transmitter seen by one node from one
seen by all of them.

Typing it in was rejected as the thing this exists to avoid: it is the step
that goes wrong on site, and the appliance can know the answer without asking.
Node provisioning was rejected for this milestone because it means firmware
work, and the plan already places BLE commissioning in a later phase — the
stream is available today at no cost.

Three consequences follow, each chosen deliberately.

**The result is a proposal, not a decision.** Which physical box is `rx-1` is
not something the stream can say, and it is exactly what an installer needs
months later when one of them goes quiet. The appliance offers identifiers in
the order the senders were first heard — the order the installer powered them
in — and the installer confirms. Identifiers already in use are skipped, so a
node replaced on a running installation is offered the first free one.

**Observation is passive and permanent, never a mode.** Replacing a node on a
live installation must not require stopping the estimation to find its
replacement. The intake records unmapped senders continuously, so pairing is
available from the settings screen at any time, not only during installation.

**A receiver's MAC is optional.** Its identity at intake is its source address
(ADR 0007); the MAC is known only from a DHCP lease, which an appliance being
installed may not have yet. A transmitter's MAC is required, because it is the
only thing a transmitter can be known by. Requiring both of both, as the
configuration first did, contradicted the identity model it was built on.

## Consequences

The observation table is **bounded** — sixteen senders, and a fixed number of
datagrams parsed per sender. Anyone able to reach the intake socket can create
an entry, so these are memory guarantees on a 512 MB appliance rather than
tuning choices. Entries already held keep updating when the table is full, so a
flood cannot freeze the pairing screen. Senders below a minimum datagram count
are not offered at all: a sensing node streams continuously, and offering a
handful of stray datagrams as a node would invite an installer to accept it.

Ties in the transmitter vote break on the MAC itself, so the same observations
always produce the same offer rather than following hash order.
