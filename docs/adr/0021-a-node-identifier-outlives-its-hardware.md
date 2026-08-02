# A node identifier outlives its hardware

- Status: accepted
- Date: 2026-08-02

## Context and Problem Statement

Sensing nodes fail. A receiver stops streaming, an installer screws a fresh one
to the same bracket, and the appliance has to adopt it. Until now the only way
to do that was the installation wizard, which pairs from scratch: it discovers
what is streaming and numbers the receivers in the order they were first heard
(ADR 0016).

That is the right behaviour for a first installation and the wrong one for a
repair, for a reason that is not visible from the pairing screen.

## Considered Options

- Re-run the full pairing from the settings, as the wizard does.
- Replace one node at a time, keeping its identifier.
- Let the operator edit the identifier freely.

## Decision Outcome

**Replacement targets one node and keeps its identifier.** The appliance offers
the senders it can hear that are not already paired, and the operator says which
one is now `rx-2`.

The identifier is not a label. It is:

- the key capture sessions are written against, in `meta.json` and in every
  frame of `csi.ndjson`;
- **the list a density model was validated for.** A bundle is accepted only if
  the pipeline it would drive builds against this appliance's receivers
  (ADR 0020), so a receiver that comes back as `rx-3` instead of `rx-2` leaves
  the site holding a model that no longer fits — after a hardware swap that had
  nothing to do with the model.

Re-running the full pairing renumbers by order of first appearance, which is
exactly what a replacement changes: the new node is heard last. The failure is
silent at the moment it happens and only shows up as a refused model, which is
the worst possible distance between cause and symptom.

Editing identifiers freely was rejected for the same reason from the other
direction: it makes the constraint the operator's to remember.

## Where a sensor sits belongs to the installation

The same reasoning settles a second question. A node's physical position was
asked for once per recording, as part of the capture request. But a sensor
screwed to a wall does not move between recordings, so the question was either
retyped or — in practice — left blank, and the recorded sessions carried
nothing.

The position is therefore a property of the paired node, described once from
the sensors screen and copied into every session recorded afterwards. A capture
may still override it, for the case the stored answer is wrong that day, and a
blank override does not erase what the installation knows: an untouched field
is not a statement that the sensor has no position.

## Consequences

Adoption is per role, because each role is known by exactly one thing: a
receiver by its source address (ADR 0007), a transmitter by the MAC the
receivers report having sensed. Offering the wrong one is refused by name
rather than ignored.

The replacement offer excludes senders that are already paired. Offering a
working sibling is how an installation ends up with two identifiers pointing at
one sensor, which validation would then refuse in a message about duplicate
addresses rather than about the mistake that was made.

Nothing here weakens the whole-list write the wizard uses: identifiers,
addresses and MACs must still be unique across the set, and only one node may
transmit. A single-node change is validated against the same rules, because it
is applied to a copy of the configuration and the copy is what gets checked.
