"""The deployable bundle: model and tuning, produced and shipped together."""

from __future__ import annotations

import json
import tarfile
from pathlib import Path

import onnx

from flow_ml.bundle import (
    MANIFEST_FILE,
    MODEL_FILE,
    SITE_FILE,
    Manifest,
    SiteTuning,
    archive_bundle,
    write_bundle,
)
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US
from tests.test_export import fitted


def _tuning(**overrides: object) -> SiteTuning:
    base = {
        "people_per_class": (0.0, 4.0, 12.0, 25.0),
        "service_rate_per_min": 6.0,
    }
    return SiteTuning(**{**base, **overrides})  # type: ignore[arg-type]


def _manifest() -> Manifest:
    return Manifest(name="campagne-juin", trained_at="2026-08-12", sessions=3)


def _model() -> onnx.ModelProto:
    proto, _, _ = fitted()
    return proto


def test_site_json_mirrors_what_the_appliance_reads(tmp_path: Path) -> None:
    write_bundle(tmp_path / "b", _model(), _tuning(), _manifest())

    payload = json.loads((tmp_path / "b" / SITE_FILE).read_text(encoding="utf-8"))

    # Field for field: the appliance deserializes this straight into its own
    # tuning type, so a renamed key is a bundle it silently refuses.
    assert set(payload) == {
        "people_per_class",
        "service_rate_per_min",
        "smoothing_tau_s",
        "hysteresis_margin",
        "min_confidence",
        "window_us",
        "hop_us",
    }
    assert payload["people_per_class"] == [0.0, 4.0, 12.0, 25.0]


def test_the_training_window_travels_with_the_model(tmp_path: Path) -> None:
    # The one value that must never be chosen independently of the run: a
    # model trained on five-second windows applied to three-second ones gives
    # estimates that look plausible and are not.
    write_bundle(
        tmp_path / "b", _model(), _tuning(window_us=7_000_000, hop_us=500_000), _manifest()
    )

    payload = json.loads((tmp_path / "b" / SITE_FILE).read_text(encoding="utf-8"))

    assert payload["window_us"] == 7_000_000
    assert payload["hop_us"] == 500_000


def test_defaults_come_from_the_shared_window_constants(tmp_path: Path) -> None:
    write_bundle(tmp_path / "b", _model(), _tuning(), _manifest())

    payload = json.loads((tmp_path / "b" / SITE_FILE).read_text(encoding="utf-8"))

    assert payload["window_us"] == DEFAULT_WINDOW_US
    assert payload["hop_us"] == DEFAULT_HOP_US


def test_the_bundle_holds_both_files(tmp_path: Path) -> None:
    bundle = write_bundle(tmp_path / "b", _model(), _tuning(), _manifest())

    assert (bundle / MODEL_FILE).stat().st_size > 0
    assert (bundle / SITE_FILE).exists()


def test_the_archive_stores_members_at_its_root(tmp_path: Path) -> None:
    bundle = write_bundle(tmp_path / "b", _model(), _tuning(), _manifest())

    archive = archive_bundle(bundle, tmp_path / "bundle.tar.gz")

    with tarfile.open(archive, "r:gz") as opened:
        names = sorted(opened.getnames())
    # Two known names rather than a prefix the appliance would have to guess.
    assert names == sorted([MODEL_FILE, SITE_FILE, MANIFEST_FILE])


def test_a_model_says_what_it_is_and_when_it_was_trained(tmp_path: Path) -> None:
    # The appliance can date the moment it received a bundle, but not the run
    # that produced it — and the two answer different questions when estimates
    # change.
    bundle = write_bundle(tmp_path / "b", _model(), _tuning(), _manifest())

    payload = json.loads((bundle / MANIFEST_FILE).read_text(encoding="utf-8"))

    assert payload == {"name": "campagne-juin", "trained_at": "2026-08-12", "sessions": 3}
