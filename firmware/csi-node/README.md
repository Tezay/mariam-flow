# csi-node firmware

**Status: placeholder — no firmware code yet.**

Planned scope: a C / ESP-IDF application for ESP32-C6 sensing nodes, based on
[`espressif/esp-csi`](https://github.com/espressif/esp-csi). A single binary
covers both radio roles, selected at build time via `sdkconfig`:

- **TX** — dedicated transmitter generating the reference Wi-Fi traffic on a
  fixed channel.
- **RX** — receiver extracting CSI from that traffic and streaming raw frames
  over UDP to the edge aggregator.

Nodes carry no other logic: parsing, storage, inference, and the API all live
in the edge software (see `docs/architecture.md`).

Code derived from `esp-csi` retains its Apache License 2.0 notices; see
`LICENSE.md` at the repository root.
