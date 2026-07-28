# Dedicated sensor access point on a separate radio from the site uplink

- Status: accepted
- Date: 2026-07-28

## Context and Problem Statement

An installed appliance has to do two incompatible network jobs at once. It
hosts the access point the sensing nodes join — which must sit on a fixed
2.4 GHz channel, the same channel the transmitter uses, because a station
only senses CSI on the channel it is associated with (ADR 0005, ADR 0007).
And it should reach the site's network, whose channel, band and security
are decided by someone else entirely.

The obvious answer, one radio doing both (concurrent AP + station), does not
survive contact with the hardware. The Broadcom/Cypress driver used by the
target board reports a single-channel constraint for concurrent interfaces:
the access point is forced onto the station's channel and follows it if the
station roams. Firmware crashes in concurrent AP+STA mode are documented on
recent kernels for this chipset family.

Applied here that is not an inconvenience, it is a correctness failure. If
the access point follows the site network onto another channel, every
receiver follows it too, the transmitter is left alone on channel 6, and
CSI capture stops — silently, with no error anywhere, just a stream of
frames that never arrives.

## Considered Options

1. One radio, concurrent AP + station on a shared channel
2. Retune the whole sensing system — transmitter and access point — onto
   the site network's channel
3. Sanctuarise the built-in radio for the sensor access point; reach the
   site network through a second interface
4. Wired uplink only

## Decision Outcome

Chosen option 3. The built-in radio serves the sensor access point and
nothing else, on a fixed channel; the site uplink runs on a second
interface — a USB Wi-Fi adapter, or a USB Ethernet adapter where the site
prefers wire. Both are configured through one code path, differing only in
interface and link-layer settings, so supporting wire costs almost nothing
on top of supporting Wi-Fi.

Options 1 and 2 both subordinate the measurement to the site's network.
Option 2 additionally makes sensing quality hostage to whatever channel the
site happens to use, including a congested one, and turns any change on the
site's side into a re-provisioning of the transmitter firmware. Option 4 is
the most robust link but cannot be a requirement: sites do not always have a
usable outlet near the queue, and the appliance must be installable
without one.

Two consequences of this shape are worth recording:

- **A dual-band adapter puts the uplink on 5 GHz**, leaving the 2.4 GHz
  band entirely to sensing. The interference argument that motivates a
  dedicated channel is then satisfied by construction rather than by
  configuration.
- **Offline is a first-class mode, not a failure state.** With no uplink at
  all, sensing, calibration and local display work unchanged; only remote
  supervision and the outbound push are unavailable. The configuration
  therefore distinguishes *no decision yet* from *deliberately offline* —
  the guided installation must be able to tell an unanswered question from
  an answered one.

### Consequences

- Good: the sensor network's channel is guaranteed stable for the life of
  the installation, independent of anything the site does to its own
  network; sensing cannot be broken by a remote configuration change.
- Good: one uplink abstraction covers Wi-Fi and Ethernet, so a site that
  demands wire is a configuration choice rather than a port of the network
  layer.
- Bad: a second network adapter becomes part of the bill of materials and
  of the installation instructions. Adapter choice is constrained to
  chipsets with in-kernel drivers, since out-of-tree drivers would have to
  be rebuilt on every kernel update — an unacceptable failure mode for
  unattended units.
- Bad: two interfaces mean routing and firewall rules must be explicit, so
  that the sensor network stays isolated from the site network. The
  appliance bridges nothing between them; that isolation is a property the
  network handout given to site administrators asserts, and it has to
  remain true.
