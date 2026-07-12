# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false
# pyright: reportUnknownVariableType=false, reportUnknownArgumentType=false
"""ONNX export tests: structure, and numerical equivalence with sklearn.

The exported initializers are read back from the protobuf and the graph is
re-executed with plain numpy — so these tests verify the *artifact*, not
the code that produced it. The Rust side re-verifies the same numbers
through tract (the actual parity test of ADR 0004).
"""

import json
from pathlib import Path

import numpy as np
import pytest
from onnx import ModelProto, numpy_helper
from sklearn.pipeline import Pipeline

from flow_ml.export import (
    PARITY_TOLERANCE,
    export_pipeline,
    fixture_sessions,
    write_parity_fixture,
)
from flow_ml.training import build_dataset, make_classifier


def fitted() -> tuple[ModelProto, Pipeline, np.ndarray]:
    x, y, _ = build_dataset(fixture_sessions(), hop_us=2_000_000)
    pipeline = make_classifier()
    pipeline.fit(x, y)
    return export_pipeline(pipeline), pipeline, x


def forward_numpy(proto: ModelProto, x: np.ndarray) -> np.ndarray:
    """Re-executes the exported graph with numpy, from its initializers."""
    weights = {t.name: numpy_helper.to_array(t) for t in proto.graph.initializer}
    z = (x - weights["mean"]) / weights["scale"]
    z = z @ weights["weight"] + weights["bias"]
    e = np.exp(z - z.max(axis=1, keepdims=True))
    return e / e.sum(axis=1, keepdims=True)


def test_exported_graph_structure() -> None:
    proto, _, _ = fitted()
    ops = [node.op_type for node in proto.graph.node]
    assert ops == ["Sub", "Div", "MatMul", "Add", "Softmax"]
    assert proto.graph.input[0].name == "features"
    assert proto.graph.output[0].name == "probabilities"
    assert {t.name for t in proto.graph.initializer} == {"mean", "scale", "weight", "bias"}
    domains = {opset.domain for opset in proto.opset_import}
    assert domains == {""}, "core ONNX domain only (ADR 0006)"


def test_exported_model_matches_sklearn_numerically() -> None:
    proto, pipeline, x = fitted()
    x32 = x.astype(np.float32)
    expected = pipeline.predict_proba(x32)
    actual = forward_numpy(proto, x32.astype(np.float64))
    assert np.abs(actual - expected).max() < PARITY_TOLERANCE


def test_parity_fixture_round_trips(tmp_path: Path) -> None:
    write_parity_fixture(tmp_path, rows=8)
    payload = json.loads((tmp_path / "parity.json").read_text(encoding="utf-8"))
    proto = ModelProto()
    proto.ParseFromString((tmp_path / "model.onnx").read_bytes())

    inputs = np.array(payload["inputs"], dtype=np.float64)
    expected = np.array(payload["expected_probabilities"], dtype=np.float64)
    assert inputs.shape[1] == payload["n_features"]
    assert expected.shape == (inputs.shape[0], 4)
    assert np.allclose(expected.sum(axis=1), 1.0)

    actual = forward_numpy(proto, inputs)
    assert np.abs(actual - expected).max() < payload["tolerance"]


def test_fixture_generation_is_deterministic(tmp_path: Path) -> None:
    write_parity_fixture(tmp_path / "a", rows=8)
    write_parity_fixture(tmp_path / "b", rows=8)
    assert (tmp_path / "a" / "model.onnx").read_bytes() == (
        tmp_path / "b" / "model.onnx"
    ).read_bytes()
    parity_a = (tmp_path / "a" / "parity.json").read_text()
    parity_b = (tmp_path / "b" / "parity.json").read_text()
    assert parity_a == parity_b


def test_probabilities_are_a_distribution() -> None:
    proto, _, x = fitted()
    probabilities = forward_numpy(proto, x)
    assert (probabilities >= 0).all()
    assert probabilities.sum(axis=1) == pytest.approx(np.ones(x.shape[0]))
