# Architecture

Mariam Flow estimates, in real time, the waiting time of a queue (typically
at a university restaurant) using Wi-Fi sensing. Human bodies disturb the
multipath propagation of Wi-Fi radio waves; Channel State Information (CSI)
captures the amplitude and phase of the channel per OFDM subcarrier and
therefore encodes those disturbances. The system classifies the monitored
zone into four density classes and converts density into a waiting time with
Little's Law.

The system is device-free by design: no cameras, no personal data, no
interaction required from the people in the queue.

## System overview

```
[Queue zone]   ESP32-C6 TX ──(Wi-Fi traffic)──> ESP32-C6 RX ×2
                                   │ UDP (raw CSI frames)
                                   ▼
                       Edge aggregator (Linux)
                       ┌──────────────────────────────────────┐
                       │ flow-ingest   parse → ring buffer →  │
                       │               session storage        │
                       │ flow-infer    features → ONNX model  │
                       │               → density → wait time  │
                       │ flow-api      local REST + push      │
                       └──────────────────────────────────────┘
                                   │ HTTPS — aggregated estimates only:
                                   │ {wait, class, confidence, timestamp}
                                   ▼
                             Backend / display layer
```

**Privacy invariant: raw CSI never leaves the site.** Only aggregated
estimates (waiting time, density class, confidence, timestamp) are pushed
upstream.

## Components

| Path | Role | Status |
|---|---|---|
| `crates/flow-core` | Canonical domain types: CSI frames, density classes, labels, session metadata | Implemented |
| `crates/flow-ingest` | UDP intake of CSI frames, ring buffering, immutable on-disk session storage | Placeholder |
| `crates/flow-infer` | Sliding-window features, ONNX inference (`tract`), Little's Law, output smoothing | Placeholder |
| `crates/flow-api` | Local REST API (`axum`): live estimate, sessions, control; outbound push | Placeholder |
| `ml/` | Python package (`flow_ml`): session loading, feature engineering, training, ONNX export | Scaffold |
| `firmware/csi-node` | C / ESP-IDF firmware for ESP32-C6 nodes, based on `espressif/esp-csi` | Placeholder |
| `tools/labeler` | Web app for live ground-truth labeling during calibration | Not created yet |

The edge components are plain Rust binaries with no board-specific
dependency; any Linux/macOS machine can play the edge role during
development. The production cross-compilation target is
`aarch64-unknown-linux-gnu`.

## Data model

`flow-core` is the single source of truth for the data model. Its
serialization (serde/JSON) defines the canonical on-disk session format:

```
data/sessions/<session_id>/
├── meta.json        # SessionMeta: site, node placement, Wi-Fi channel,
│                    # firmware/software versions, environment description,
│                    # site-specific class mapping
├── csi.ndjson       # one CsiFrame per line:
│                    # {ts_us, node_id, rssi, mcs, len, amp[], phase[]}
└── labels.ndjson    # one Label per line: {ts_us, class, count?}
```

Format rules:

- Timestamps are microsecond-resolution Unix timestamps (`ts_us`), assigned
  by the edge at frame reception — node clocks are not trusted.
- One session is one continuous capture; recorded sessions are immutable.
- `count` is the exact people count measured by a reference sensor during
  supervised calibration; when present, `class` is derived from it using the
  site-specific thresholds recorded in `meta.json`. Manually produced labels
  omit `count`. Storing the raw count keeps class boundaries re-derivable
  without recapturing sessions.
- Frames deserialized from a trust boundary (network, disk) must pass
  `CsiFrame::validate` before use; serde alone does not enforce structural
  invariants.
- `csi.ndjson` is the v0 human-readable format; migration to Parquet is
  planned once volumes require it.

### Density classes

The classifier output space is frozen at four classes, encoded as integers:

| Value | Class | Meaning |
|---|---|---|
| 0 | `empty` | No detectable presence |
| 1 | `low` | Sparse presence, no meaningful queue |
| 2 | `medium` | Established queue, moderate density |
| 3 | `saturated` | Zone at or near capacity |

What each class concretely means at a given site (e.g. person counts in a
lab, queue landmarks in a restaurant) is recorded per session in the
`class_mapping` of `meta.json`.

The classifier's output is a probability distribution over the four classes,
not just the most likely class. The discrete class is the argmax; downstream
consumers (smoothing, wait-time estimation) operate on the full distribution,
which provides a continuous density signal at no extra cost. Classes are
discrete because the supervision signal is: ground truth comes either from a
human selecting one of four levels or from counts bucketed by documented
thresholds, and coarse ordered classes match the effective resolution of
indoor CSI, which saturates as zone density grows.

## ML-to-production bridge

Models are trained in Python (`ml/`) and exported to ONNX; the edge runs
inference in Rust through `tract`, so production carries no Python runtime.
Every exported model must pass a Python↔Rust parity test: identical inputs
must produce identical outputs within a 1e-5 tolerance.

Evaluation uses cross-validation grouped by session — a single session is
never split between train and test, as adjacent windows of the same capture
are too similar and would leak.

## Wait-time estimation (design)

Density is converted to a waiting time with Little's Law, W = L / λ:

- each density class maps to an estimated number of people via a per-site
  calibrated mapping; the expected count E[L] is computed over the
  classifier's probability distribution, so the wait-time estimate evolves
  continuously instead of jumping at class transitions;
- λ is the service rate (people served per minute), calibrated per time
  slot;
- the output is smoothed (exponential moving average) with hysteresis on
  class transitions to avoid flapping.

This layer lives in `flow-infer` and is not implemented yet.

## Toolchain and quality gates

- **Rust**: stable toolchain, edition 2024. CI enforces `cargo fmt --check`,
  `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and a
  cross-compilation check for `aarch64-unknown-linux-gnu`. Errors are typed
  with `thiserror`; `unwrap()`/`expect()` are confined to tests.
- **Python**: `uv`-managed environment (Python ≥ 3.12). CI enforces
  `ruff check`, `ruff format --check`, `pyright` (strict), and `pytest`.
- **Docs**: this file tracks the implemented state of the system; structural
  decisions are recorded as ADRs under `docs/adr/`.
