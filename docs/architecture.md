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
| `crates/flow-ingest` | Frame parsing (esp-csi text format, see ADR 0005), stream reading with loss statistics, immutable on-disk session storage, `csi-replay` capture tool, UDP intake | Parser, stream reader, session writer and replay tool implemented; UDP intake planned |
| `crates/flow-infer` | Window feature extraction (mirror of `flow_ml`), ONNX inference (`tract`), Little's Law, output smoothing, live pipeline and `csi-infer` tool | Full inference chain implemented; REST exposure planned |
| `crates/flow-api` | Local REST API (`axum`): live estimate, sessions, control; outbound push | Live-estimate surface and edge daemon implemented; sessions/control/push planned |
| `ml/` | Python package (`flow_ml`): session loading, feature engineering, training, ONNX export | Loading, windowing, v1 features, training and grouped evaluation implemented; ONNX export planned |
| `firmware/csi-node` | C / ESP-IDF firmware for ESP32-C6 nodes, based on `espressif/esp-csi` | Placeholder |
| `crates/flow-capture` | Labeled capture: session recording plus the phone labeling page (`csi-capture`) | Implemented |

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
  While a capture is running, the directory carries a `.recording` suffix;
  it is atomically renamed on finalization, so a truncated capture is
  always distinguishable from a clean one.
- `csi.ndjson` and `labels.ndjson` are append-ordered by `ts_us`; the
  writer rejects out-of-order timestamps, structurally invalid frames, and
  frames from nodes not declared in `meta.json`.
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

### Node-to-edge frame format

Sensing nodes emit one `CSI_DATA` text line per measurement, in the format
of the stock `esp-csi` examples (ADR 0005). `flow-ingest` parses these
lines regardless of transport (serial capture, recorded file, UDP
datagram), supports both esp-csi column layouts (ESP32-C6 family and
classic ESP32, detected from the column count), and converts raw
interleaved I/Q values into per-sub-carrier amplitude and phase — a
bijective mapping, so nothing is lost. Edge rules are applied at
conversion: timestamps are assigned by the edge, and node MAC addresses
are mapped to logical `node_id`s from configuration.

Ingestion is resilient by policy: non-frame lines and malformed frames are
counted and skipped, never fatal to a capture. Frame loss is inferred from
gaps in per-transmitter sequence numbers and exposed as stream statistics,
which back the frame-loss quality metric of recorded sessions.

The `csi-replay` binary (in `flow-ingest`) turns any stream of `CSI_DATA`
lines — a recorded capture file, or stdin piped from a serial port — into a
canonical session directory: it filters frames by transmitter MAC (ambient
traffic exclusion), reconstructs monotonic edge timestamps from the node's
wrapping 32-bit local clock while preserving real inter-frame timing, and
writes through the session writer's invariant checks:

```sh
csi-replay --input capture.txt --meta meta.json --node-id rx-1 \
           --tx-mac aa:bb:cc:dd:ee:ff
cat /dev/ttyUSB0 | csi-replay --input - --meta meta.json --node-id rx-1
```

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

The v1 exporter (`flow_ml.export`) hand-builds the ONNX graph from the
fitted pipeline using five core operators — `Sub, Div, MatMul, Add,
Softmax` — because tract does not register the `ai.onnx.ml` extension
operators that sklearn-specific converters emit (ADR 0006). The artifact
carries the whole pipeline, standardization included, so the edge cannot
mismatch the normalization. The parity contract is enforced in CI:
`flow_ml.export` writes fixtures (model plus sklearn-computed
probabilities) committed under `crates/flow-infer/tests/fixtures/`, and a
`flow-infer` integration test requires tract to reproduce them within
tolerance. `flow-infer` exposes the result as the full probability
distribution with derived class, confidence, and expected density level
(ADR 0002).

Evaluation uses cross-validation grouped by session — a single session is
never split between train and test, as adjacent windows of the same capture
are too similar and would leak.

### Feature extraction (v1)

`flow_ml` loads sessions with the same validations as the Rust side (the
two implementations are pinned by tests on the same canonical examples),
cuts them into time-based sliding windows (default 5 s, hop 1 s — time, not
frame counts, since the frame rate varies with losses), and computes
per-RX-node statistics per window: mean amplitude, temporal amplitude
deviation, motion energy (frame-to-frame change), mean inter-subcarrier
correlation, RSSI summary, and observed frame rate. Phase is not used in
v1: raw phase from unsynchronized commodity radios needs a dedicated
sanitization step first.

Ground truth is a step function over label timestamps; a window takes the
state at its center. Windows without ground truth, or incomplete for any
declared RX node, are dropped rather than imputed. The result is the
supervised dataset `(X, y)` consumed by training.

Live inference computes the same features in Rust (`flow-infer`), a
deliberate mirror of `flow_ml.features`: a silent divergence between the
two would skew every model output with no error anywhere. The mirror is
pinned by its own parity fixtures (windows of frames plus the
Python-computed reference vectors, 1e-8 tolerance on float64), generated
by `flow_ml.export` together with the ONNX fixtures. Reference vectors are
computed on float32-quantized inputs, since production frames always cross
the f32 `CsiFrame` representation.

### Training and evaluation (v1)

The v1 classifier is a multinomial logistic regression over standardized
features (a scikit-learn pipeline, so scaling parameters are learned on
training folds only). Its canonical output is the probability distribution
over the four classes (ADR 0002); the discrete class is the argmax.

Evaluation is session-grouped cross-validation: sessions are the split
unit, so every window is predicted exactly once by a model that never saw
its session — overlapping windows of one capture are heavily correlated
and would otherwise leak. Reports include accuracy against a
majority-class baseline and the full confusion matrix.

Deterministic synthetic sessions with separable classes
(`flow_ml.synthetic`) validate the pipeline end to end without hardware;
accuracy on them validates plumbing only, never field performance.

## Wait-time estimation

Density is converted to a waiting time with Little's Law, W = L / λ,
implemented in `flow-infer` (`WaitEstimator`):

- each density class maps to an estimated number of people via a per-site
  calibrated mapping; the expected count E[L] is computed over the
  classifier's probability distribution, so the wait-time estimate evolves
  continuously instead of jumping at class transitions;
- λ is the service rate (people served per minute), calibrated per time
  slot; Little's Law assumes a stable regime, and the displayed range is
  what absorbs its degradation while the queue is still building up;
- smoothing is a time-aware exponential moving average — the continuous
  first-order low-pass `τ·ds/dt = x − s` discretized exactly for the
  elapsed time between samples (`α = 1 − e^(−Δt/τ)`), so the time constant
  holds regardless of sampling irregularity;
- the displayed class is stabilized by hysteresis (a Schmitt trigger on
  the smoothed 0–3 level): it only changes when the level crosses a class
  boundary by more than a configured margin, so boundary noise below the
  margin can never make the display flap;
- estimates carry the classifier confidence and a `reliable` flag; the
  publishing layer must not show unreliable estimates.

All parameters (class-to-people mapping, λ, τ, hysteresis margin,
confidence threshold) are per-site configuration, validated at
construction.

## Live inference

`flow-infer` ties the chain together in `LivePipeline`: frames are pushed
in stream order into a trailing time window (`(t − window, t]`,
duration-matched to the training windows — anchoring is statistically
irrelevant, duration is not), and an estimate is emitted every hop of
stream time once the first window has filled. Incomplete windows are
counted and skipped; after a capture gap, missed hops are never replayed.
At construction the pipeline cross-checks that the model's input width
equals `rx_nodes × features` — a configuration/model mismatch cannot start.

The `csi-infer` binary runs this chain on any stream of `CSI_DATA` lines
with a site configuration file (calibration parameters, window/hop):

```sh
csi-infer --input capture.txt --model model.onnx --config site.json
cat /dev/ttyUSB0 | csi-infer --input - --model model.onnx --config site.json --json
```

## Labeled capture

Supervised calibration runs through the `csi-capture` binary
(`flow-capture`), which combines session recording and ground-truth
labeling so both land in one session, stamped by one clock: the installer
labels from a phone over the LAN, and label timestamps are assigned by
the edge at HTTP reception — the phone's clock is never trusted, exactly
like the sensing nodes' clocks.

The labeling page is a single embedded vanilla-HTML file (no framework,
no build step) with four large color-coded buttons — each showing the
site-specific class description from the session's `class_mapping` — and
a status bar (recording state, frame count, duration, active label with a
live elapsed counter corrected for phone-vs-edge clock skew via the
server time exposed in `/status`). UI chrome is bilingual (English
default, French auto-detected) with a persisted 12/24-hour clock toggle;
class descriptions are site *content*, displayed verbatim — the
recommended convention is numeric ("6–15", "15+"), which reads in any
language. A wrong tap is corrected by tapping the right button: labels
form a step function, so a couple of mislabeled seconds are negligible
noise.

`Ctrl-C` or the end of the input stream flushes, syncs, and seals the
session. The tool serves no CSI data and is only run during calibration.

```sh
cat /dev/ttyUSB0 | csi-capture --input - --meta meta.json --node-id rx-1
# then open http://<edge-ip>:8088 on a phone
```

## Local REST API

The `flow-api` binary is the edge daemon: the blocking stream loop runs on
its own thread and publishes each estimate into a `watch` channel; the
async HTTP server (`axum`) reads the latest value. It binds to localhost
by default and never exposes raw CSI.

- `GET /health` — liveness probe.
- `GET /estimate` — the public contract. Only **reliable** (confidence
  above the site threshold) and **fresh** (younger than `--max-age-s`)
  estimates are exposed; anything else answers
  `{"status":"unavailable"}` without leaking values. Staleness masking
  means a dead stream degrades to "unavailable" on its own — a frozen
  wait time can never stay on display.
- `GET /internal/estimate` — operator view: the full internal state
  (people, level, reliability) plus whether and why the public endpoint
  masks it.

```sh
flow-api --input - --model model.onnx --config site.json \
         --listen 127.0.0.1:8080 --max-age-s 15
```

## Toolchain and quality gates

- **Rust**: stable toolchain, edition 2024. CI enforces `cargo fmt --check`,
  `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and a
  cross-compilation check for `aarch64-unknown-linux-gnu`. Errors are typed
  with `thiserror`; `unwrap()`/`expect()` are confined to tests.
- **Python**: `uv`-managed environment (Python ≥ 3.12). CI enforces
  `ruff check`, `ruff format --check`, `pyright` (strict), and `pytest`.
- **Docs**: this file tracks the implemented state of the system; structural
  decisions are recorded as ADRs under `docs/adr/`.
