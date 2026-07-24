# csi-node firmware

C / ESP-IDF application for ESP32-C6 sensing nodes, based on the
[`espressif/esp-csi`](https://github.com/espressif/esp-csi) get-started
examples (Apache License 2.0, notices preserved). One binary covers both
radio roles, selected in `menuconfig`:

- **TX** — overrides its MAC with the fixed reference address
  `1a:00:00:00:00:00` (same as esp-csi) and blasts ESP-NOW broadcast
  frames on a fixed channel at a configurable rate.
- **RX** — joins the edge's Wi-Fi network, extracts CSI from the TX's
  frames (sender-MAC filtered), formats one `CSI_DATA` line per
  measurement (the C6 15-column layout the edge parser expects), and
  sends **one line per UDP datagram** to the edge (ADR 0005/0007).
  Optionally echoes lines on the serial console — the zero-network,
  day-one capture path (`csi-capture --input -`).

> **Status: written, not yet compiled or flashed.** The code follows the
> esp-csi examples and ESP-IDF 5.x APIs but must be validated on real
> hardware; `fft_gain`/`agc_gain` field locations in `wifi_pkt_rx_ctrl_t`
> are the most likely IDF-version-sensitive spots.

## Design notes

- **Channel constraint**: a station captures CSI on the channel it is
  associated on. The TX channel must therefore match the access point
  the RX nodes join — typically an AP hosted by the edge device, which
  also gives the nodes stable addresses (the edge identifies RX nodes by
  source IP, ADR 0007).
- The CSI callback runs in the Wi-Fi task: it only formats and enqueues;
  a dedicated task does the UDP sends. A full queue drops lines (counted)
  — losing a frame is acceptable and measured end-to-end via the
  sequence numbers; blocking the Wi-Fi task is not.
- Power save is disabled (`WIFI_PS_NONE`): it decimates the capture rate.
- Raw CSI I/Q values are emitted as-is (no gain compensation), matching
  what the edge stores and the models consume.

## Build and flash

Requires [ESP-IDF](https://docs.espressif.com/projects/esp-idf/) ≥ 5.3
(the official VS Code extension installs everything).

```sh
cd firmware/csi-node
idf.py set-target esp32c6
idf.py menuconfig          # CSI node configuration → role + parameters
idf.py build
idf.py -p /dev/ttyUSB0 flash monitor
```

Flash one board as TX and the others as RX (set SSID/password, edge
address, and keep the TX channel equal to the AP channel).

## First-contact checklist (with the edge tools)

1. RX with serial output enabled, no network:
   `idf.py monitor` shows `CSI_DATA,...` lines once the TX runs.
2. Pipe them into the edge: `cat /dev/ttyUSB0 | csi-capture --input - …`
   (or `csi-replay`) — the parser must accept the lines; `parse errors`
   in the stats means a layout mismatch to fix here.
3. Switch to UDP output and verify
   `csi-capture --input udp://0.0.0.0:5566 --node rx-1=<ip> …` sees the
   frames, then a two-node session.
