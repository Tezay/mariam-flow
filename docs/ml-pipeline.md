# Producing a model

Models are trained in Python (`ml/`) and exported to ONNX. The edge runs
inference in Rust through `tract`, so production carries no Python runtime.

## The Python-to-Rust bridge

Every exported model passes a parity test: identical inputs must produce
identical outputs within 1e-5.

The v1 exporter (`flow_ml.export`) hand-builds the ONNX graph from the fitted
pipeline using five core operators: `Sub`, `Div`, `MatMul`, `Add` and
`Softmax`. `tract` does not register the `ai.onnx.ml` extension operators that
sklearn-specific converters emit (ADR 0006). The artifact carries the whole
pipeline, standardization included.

The parity contract is enforced in CI. `flow_ml.export` writes fixtures, the
model plus sklearn-computed probabilities, committed under
`crates/flow-infer/tests/fixtures/`. A `flow-infer` integration test requires
`tract` to reproduce them within tolerance. `flow-infer` exposes the result as
the full probability distribution with derived class, confidence and expected
density level (ADR 0002).

## Feature extraction (v1)

`flow_ml` loads sessions with the same validations as the Rust side and cuts
them into sliding windows, by default 5 s long and 1 s apart. Windows are
measured in time, not in frames, because the frame rate varies with losses.
Seven statistics are computed per receiver and per window:

- mean amplitude;
- deviation of the amplitude over time;
- motion energy, the frame-to-frame change;
- mean correlation between subcarriers;
- mean and deviation of the RSSI;
- observed frame rate.

Phase is unused in v1. Raw phase from unsynchronized commodity radios needs a
dedicated sanitization step first.

Ground truth is a step function over label timestamps, and a window takes the
state at its centre. A window without ground truth, or incomplete for any
declared receiver, is dropped and not imputed. The result is the supervised
dataset `(X, y)`.

Live inference computes the same features in Rust (`flow-infer`). That code
mirrors `flow_ml.features` and is pinned by its own parity fixtures: windows of
frames with reference vectors computed in Python, compared at a tolerance of
1e-8 on float64. The reference vectors are computed on float32-quantized
inputs, since production frames always cross the f32 `CsiFrame`
representation.

## Training and evaluation (v1)

The v1 classifier is a multinomial logistic regression over standardized
features. It is a scikit-learn pipeline, so scaling parameters are learned on
training folds only. Its output is the probability distribution over the four
classes, and the discrete class is the argmax.

Evaluation is session-grouped cross-validation. Sessions are the split unit, so
every window is predicted once by a model that never saw its session.
Overlapping windows of one capture are heavily correlated and would otherwise
leak. Reports carry accuracy against a majority-class baseline and the full
confusion matrix.

Deterministic synthetic sessions with separable classes (`flow_ml.synthetic`)
exercise the pipeline end to end without hardware. Accuracy on them validates
the pipeline, not field performance.

## Visual reports

`flow_ml.report` renders one portrait per session and one evaluation figure.
A portrait shows an amplitude heatmap per receiver over real frame timestamps,
so capture gaps stay visible, the ground-truth band in the class palette, and
the v1 features over time. The evaluation figure is an annotated confusion
matrix with accuracy and baseline.

```sh
uv run python -m flow_ml.report --sessions data/sessions --out report/
uv run python -m flow_ml.report --demo 6 --out /tmp/report   # synthetic
```

## The deployable bundle

A training run produces a directory holding `model.onnx`, `analysis.json`,
`evaluation.json` and an optional `model.json` manifest naming the run.
`analysis.json` carries the analysis window and hop the run was trained under,
which must not be chosen independently of it.

`evaluation.json` carries what the run measured: accuracy, the majority-class
baseline, the confusion matrix, and the captures it was trained on (ADR 0024).
The appliance holds neither those captures nor a training runtime, so it cannot
recompute any of it. The matrix has truth in its rows and prediction in its
columns, in `empty, low, medium, saturated` order. The orientation is part of
the format, because reading it the other way round inverts every conclusion.

The member is additive and declares its schema. A bundle produced before it
existed still imports and reports no evaluation. A payload announcing a version
the appliance does not know is reported the same way.

The bundle does not say how a site turns a density into a waiting time.
Nothing in a training run counts heads, so the number of people a class stands
for is answered on the appliance (ADR 0023).

The bundle is packed as a gzipped tar with its members at the archive root and
imported from the dashboard. See [models.md](models.md).

## Training a bundle

One command reads recorded captures, as session directories or as the archives
the dashboard exports. It evaluates the classifier by holding whole sessions
out, fits on everything, and writes the bundle:

```sh
uv run python -m flow_ml.train --sessions data/sessions \
                               --out models --name campagne-juin
uv run python -m flow_ml.train --demo 6 --out /tmp/models --name trial
```

It runs off the appliance. A deployed unit carries no Python runtime, and its
board has neither the memory nor the time for a training run.

Sessions are the unit of the cross-validation split, so the run needs at least
as many sessions as folds. Fewer is refused with a message naming the problem.

A campaign that did not mark one of the four levels is refused the same way.
The classifier fits the classes it is shown, so an unobserved one would be
missing from the exported graph's output, and the four classes are the frozen
output space (ADR 0002). The evaluation is still printed before the refusal:
the empty row of the confusion matrix explains it.
