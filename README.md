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
| `crates/flow-ingest` | UDP ingestion, buffering, session storage *(placeholder)* |
| `crates/flow-infer` | ONNX inference, Little's Law, smoothing *(placeholder)* |
| `crates/flow-api` | Local REST API *(placeholder)* |
| `ml/` | Python package: features, training, ONNX export *(scaffold)* |
| `firmware/csi-node` | ESP32-C6 firmware based on `espressif/esp-csi` *(placeholder)* |
| `docs/` | Architecture documentation and ADRs |

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

## Project status

Early development. The data model (`flow-core`) is implemented and tested;
ingestion, inference, API, and firmware are placeholders. CI enforces
formatting, linting, type checks, and tests on every change.

## License

Code is licensed under the PolyForm Noncommercial License 1.0.0; commercial
licensing is available separately. Datasets and trained models are published
separately under CC BY-NC 4.0. See [LICENSE.md](LICENSE.md).
