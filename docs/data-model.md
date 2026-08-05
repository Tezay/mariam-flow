# Data model

`flow-core` owns the domain types and their serialization. Its serde output
defines the canonical on-disk session format.

## Session format

```
data/sessions/<session_id>/
├── meta.json        # SessionMeta: site, node placement, Wi-Fi channel,
│                    # firmware/software versions, environment description,
│                    # site-specific class mapping
├── csi.ndjson       # one CsiFrame per line:
│                    # {ts_us, node_id, rssi, mcs, len, amp[], phase[]}
└── labels.ndjson    # one Label per line: {ts_us, class, count?}
```

Rules:

- Timestamps are microsecond-resolution Unix timestamps (`ts_us`), assigned by
  the edge at frame reception. Node clocks are not trusted.
- One session is one continuous capture, and a recorded session is immutable.
  A running capture carries a `.recording` suffix and is renamed atomically on
  finalization, so a truncated capture is distinguishable from a clean one.
- `csi.ndjson` and `labels.ndjson` are append-ordered by `ts_us`. The writer
  rejects out-of-order timestamps, structurally invalid frames, and frames from
  nodes not declared in `meta.json`.
- `count` is an exact people count from a reference sensor during supervised
  calibration. When present, `class` is derived from it using the thresholds in
  `meta.json`; manually produced labels omit it. Keeping the raw count leaves
  class boundaries re-derivable without recapturing.
- Frames deserialized from a trust boundary must pass `CsiFrame::validate`
  before use. Serde alone does not enforce the structural invariants.
- `csi.ndjson` is the v0 human-readable format. Migration to Parquet is planned
  once volumes require it.

## Node-to-edge frame format

Sensing nodes emit one `CSI_DATA` text line per measurement, in the format of
the stock `esp-csi` examples (ADR 0005). `flow-ingest` parses these lines
regardless of transport — serial capture, recorded file, UDP datagram — and
supports both esp-csi column layouts (ESP32-C6 family and classic ESP32,
detected from the column count). Raw interleaved I/Q values are converted to
per-sub-carrier amplitude and phase, a bijective mapping. Two edge rules are
applied at conversion: timestamps come from the edge, and node MAC addresses
are mapped to logical `node_id`s from configuration.

Non-frame lines and malformed frames are counted and skipped, never fatal to a
capture. Frame loss is inferred from gaps in per-transmitter sequence numbers
and exposed as stream statistics, which back the frame-loss quality metric of
recorded sessions.

In production, nodes stream over UDP — one `CSI_DATA` line per datagram
(ADR 0007). The receiving node is identified by the datagram's source address
through an explicit mapping; unknown senders are counted and dropped. Frames
are stamped by the edge clock at reception with a monotonic clamp, so the
merged multi-node stream is ordered by construction.

The capture and inference tools accept every transport through one input
specification — a file, `-` for stdin, or `udp://ADDR:PORT`:

```sh
csi-capture --input udp://0.0.0.0:5566 \
            --node rx-1=192.168.4.11 --node rx-2=192.168.4.12 \
            --meta meta.json
csi-infer   --input udp://0.0.0.0:5566 --node rx-1=192.168.4.11 \
            --node rx-2=192.168.4.12 --model model.onnx --config site.json
```

`csi-replay` turns any stream of `CSI_DATA` lines into a canonical session
directory. It filters frames by transmitter MAC to exclude ambient traffic,
reconstructs monotonic edge timestamps from the node's wrapping 32-bit local
clock while preserving real inter-frame timing, and writes through the session
writer's invariant checks:

```sh
csi-replay --input capture.txt --meta meta.json --node-id rx-1 \
           --tx-mac aa:bb:cc:dd:ee:ff
cat /dev/ttyUSB0 | csi-replay --input - --meta meta.json --node-id rx-1
```

## Density classes

The classifier output space is frozen at four classes, encoded as integers:

| Value | Class | Meaning |
|---|---|---|
| 0 | `empty` | No detectable presence |
| 1 | `low` | Sparse presence, no meaningful queue |
| 2 | `medium` | Established queue, moderate density |
| 3 | `saturated` | Zone at or near capacity |

What each class means at a given site is recorded per session in the
`class_mapping` of `meta.json`, and answered once per site on the appliance
(ADR 0023).

The classifier's output is a probability distribution over the four classes,
not only the most likely one. The discrete class is the argmax; smoothing and
wait-time estimation operate on the full distribution, which provides a
continuous density signal (ADR 0002). Classes are discrete because the
supervision signal is: ground truth comes from a human selecting one of four
levels, or from counts bucketed by documented thresholds.

One palette carries these classes wherever they are drawn — the labelling
page, the Python session portraits, the dashboard. It reads as a status ramp,
and every adjacent pair stays distinguishable in normal vision and under
simulated colour-vision deficiency. One step sits below the 3:1 contrast floor
against a light surface, so a class is never shown by colour alone: its name is
written beside it, and every chart has a table view.
