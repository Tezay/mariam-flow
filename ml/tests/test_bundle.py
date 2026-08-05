"""The deployable bundle: a model and the geometry it was trained under."""

from __future__ import annotations

import json
import tarfile
from pathlib import Path

import onnx

from flow_ml.bundle import (
    ANALYSIS_FILE,
    MANIFEST_FILE,
    MODEL_FILE,
    AnalysisWindow,
    Manifest,
    archive_bundle,
    write_bundle,
)
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US


def _analysis(**overrides: object) -> AnalysisWindow:
    return AnalysisWindow(**overrides)  # type: ignore[arg-type]


def _manifest() -> Manifest:
    return Manifest(name="campagne-juin", trained_at="2026-08-12", sessions=3)


def _model() -> onnx.ModelProto:
    from tests.test_export import fitted

    proto, _, _ = fitted()
    return proto


def test_analysis_json_holds_the_geometry_and_nothing_else(tmp_path: Path) -> None:
    write_bundle(tmp_path / "b", _model(), _analysis(), _manifest())

    payload = json.loads((tmp_path / "b" / ANALYSIS_FILE).read_text(encoding="utf-8"))

    # What a site turns a density into a waiting time with is answered on the
    # appliance: nothing in a training run counts heads, so shipping a people
    # count here would be shipping a guess as a result.
    assert set(payload) == {"window_us", "hop_us"}


def test_the_training_window_travels_with_the_model(tmp_path: Path) -> None:
    # The value that must never be chosen independently of the run: a model
    # trained on five-second windows applied to three-second ones gives
    # estimates that look plausible and are not.
    write_bundle(
        tmp_path / "b", _model(), _analysis(window_us=7_000_000, hop_us=500_000), _manifest()
    )

    payload = json.loads((tmp_path / "b" / ANALYSIS_FILE).read_text(encoding="utf-8"))

    assert payload["window_us"] == 7_000_000
    assert payload["hop_us"] == 500_000


def test_defaults_come_from_the_shared_window_constants(tmp_path: Path) -> None:
    write_bundle(tmp_path / "b", _model(), _analysis(), _manifest())

    payload = json.loads((tmp_path / "b" / ANALYSIS_FILE).read_text(encoding="utf-8"))

    assert payload["window_us"] == DEFAULT_WINDOW_US
    assert payload["hop_us"] == DEFAULT_HOP_US


def test_the_bundle_holds_both_files(tmp_path: Path) -> None:
    bundle = write_bundle(tmp_path / "b", _model(), _analysis(), _manifest())

    assert (bundle / MODEL_FILE).stat().st_size > 0
    assert (bundle / ANALYSIS_FILE).exists()


def test_the_archive_stores_members_at_its_root(tmp_path: Path) -> None:
    bundle = write_bundle(tmp_path / "b", _model(), _analysis(), _manifest())

    archive = archive_bundle(bundle, tmp_path / "bundle.tar.gz")

    with tarfile.open(archive, "r:gz") as opened:
        names = sorted(opened.getnames())
    # Known names rather than a prefix the appliance would have to guess.
    assert names == sorted([MODEL_FILE, ANALYSIS_FILE, MANIFEST_FILE])


def test_a_model_says_what_it_is_and_when_it_was_trained(tmp_path: Path) -> None:
    # The appliance can date the moment it received a bundle, but not the run
    # that produced it — and the two answer different questions when estimates
    # change.
    bundle = write_bundle(tmp_path / "b", _model(), _analysis(), _manifest())

    payload = json.loads((bundle / MANIFEST_FILE).read_text(encoding="utf-8"))

    assert payload == {"name": "campagne-juin", "trained_at": "2026-08-12", "sessions": 3}
