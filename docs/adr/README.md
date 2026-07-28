# Architecture Decision Records

This directory holds one record per structural technical decision, in
[MADR](https://adr.github.io/madr/) format.

Conventions:

- Files are numbered sequentially: `NNNN-short-title.md`
  (e.g. `0001-canonical-session-format.md`).
- A record is immutable once accepted; a superseding decision gets a new
  record that references the old one.
- Records document the technical context, the options considered, the
  decision, and its consequences.

## Index

| # | Title | Status |
|---|---|---|
| [0001](0001-canonical-capture-session-format.md) | Canonical capture-session format | accepted |
| [0002](0002-probabilistic-four-class-density-output.md) | Density model output: probability distribution over four ordered classes | accepted |
| [0003](0003-per-site-model-training.md) | One trained model per site | accepted |
| [0004](0004-python-training-onnx-rust-inference.md) | Train in Python, export to ONNX, run inference in Rust with tract | accepted |
| [0005](0005-esp-csi-text-wire-format.md) | Node-to-edge frame format: esp-csi text lines | accepted |
| [0006](0006-handbuilt-onnx-graph-export.md) | Hand-built core-operator ONNX graph for the v1 export | accepted |
| [0007](0007-udp-intake-sender-identity.md) | UDP intake: sender-identified nodes, reception-time stamping | accepted |
| [0008](0008-single-process-edge-appliance-daemon.md) | Single-process edge appliance daemon | accepted |
| [0009](0009-dedicated-sensor-access-point-separate-uplink.md) | Dedicated sensor access point on a separate radio from the site uplink | accepted |
| [0010](0010-per-device-secret-argon2id-credential.md) | Per-device secret as the administrator credential | accepted |
