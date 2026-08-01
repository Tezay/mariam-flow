"""The deployable output of a training run.

An appliance needs two things to estimate, and they are produced by the same
run: the model, and the site tuning it was trained under. Shipping them apart
is an invitation to pair a model with a window it was never trained on, which
produces estimates that look plausible and are not.

The bundle is therefore a directory holding ``model.onnx`` and ``site.json``,
both already canonical formats — ``site.json`` is what the laboratory tools
read, and the appliance mirrors it field for field.
"""

from __future__ import annotations

import json
import tarfile
from dataclasses import dataclass
from pathlib import Path

import onnx

from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US

MODEL_FILE = "model.onnx"
SITE_FILE = "site.json"
MANIFEST_FILE = "model.json"


@dataclass(frozen=True)
class Manifest:
    """What a model says about itself.

    An appliance can date the moment it received a bundle, but not the run
    that produced it — and the two answer different questions when estimates
    change. Both are needed to tell "trained in June" from "installed today".
    """

    name: str
    trained_at: str
    sessions: int = 0

    def as_dict(self) -> dict[str, object]:
        return {"name": self.name, "trained_at": self.trained_at, "sessions": self.sessions}


@dataclass(frozen=True)
class SiteTuning:
    """What the appliance needs besides the weights.

    ``window_us`` and ``hop_us`` come from the training run and must never be
    chosen independently of it. The rest are operational values a site can
    revise from the dashboard without retraining.
    """

    people_per_class: tuple[float, float, float, float]
    service_rate_per_min: float
    window_us: int = DEFAULT_WINDOW_US
    hop_us: int = DEFAULT_HOP_US
    smoothing_tau_s: float = 30.0
    hysteresis_margin: float = 0.15
    min_confidence: float = 0.5

    def as_dict(self) -> dict[str, object]:
        """The `site.json` payload, field for field as the appliance reads it."""
        return {
            "people_per_class": list(self.people_per_class),
            "service_rate_per_min": self.service_rate_per_min,
            "smoothing_tau_s": self.smoothing_tau_s,
            "hysteresis_margin": self.hysteresis_margin,
            "min_confidence": self.min_confidence,
            "window_us": self.window_us,
            "hop_us": self.hop_us,
        }


def write_bundle(
    out_dir: Path,
    model: onnx.ModelProto,
    tuning: SiteTuning,
    manifest: Manifest,
) -> Path:
    """Writes the model, its tuning and its manifest, and returns the directory."""
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / MODEL_FILE).write_bytes(model.SerializeToString())
    (out_dir / SITE_FILE).write_text(
        json.dumps(tuning.as_dict(), indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / MANIFEST_FILE).write_text(
        json.dumps(manifest.as_dict(), indent=2) + "\n", encoding="utf-8"
    )
    return out_dir


def archive_bundle(bundle_dir: Path, destination: Path) -> Path:
    """Packs a bundle as a gzipped tar the appliance accepts.

    Members are stored at the archive root rather than under a directory, so
    the appliance reads two known names instead of guessing a prefix.
    """
    with tarfile.open(destination, "w:gz") as archive:
        for name in (MODEL_FILE, SITE_FILE, MANIFEST_FILE):
            archive.add(bundle_dir / name, arcname=name)
    return destination
