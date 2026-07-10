# Train in Python, export to ONNX, run inference in Rust with tract

- Status: accepted
- Date: 2026-07-10

## Context and Problem Statement

The edge aggregator runs unattended, continuously, on modest hardware, and
must stay reliable for long periods. Model development, in contrast, needs
the Python ecosystem (scikit-learn, and later deep-learning frameworks) for
iteration speed. These two requirements pull in opposite directions: a
production Python runtime on the edge brings dependency management, memory
footprint, and long-running-process concerns; an all-Rust ML stack gives up
the ecosystem where models are actually developed.

## Considered Options

1. Train in Python, export to ONNX, infer in Rust with `tract`
2. Python everywhere (training and edge inference)
3. Rust everywhere (train with `linfa` or similar)
4. Embed Python in the Rust edge process (PyO3)

## Decision Outcome

Chosen option 1. Models are developed and trained in `ml/` (Python), then
exported to ONNX (`skl2onnx` for classical models, `torch.onnx` later).
The edge (`flow-infer`) loads the ONNX file with `tract`, a pure-Rust
inference engine — production carries no Python interpreter at all.

The bridge is guarded by a mandatory parity test: for every exported model,
identical inputs must produce identical outputs between the Python model and
the `tract` execution, within a 1e-5 tolerance. A model that fails parity is
not deployable.

Option 2 optimizes for iteration but moves Python's operational burden onto
every edge deployment. Option 3 keeps the edge clean but abandons the
standard ML toolchain precisely where velocity matters most (feature
iteration, evaluation, future deep-learning work). Option 4 couples the edge
binary to an embedded interpreter, inheriting Python's operational concerns
plus FFI complexity — the worst of both worlds for a long-running daemon.

### Consequences

- Good: single static Rust binary on the edge; model updates are data (an
  ONNX file plus metadata), not code deployments; training iteration keeps
  full ecosystem access.
- Bad: only ONNX-exportable model classes are eligible, which constrains
  model choice; the parity test and export tooling are additional pipeline
  surface that must be maintained.
