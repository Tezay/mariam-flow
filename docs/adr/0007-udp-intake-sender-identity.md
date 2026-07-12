# UDP intake: sender-identified nodes, reception-time stamping

- Status: accepted
- Date: 2026-07-12

## Context and Problem Statement

In production, sensing nodes stream CSI frames to the edge over UDP
(ADR 0005 fixed the payload as esp-csi text lines, transport-agnostic).
Three questions remained open: how lines map onto datagrams, how the edge
knows *which receiving node* a frame comes from — the MAC inside a line
identifies the transmitter of the sensed packet, never the receiver — and
how the interleaved streams of several RX nodes merge into one ordered
frame sequence.

## Considered Options

1. One line per datagram; RX identity from the datagram source address;
   timestamps assigned at reception
2. Extend the payload with an explicit node identifier field
3. One socket per node (port-based identity)
4. Node-clock timestamps with a reordering buffer at the edge

## Decision Outcome

Chosen option 1, three decisions in one:

- **Framing**: one `CSI_DATA` line per datagram. Datagram boundaries are
  the line boundaries; the firmware sends exactly the bytes it prints on
  serial. No payload change (option 2 would fork the wire format and the
  parser for a value the network layer already provides).
- **Identity**: the receiving node is identified by the datagram's source
  address, resolved through an explicit mapping
  (`rx-1=192.168.4.11`, with an optional `ip:port` form for setups that
  share one IP). Unknown senders are counted and dropped — a
  misconfigured or foreign device cannot pollute a session. Option 3
  (port per node) multiplies sockets and configuration for no benefit.
- **Time**: frames are stamped by the edge clock at reception, clamped
  monotonically non-decreasing against system-clock steps. Since every
  node lands on one socket stamped by one clock, the merged multi-node
  stream is ordered *by construction* — the reordering buffer of
  option 4 becomes unnecessary, along with the cross-node clock
  reconciliation it would require. (Node-local timestamps remain used for
  *replayed* line captures, where reception time is meaningless.)

The same robustness policy as the line reader applies: unknown senders,
non-frame datagrams and malformed lines are counted, never fatal.

### Consequences

- Good: zero firmware format work beyond redirecting output to a socket;
  multi-RX sessions with a single socket and no reordering logic;
  per-node frame-loss statistics preserved.
- Bad: source-address identity requires stable node addresses (DHCP
  reservations or static IPs — a deployment requirement) and assumes a
  flat LAN between nodes and edge (NAT would break identity; the
  architecture already mandates an on-site edge). Reception-time
  stamping folds network jitter into timestamps — negligible on a LAN
  against the 5-second analysis windows.
