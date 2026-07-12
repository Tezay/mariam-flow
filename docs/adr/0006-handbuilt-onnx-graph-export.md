# Hand-built core-operator ONNX graph for the v1 export

- Status: accepted
- Date: 2026-07-12

## Context and Problem Statement

ADR 0004 fixed the training-to-production bridge: train in Python, export
to ONNX, run with `tract` in Rust, with a mandatory numerical parity test.
It left the export tooling open. The standard converter for scikit-learn,
`skl2onnx`, emits operators from the `ai.onnx.ml` extension domain: a
`StandardScaler` becomes `Scaler`, a classifier's probability output goes
through `ZipMap`. Inspection of tract's operator registry
(`onnx/src/ops/ml`) shows partial `ai.onnx.ml` coverage:
`LinearClassifier`, `LinearRegressor`, `TreeEnsembleClassifier`,
`CategoryMapper` and `Normalizer` are registered — `Scaler` and `ZipMap`
are not. A pipeline exported by `skl2onnx` would therefore fail to load on
the edge runtime.

## Considered Options

1. Hand-build the ONNX graph from the fitted pipeline, core operators only
2. `skl2onnx` with `zipmap` disabled, folding the scaler into the
   classifier coefficients
3. Switch the edge runtime to one with full `ai.onnx.ml` support

## Decision Outcome

Chosen option 1. The v1 pipeline is mathematically
`softmax((x − μ)/σ · Wᵀ + b)`; the exporter reads the fitted parameters
and emits a five-node graph of core ONNX operators:

```
features ─ Sub(μ) ─ Div(σ) ─ MatMul(Wᵀ) ─ Add(b) ─ Softmax ─ probabilities
```

Core operators are the best-supported subset of every ONNX runtime, and
the resulting artifact is fully auditable (five nodes, four initializers,
under a kilobyte). The mandatory parity test guards the construction: the
Python side records sklearn probabilities for fixture inputs, and a Rust
integration test requires tract to reproduce them within 1e-5.

Option 2 works but hides the standardization inside doctored classifier
coefficients — the artifact no longer reflects the pipeline's structure,
and it still depends on converter behavior for an operator set the runtime
only partially implements. Option 3 trades a small exporter for the choice
of inference runtime, inverting the priorities: `tract` was selected for
its pure-Rust, small-footprint fit for the edge (ADR 0004), and the v2
path (`torch.onnx`) emits core operators natively, where tract's coverage
is strongest.

### Consequences

- Good: guaranteed runtime compatibility; transparent, tiny artifacts; one
  less heavyweight Python dependency; the export logic doubles as
  documentation of the model.
- Bad: each new model family needs its own export function (acceptable
  while the family count is one; v2 deep-learning models will use
  `torch.onnx` instead); the exporter must be kept consistent with the
  sklearn pipeline structure — enforced by tests that re-execute the
  exported graph numerically against `predict_proba`.
