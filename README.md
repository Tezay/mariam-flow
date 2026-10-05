# Mariam Flow

Real-time queue wait-time estimation using Wi-Fi CSI sensing.

Mariam Flow estimates how long people will wait in a queue, typically at a
university restaurant, without cameras and without collecting any personal
data. Human bodies disturb Wi-Fi multipath propagation. Channel State
Information (CSI), the per-subcarrier amplitude and phase of the channel,
encodes those disturbances. A classifier turns CSI into one of four
zone-density classes, and Little's Law (`W = L / λ`) turns density into a
waiting time.

Sensing is device-free: nobody in the queue installs, carries or does
anything. Raw CSI never leaves the site. Only aggregated estimates are
published.

## How it works

```
ESP32-C6 TX ──(Wi-Fi)──> ESP32-C6 RX ×2 ──UDP──> appliance ──HTTP──> display
                                                 ingest → infer → api
```

Dedicated ESP32-C6 nodes frame the queue zone: one transmitter generates
reference traffic, two receivers extract CSI and stream raw frames over UDP to
an edge appliance. The appliance parses and stores capture sessions, runs the
density classifier (ONNX through `tract`), applies Little's Law and smoothing,
and publishes the estimate. An installer configures the whole unit from a web
dashboard the appliance serves itself.

Models are trained in Python and exported to ONNX. Production inference is
pure Rust, so a deployed unit carries no Python runtime.

## Method and evaluation

Each receiver's CSI is cut into sliding windows of 5 s, 1 s apart. Seven
statistics are computed per receiver and per window: mean amplitude, its
deviation over time, motion energy, mean correlation between subcarriers, the
mean and deviation of the RSSI, and the observed frame rate. Phase is not
used. The first classifier is a multinomial logistic regression over
standardized features. It outputs a probability distribution over four ordered
classes (empty, low, medium, saturated), and Little's Law turns the expected
number of people into a waiting time.

Evaluation is cross-validation grouped by capture session, because overlapping
windows of one capture are strongly correlated and would leak across folds.
Every result is reported against a majority-class baseline, with the full
confusion matrix. Training runs in Python and inference in Rust, and CI checks
on committed fixtures that both compute the same features and the same
probabilities.

Details are in [docs/ml-pipeline.md](docs/ml-pipeline.md) and
[docs/estimation.md](docs/estimation.md).

## Status

| Area | State |
|---|---|
| Sensing chain (parsing, intake, session storage, features, inference) | Implemented, Python↔Rust parity enforced in CI |
| Appliance daemon and its dashboard | Implemented and run on a Raspberry Pi Zero 2 W. The system network is still configured by hand |
| Node firmware | Validated on ESP32-C6, over serial and over UDP to the appliance |
| Trained model | Awaiting captures from a real site |

The appliance runs end to end on a development machine with no sensors
attached. That covers provisioning, installation, capture and the dashboard.

## Limitations

- No model has been trained on captures from a real site yet. Accuracy
  measured so far comes from synthetic sessions and validates the pipeline,
  not field performance.
- A model is calibrated for one site and one placement of the sensors. Nothing
  here shows that it transfers to another.
- The model classifies a density and does not count people. The waiting time
  depends on head counts and a service rate that the operator enters.
- Estimate history is kept at one-minute resolution.

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

Rust, with the toolchain pinned by `rust-toolchain.toml`:

```sh
cargo build --workspace
cargo test --workspace
```

Python ≥ 3.12, managed with [uv](https://docs.astral.sh/uv/):

```sh
cd ml && uv sync && uv run pytest
```

Dashboard, with Node ≥ 22 and [pnpm](https://pnpm.io/):

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

| Document | Contents |
|---|---|
| [docs/architecture.md](docs/architecture.md) | The entry point: invariants, components, and an index of the other documents |
| [docs/ml-pipeline.md](docs/ml-pipeline.md) | Features, training, evaluation, export and the Python-to-Rust parity tests |
| [docs/estimation.md](docs/estimation.md) | From a density to a waiting time, and how the estimate is published |
| [docs/data-model.md](docs/data-model.md) | The capture session format and the frames the nodes emit |
| [docs/sensing-nodes.md](docs/sensing-nodes.md) | How the sensors are placed, identified and replaced |
| [docs/appliance.md](docs/appliance.md) | The appliance, its configuration and its networks |
| [docs/running-locally.md](docs/running-locally.md) | Bringing an appliance up on a development machine |
| [docs/adr/](docs/adr/README.md) | Architecture decision records: what was decided, and why |

## References

- J. D. C. Little, "A Proof for the Queuing Formula: L = λW", *Operations
  Research*, 9(3), 383–387, 1961.
- [`espressif/esp-csi`](https://github.com/espressif/esp-csi), from which the
  node firmware is derived.

## Security

Vulnerability reports, the scope, and what the product promises about the
people it measures are in [SECURITY.md](SECURITY.md).

## License

Source available, not open source.

| What | Licence |
|---|---|
| Source code | PolyForm Noncommercial 1.0.0. Commercial licensing is available separately |
| Firmware code derived from `espressif/esp-csi` | Apache 2.0 |
| Datasets and trained models | CC BY-NC 4.0, published separately. None is published yet |

The terms are in [LICENSE.md](LICENSE.md). Third-party components distributed
with the appliance keep their own licences, listed in
[THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
