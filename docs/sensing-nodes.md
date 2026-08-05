# Sensing nodes

Three ESP32-C6 nodes frame the queue zone: one transmitter generating
reference traffic, two receivers extracting CSI from it. The firmware is a
single application whose role is selected at build time, based on the
`espressif/esp-csi` examples.

## Placement

The transmitter sits on one side of the monitored zone and the receivers on
the other, so each radio path crosses it. Nodes are mounted at the same height,
antennas clear, on a non-metallic support, two to four metres apart with no
solid obstacle between them.

The transmitter's channel must equal the channel of the access point the
receivers join: a station senses CSI only on the channel it is associated with.

## Pairing

An installer arrives with pre-flashed nodes and an appliance that knows nothing
about them, and the firmware only streams. Identity is therefore read off the
stream, with the two roles found in different places (ADR 0016). A receiver is
a source address sending CSI datagrams. The transmitter never joins the access
point and appears only as the MAC inside those datagrams.

The transmitter is found by agreement: a receiver reports every transmitter it
sensed, so one receiver's list may hold a passing laptop, while the MAC several
receivers share is the one lighting the room they both watch. How many agreed
travels with the offer.

What comes out is a proposal the installer confirms. Which physical box is
`rx-1` is not something the stream can say, and it is what matters when one of
them later goes quiet. Identifiers are offered in the order the senders were
first heard, skipping any already in use.

Observation is passive and permanent rather than a mode: `GET /api/discovery`
answers at any time, so re-pairing never means stopping the estimation. The
table is bounded — sixteen senders, a fixed number of datagrams parsed per
sender — because anyone reaching the intake socket can create an entry.

A receiver's MAC is optional, being known only from a DHCP lease. A
transmitter's is required, being the only thing it can be known by.

Fewer receivers than the design expects is reported, never enforced: a second
receiver may be installed later, and refusing to continue would equally block
repairing an installation that has lost one.

## Replacement

A failed node is replaced one at a time, keeping its identifier (ADR 0021).
The identifier is what capture sessions are written against and what a density
model was validated for, so a receiver renumbered by a repair would leave the
site holding a model that no longer fits it.

The sensors screen offers the senders the appliance can hear that are not
already paired, and the operator says which one is now `rx-2`. A receiver is
adopted by address and a transmitter by MAC — each role is known by exactly one
thing.

Where a sensor sits is a property of the installation rather than of one
capture: it is described once and copied into every recording afterwards.

## Health

Silence is measured against the newest frame of the whole stream, or the clock
when frames carry appliance timestamps, whichever is later. Against the stream
alone a node cannot lag itself, so an installation with one receiver could never
report it silent, and one where every receiver stopped would report them all
healthy.

Sensors appearing and going quiet are journalled, so the question the screen
answers in the present has an answer in the past.
