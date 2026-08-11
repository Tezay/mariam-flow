# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false
# pyright: reportUnknownVariableType=false, reportUnknownArgumentType=false
"""ONNX export of the v1 classifier, and parity-fixture generation.

The fitted pipeline (standardization + multinomial logistic regression) is
mathematically ``softmax((x − μ)/σ · Wᵀ + b)``, so the exported model is a
hand-built graph of five core ONNX operators::

    features ─ Sub(μ) ─ Div(σ) ─ MatMul(Wᵀ) ─ Add(b) ─ Softmax ─ probabilities

Core operators only, by decision (ADR 0006): the sklearn-specific
converters emit ``ai.onnx.ml`` operators (``Scaler``, ``ZipMap``) that the
Rust runtime (`tract`) does not register. Building the graph ourselves
keeps the artifact auditable and the runtime path guaranteed.

The exported model carries the whole pipeline — scaler included — as one
artifact, so the edge cannot forget or mismatch the normalization.

Parity fixtures (``model.onnx`` + ``parity.json``) feed the mandatory
Python↔Rust parity test (ADR 0004): the Rust side must reproduce the
probabilities below a 1e-5 tolerance. Regenerate them with::

    uv run python -m flow_ml.export ../crates/flow-infer/tests/fixtures
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import numpy.typing as npt
import onnx
from onnx import TensorProto, helper, numpy_helper
from sklearn.pipeline import Pipeline

from flow_ml.features import NODE_FEATURES, node_features
from flow_ml.session import DensityClass, Frame, Session
from flow_ml.synthetic import synthetic_session
from flow_ml.training import build_dataset, make_classifier
from flow_ml.windows import sliding_windows

PARITY_TOLERANCE = 1e-5
MODEL_FILE = "model.onnx"
INCOMPLETE_MODEL_FILE = "model-incomplete.onnx"
PARITY_FILE = "parity.json"

FEATURES_FILE = "features.json"
FEATURE_TOLERANCE = 1e-8
"""Feature parity tolerance: both sides compute in float64, so only
summation-order differences remain — far below the float32 resolution the
model consumes, far above accumulated rounding."""

_OPSET = 13
_IR_VERSION = 8


def export_pipeline(pipeline: Pipeline) -> onnx.ModelProto:
    """Exports a fitted scaler+logistic-regression pipeline to ONNX.

    Input: ``features`` (float32, shape ``[1, d]``) — one window at a time,
    matching real-time edge inference. Output: ``probabilities`` (float32,
    shape ``[1, 4]``), the softmax distribution of ADR 0002.
    """
    scaler = pipeline.named_steps["scale"]
    model = pipeline.named_steps["model"]
    mean = np.asarray(scaler.mean_, dtype=np.float32)
    scale = np.asarray(scaler.scale_, dtype=np.float32)
    weight = np.asarray(model.coef_, dtype=np.float32).T  # (d, 4)
    bias = np.asarray(model.intercept_, dtype=np.float32)
    n_features = int(mean.shape[0])
    n_classes = int(bias.shape[0])

    nodes = [
        helper.make_node("Sub", ["features", "mean"], ["centered"]),
        helper.make_node("Div", ["centered", "scale"], ["standardized"]),
        helper.make_node("MatMul", ["standardized", "weight"], ["scores_raw"]),
        helper.make_node("Add", ["scores_raw", "bias"], ["scores"]),
        helper.make_node("Softmax", ["scores"], ["probabilities"], axis=-1),
    ]
    graph = helper.make_graph(
        nodes,
        "density_classifier_v1",
        inputs=[helper.make_tensor_value_info("features", TensorProto.FLOAT, [1, n_features])],
        outputs=[helper.make_tensor_value_info("probabilities", TensorProto.FLOAT, [1, n_classes])],
        initializer=[
            numpy_helper.from_array(mean, "mean"),
            numpy_helper.from_array(scale, "scale"),
            numpy_helper.from_array(weight, "weight"),
            numpy_helper.from_array(bias, "bias"),
        ],
    )
    proto = helper.make_model(
        graph,
        producer_name="flow-ml",
        opset_imports=[helper.make_opsetid("", _OPSET)],
    )
    proto.ir_version = _IR_VERSION
    onnx.checker.check_model(proto)
    return proto


def fixture_sessions() -> list[Session]:
    """The deterministic sessions behind the committed parity fixture."""
    return [
        synthetic_session(
            f"parity-{seed:02}",
            seed=seed,
            seconds_per_class=12.0,
            frame_rate_hz=10.0,
            subcarriers=8,
        )
        for seed in range(4)
    ]


def write_parity_fixture(out_dir: Path, *, rows: int = 24) -> None:
    """Trains on the fixture sessions and writes ``model.onnx`` +
    ``parity.json`` (inputs and sklearn-computed expected probabilities)."""
    x, y, _ = build_dataset(fixture_sessions(), hop_us=2_000_000)
    pipeline = make_classifier()
    pipeline.fit(x, y)

    proto = export_pipeline(pipeline)

    # The Rust side receives float32 features; expectations are computed by
    # sklearn on those exact float32 values (upcast to float64 internally).
    indices = np.unique(np.linspace(0, x.shape[0] - 1, rows).astype(np.int64))
    inputs = x[indices].astype(np.float32)
    expected: npt.NDArray[np.float64] = pipeline.predict_proba(inputs)

    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / MODEL_FILE).write_bytes(proto.SerializeToString())
    payload = {
        "n_features": int(inputs.shape[1]),
        "tolerance": PARITY_TOLERANCE,
        "inputs": [[float(v) for v in row] for row in inputs],
        "expected_probabilities": [[float(v) for v in row] for row in expected],
    }
    (out_dir / PARITY_FILE).write_text(json.dumps(payload, indent=1), encoding="utf-8")


def _quantize_frame(frame: Frame) -> Frame:
    """Rounds amplitudes and phases to float32-representable values.

    Real frames always cross the f32 `CsiFrame` type on the Rust side, so
    the parity reference must be computed on f32-quantized inputs — the
    parity test caught exactly this mismatch (max |Δ| ≈ 4e-8, the f32
    quantization scale) when the fixture was first generated from raw
    float64 synthetic values.
    """
    return Frame(
        ts_us=frame.ts_us,
        node_id=frame.node_id,
        rssi=frame.rssi,
        mcs=frame.mcs,
        amp=tuple(float(np.float32(a)) for a in frame.amp),
        phase=tuple(float(np.float32(p)) for p in frame.phase),
    )


def _frame_to_json(frame: Frame) -> dict[str, object]:
    """Canonical JSON form of one frame (the flow-core wire format)."""
    return {
        "ts_us": frame.ts_us,
        "node_id": frame.node_id,
        "rssi": frame.rssi,
        "mcs": frame.mcs,
        "len": len(frame.amp),
        "amp": list(frame.amp),
        "phase": list(frame.phase),
    }


def write_feature_fixture(out_dir: Path, *, cases: int = 8) -> None:
    """Writes ``features.json``: windows of frames with the feature vectors
    computed by this (reference) implementation, for the Rust mirror's
    parity test."""
    session = synthetic_session(
        "feature-parity",
        seed=99,
        seconds_per_class=10.0,
        frame_rate_hz=8.0,
        subcarriers=6,
    )
    windows = sliding_windows(session, hop_us=3_000_000)
    indices = np.unique(np.linspace(0, len(windows) - 1, cases).astype(np.int64))
    payload_cases: list[dict[str, object]] = []
    for index in indices:
        window = windows[int(index)]
        frames = [_quantize_frame(frame) for frame in window.frames]
        vector = node_features(frames, window.duration_us)
        payload_cases.append(
            {
                "duration_us": window.duration_us,
                "frames": [_frame_to_json(frame) for frame in frames],
                "expected": [float(v) for v in vector],
            }
        )
    payload = {
        "tolerance": FEATURE_TOLERANCE,
        "features": list(NODE_FEATURES),
        "cases": payload_cases,
    }
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / FEATURES_FILE).write_text(json.dumps(payload, indent=1), encoding="utf-8")


def write_incomplete_fixture(out_dir: Path) -> None:
    """Writes a model one class short, for the guard that must refuse it.

    Generated here rather than hand-forged in Rust: the fixture has to be a
    graph a real run would produce, not one shaped to pass the check.
    """
    x, y, _ = build_dataset(fixture_sessions(), hop_us=2_000_000)
    observed = y != int(DensityClass.LOW)
    pipeline = make_classifier()
    pipeline.fit(x[observed], y[observed])

    out_dir.mkdir(parents=True, exist_ok=True)
    proto = export_pipeline(pipeline)
    (out_dir / INCOMPLETE_MODEL_FILE).write_bytes(proto.SerializeToString())


def main() -> None:
    """Entry point: ``python -m flow_ml.export <output_dir>`` — writes all
    Rust-side parity fixtures (model, probabilities, features)."""
    if len(sys.argv) != 2:
        raise SystemExit("usage: python -m flow_ml.export <output_dir>")
    out_dir = Path(sys.argv[1])
    write_parity_fixture(out_dir)
    write_feature_fixture(out_dir)
    write_incomplete_fixture(out_dir)


if __name__ == "__main__":
    main()
