# Mariam Flow

Real-time queue wait-time estimation using Wi-Fi CSI sensing.

Mariam Flow estimates how long people will wait in a queue — typically at a
university restaurant — without cameras and without collecting any personal
data. Human bodies disturb Wi-Fi multipath propagation; Channel State
Information (CSI), the per-subcarrier amplitude and phase of the channel,
encodes those disturbances. A classifier turns CSI into one of four
zone-density classes, and Little's Law (`W = L / λ`) turns density into a
waiting time.

Sensing is device-free: nobody in the queue installs, carries or does
anything. Raw CSI never leaves the site — only aggregated estimates are
published.

## How it works

```
ESP32-C6 TX ──(Wi-Fi)──> ESP32-C6 RX ×2 ──UDP──> appliance ──HTTPS──> display
                                                 ingest → infer → api
```

Dedicated ESP32-C6 nodes frame the queue zone: one transmitter generates
reference traffic, two receivers extract CSI and stream raw frames over UDP to
an edge appliance. The appliance parses and stores capture sessions, runs the
density classifier (ONNX through `tract`), applies Little's Law and smoothing,
and publishes the estimate. An installer configures the whole unit from a web
dashboard the appliance serves itself.

Models are trained in Python and exported to ONNX; production inference is
pure Rust, so a deployed unit carries no Python runtime.

## Status

| Area | State |
|---|---|
| Sensing chain — parsing, intake, session storage, features, inference | Implemented, Python↔Rust parity enforced in CI |
| Appliance daemon and its dashboard | Implemented; the real network backend arrives with the hardware |
| Node firmware | Serial capture validated on ESP32-C6; the UDP path awaits hardware |
| Trained model | Awaiting captures from a real site |

The appliance runs end to end on a development machine with no sensors
attached — provisioning, installation, capture and the dashboard included.

## Repository layout

| Path | Contents |
|---|---|
| `crates/flow-core` | Domain types: CSI frames, density classes, labels, sessions |
| `crates/flow-ingest` | Frame parsing, UDP intake, session storage, `csi-replay` |
| `crates/flow-infer` | Feature extraction, ONNX inference, Little's Law, smoothing, `csi-infer` |
| `crates/flow-capture` | Labelled capture on the bench: recording plus the phone labelling page |
| `crates/flow-edge` | The appliance daemon |
| `dashboard/` | Svelte single-page dashboard, embedded in the daemon |
| `ml/` | Python package: features, training, evaluation, ONNX export, reports |
| `firmware/csi-node` | ESP32-C6 firmware based on `espressif/esp-csi` |
| `docs/` | Technical documentation and architecture decision records |

## Getting started

Rust — toolchain pinned by `rust-toolchain.toml`:

```sh
cargo build --workspace
cargo test --workspace
```

Python — managed with [uv](https://docs.astral.sh/uv/), Python ≥ 3.12:

```sh
cd ml && uv sync && uv run pytest
```

Dashboard — Node ≥ 22 with [pnpm](https://pnpm.io/):

```sh
cd dashboard && pnpm install && pnpm test && pnpm build
```

The daemon embeds the built dashboard behind an optional feature, so the Rust
workspace builds and tests without a JavaScript toolchain:

```sh
cargo build --release -p flow-edge --features dashboard
```

To bring an appliance up locally, see
[docs/running-locally.md](docs/running-locally.md).

## Documentation

[docs/architecture.md](docs/architecture.md) is the entry point: the
invariants, the components, and an index of one document per stage of the
chain. Structural decisions and their rationale are recorded as ADRs under
[docs/adr/](docs/adr/README.md).

## Security

Vulnerability reports, the scope, and what the product promises about the
people it measures are in [SECURITY.md](SECURITY.md).

## License

Code is licensed under the PolyForm Noncommercial License 1.0.0; commercial
licensing is available separately. Datasets and trained models are published
separately under CC BY-NC 4.0. See [LICENSE.md](LICENSE.md).

Third-party components distributed with the appliance keep their own licences;
their notices are in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
