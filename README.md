# Mariam Flow

Real-time queue wait-time estimation using Wi-Fi CSI sensing.

Mariam Flow estimates how long people will wait in a queue (typically at a
university restaurant) without cameras and without collecting any personal
data. Human bodies disturb Wi-Fi multipath propagation; Channel State
Information (CSI) — the per-subcarrier amplitude and phase of the channel —
encodes those disturbances. A small classifier turns CSI into one of four
zone-density classes, and Little's Law (W = L / λ) turns density into a
waiting time.

Sensing is device-free: nobody in the queue needs to install, carry, or do
anything. Raw CSI never leaves the site; only aggregated estimates
(waiting time, density class, confidence, timestamp) are pushed upstream.

## How it works

```
ESP32-C6 TX ──(Wi-Fi)──> ESP32-C6 RX ×2 ──UDP──> edge (Rust) ──HTTPS──> backend
                                                  ingest → infer → api
```

Dedicated ESP32-C6 nodes frame the queue zone: one transmitter generates
reference traffic, receivers extract CSI and stream raw frames over UDP to an
edge aggregator. The edge parses and stores capture sessions, runs the
density classifier (ONNX via `tract`), applies Little's Law and smoothing,
and exposes the live estimate over a local REST API.

Models are trained in Python and exported to ONNX; production edge inference
is pure Rust. See [docs/architecture.md](docs/architecture.md) for the full
picture.

## Repository layout

| Path | Contents |
|---|---|
| `crates/flow-core` | Canonical domain types (CSI frames, density classes, sessions) |
| `crates/flow-ingest` | Frame parsing, UDP intake, session storage, `csi-replay` |
| `crates/flow-infer` | Feature extraction, ONNX inference, Little's Law, smoothing, `csi-infer` |
| `crates/flow-capture` | Labeled capture: recording plus the phone labeling page (`csi-capture`) |
| `crates/flow-edge` | Appliance daemon: configuration, installation lifecycle, credential, pairing, calibration, models, journal *(the real network backend arrives with the hardware)* |
| `dashboard/` | Svelte single-page dashboard, embedded in the daemon |
| `ml/` | Python package: features, training, evaluation, ONNX export, reports |
| `firmware/csi-node` | ESP32-C6 firmware based on `espressif/esp-csi` |
| `docs/` | Architecture, dashboard and local-run documentation, and ADRs |

## Getting started

Rust (stable toolchain ≥ 1.85):

```sh
cargo build --workspace
cargo test --workspace
```

Python (managed with [uv](https://docs.astral.sh/uv/), Python ≥ 3.12):

```sh
cd ml
uv sync
uv run pytest
```

Dashboard (Node ≥ 22, [pnpm](https://pnpm.io/)):

```sh
cd dashboard
pnpm install
pnpm test
pnpm build
```

The daemon embeds the built dashboard behind an optional feature, so the
Rust workspace builds and tests without a JavaScript toolchain. A release
build turns it on:

```sh
cargo build --release -p flow-edge --features dashboard
```

The appliance carries no board-specific dependency and runs on a development
machine with no sensors attached — provisioning, installation, capture and
the dashboard included. See
[docs/running-locally.md](docs/running-locally.md).

## Project status

Early development. The sensing chain is implemented end to end — frame
parsing, UDP intake, session recording and labeling, feature extraction,
training and ONNX export, live inference, wait-time estimation and the
local estimate API — with the Python↔Rust parity contracts enforced in CI.
Node firmware is validated for serial capture; the UDP path awaits hardware
validation. The appliance daemon (`flow-edge`) and its dashboard are the
work in progress. CI enforces formatting, linting, type checks, tests and a
cross-compilation check on every change.

## License

Code is licensed under the PolyForm Noncommercial License 1.0.0; commercial
licensing is available separately. Datasets and trained models are published
separately under CC BY-NC 4.0. See [LICENSE.md](LICENSE.md).

Third-party components distributed with the appliance keep their own
licenses; their notices are in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
