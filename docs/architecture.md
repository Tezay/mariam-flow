# Architecture

Mariam Flow estimates, in real time, the waiting time of a queue — typically at
a university restaurant — using Wi-Fi sensing. Human bodies disturb the
multipath propagation of Wi-Fi radio waves; Channel State Information (CSI)
captures the amplitude and phase of the channel per OFDM subcarrier and
therefore encodes those disturbances. The system classifies the monitored zone
into four density classes and converts density into a waiting time with
Little's Law.

The system is device-free by design: no cameras, no personal data, no
interaction required from the people in the queue.

## System overview

```
[Queue zone]   ESP32-C6 TX ──(Wi-Fi traffic)──> ESP32-C6 RX ×2
                                   │ UDP (raw CSI frames)
                                   ▼
                       Edge appliance (Linux)
                       ┌──────────────────────────────────────┐
                       │ flow-ingest   parse → ring buffer →  │
                       │               session storage        │
                       │ flow-infer    features → ONNX model  │
                       │               → density → wait time  │
                       │ flow-edge     public estimate +      │
                       │               appliance dashboard    │
                       └──────────────────────────────────────┘
                                   │ HTTPS — aggregated estimates only:
                                   │ {wait, class, confidence, timestamp}
                                   ▼
                             Backend / display layer
```

## Invariants

- **Raw CSI never leaves the site.** Only aggregated estimates — waiting time,
  density class, confidence, timestamp — are published.
- **Nothing is collected about the people in the queue.** No camera, no
  identifier, no per-person record anywhere in the system.
- **No credential crosses the HTTP surface.** The uplink is reported by mode
  and network name, never by passphrase.
- **A model and the geometry it was trained under travel together.** What a
  site turns a density into a waiting time with is answered on the appliance.

## Components

| Path | Role | Status |
|---|---|---|
| `crates/flow-core` | Domain types: CSI frames, density classes, labels, session metadata | Implemented |
| `crates/flow-ingest` | Frame parsing, stream reading with loss statistics, session storage, UDP intake, `csi-replay` | Implemented |
| `crates/flow-infer` | Window features, ONNX inference (`tract`), Little's Law, smoothing, live pipeline, `csi-infer` | Implemented |
| `crates/flow-capture` | Labelled capture on the bench: recording plus the phone labelling page (`csi-capture`) | Implemented |
| `crates/flow-edge` | The appliance daemon | Implemented; the real network backend arrives with the hardware |
| `dashboard/` | Svelte single-page dashboard, embedded in the daemon | Implemented |
| `ml/` | Python package (`flow_ml`): features, training, ONNX export, bundles, reports | Implemented; awaiting real captures |
| `firmware/csi-node` | C / ESP-IDF firmware for ESP32-C6 nodes, based on `espressif/esp-csi` | Serial capture validated; UDP path pending |

The edge components are plain Rust binaries with no board-specific dependency;
any Linux or macOS machine can play the edge role during development. The
production cross-compilation target is `aarch64-unknown-linux-gnu`.

## The chain, document by document

| Stage | Document |
|---|---|
| What a capture holds, and the frame format nodes emit | [data-model.md](data-model.md) |
| How the sensors are placed, identified and replaced | [sensing-nodes.md](sensing-nodes.md) |
| How a labelled capture is recorded, on the bench and on site | [capture.md](capture.md) |
| How a model is trained, exported and checked for parity | [ml-pipeline.md](ml-pipeline.md) |
| How a model returns to an appliance | [models.md](models.md) |
| How a density becomes a waiting time, and how it is published | [estimation.md](estimation.md) |
| What the appliance is, and how it reaches its networks | [appliance.md](appliance.md) |
| The routes it serves, and how they are guarded | [http-surface.md](http-surface.md) |
| What it retains: estimate history and event journal | [records.md](records.md) |
| How the dashboard is built and laid out | [dashboard.md](dashboard.md) |
| The screens an operator works from | [dashboard-screens.md](dashboard-screens.md) |
| Running the whole thing on a development machine | [running-locally.md](running-locally.md) |

Structural decisions and their rationale are recorded as ADRs under
[adr/](adr/README.md). This set of documents describes the implemented state;
the ADRs say why it is that way.

## Toolchain and quality gates

- **Rust** — stable toolchain pinned by `rust-toolchain.toml`, edition 2024. CI
  enforces `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -D warnings`, `cargo test --workspace`, and a cross-compilation check for
  `aarch64-unknown-linux-gnu`. Errors are typed with `thiserror`;
  `unwrap()`/`expect()` are confined to tests.
- **Python** — `uv`-managed environment, Python ≥ 3.12. CI enforces
  `ruff check`, `ruff format --check`, `pyright` in strict mode, and `pytest`.
- **Dashboard** — Node ≥ 22 with pnpm. CI enforces Prettier, ESLint,
  `svelte-check`, Vitest, the production build, and that the result still
  embeds into the daemon.
- **Dependencies** — `cargo-deny` gates advisories, licences, bans and sources;
  `pnpm audit` gates the dashboard at moderate severity and above.
