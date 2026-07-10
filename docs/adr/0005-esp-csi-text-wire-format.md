# Node-to-edge frame format: esp-csi text lines

- Status: accepted
- Date: 2026-07-10

## Context and Problem Statement

Sensing nodes must stream CSI measurements to the edge aggregator. The
format of that stream determines firmware complexity, edge parsing,
debuggability, and — critically — how early real captures can happen: the
node firmware is derived from `espressif/esp-csi`, whose stock examples
already emit one `CSI_DATA` CSV text line per measurement over the serial
port, with a chip-dependent column layout (the ESP32-C6 family uses a
different column set than the other ESP32 variants).

## Considered Options

1. Reuse the esp-csi text line format, transport-agnostic
2. A custom versioned binary format
3. JSON per frame

## Decision Outcome

Chosen option 1: the edge parses `CSI_DATA` lines regardless of transport —
serial capture, recorded file, or (later) one line per UDP datagram. Both
esp-csi column layouts are supported, detected from the column count, so
captures from any esp-csi chip can be replayed through the same parser.

Consequences of this choice drove it:

- The very first capture requires **zero firmware modification**: a stock
  esp-csi receiver example plus a serial connection feeds the pipeline.
  The later UDP step only relays the same lines.
- The stream is human-readable end to end (serial monitor, `tcpdump`),
  which matters while the radio setup is being debugged.
- Recorded serial logs — including third-party ones — double as parser
  test fixtures and development data.

At the target rates (on the order of 100 frames/s per node, lines around
1 KB), the ~2× size overhead of text over a packed binary encoding
(option 2) is irrelevant, and option 2 would require writing and debugging
custom firmware serialization before any capture is possible. Option 3
inherits text verbosity without matching what the firmware already emits.
A binary format remains a possible later optimization if measured
constraints ever justify it.

### Consequences

- Good: hardware-day-one captures with stock firmware; debuggable stream;
  replayable fixtures; a single parser for all chip variants.
- Bad: the layout is defined upstream by esp-csi — an upstream change
  requires a parser update (guarded by unit tests and an explicit
  unknown-layout error); text parsing sits on the ingestion path, which is
  trivial at target rates but worth re-measuring if rates grow.
