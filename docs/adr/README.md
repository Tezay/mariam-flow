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
| [0011](0011-cookie-sessions-and-login-throttling.md) | Cookie sessions, login throttling, and a deny-by-default surface | accepted |
| [0012](0012-sqlite-appliance-journal.md) | SQLite appliance journal with bounded retention | accepted |
| [0013](0013-svelte-dashboard-embedded-in-the-daemon.md) | Svelte single-page dashboard embedded in the daemon | accepted |
| [0014](0014-minute-resolution-estimate-history.md) | Minute-resolution estimate history | accepted |
| [0015](0015-declared-service-hours.md) | Declared service hours | accepted |
| [0016](0016-node-pairing-from-the-observed-stream.md) | Node pairing from the observed stream | accepted |
| [0017](0017-intake-independent-of-inference.md) | Intake independent of inference | accepted |
| [0018](0018-network-interview-and-administrator-request.md) | Network interview and administrator request | accepted |
| [0019](0019-calibration-recording-on-the-appliance.md) | Calibration recording on the appliance | accepted |
| [0020](0020-model-bundles-and-the-appliance-library.md) | Model bundles and the appliance library | accepted |
| [0021](0021-a-node-identifier-outlives-its-hardware.md) | A node identifier outlives its hardware | accepted |
