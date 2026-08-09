"""The deployable output of a training run.

A model, the analysis geometry it was trained under and the scores it earned
are produced by the same run and ship together: pairing a model with a window
it was never trained on produces estimates that look plausible and are not.

What a site turns a density into a waiting time with is deliberately absent.
Nothing in a training run counts heads, so how many people a class represents
is a human observation — it is answered on the appliance and belongs to the
site, not to the model.
"""

from __future__ import annotations

import json
import tarfile
from dataclasses import dataclass
from pathlib import Path

import onnx

from flow_ml.training import EvaluationReport
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US

MODEL_FILE = "model.onnx"
ANALYSIS_FILE = "analysis.json"
MANIFEST_FILE = "model.json"
EVALUATION_FILE = "evaluation.json"


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
class AnalysisWindow:
    """The geometry the run was trained under.

    Chosen by the training run and never independently of it: a model fed
    windows of a different length sees a signal it was never shown.
    """

    window_us: int = DEFAULT_WINDOW_US
    hop_us: int = DEFAULT_HOP_US

    def as_dict(self) -> dict[str, object]:
        """The `analysis.json` payload, field for field as the appliance reads it."""
        return {"window_us": self.window_us, "hop_us": self.hop_us}


def write_bundle(
    out_dir: Path,
    model: onnx.ModelProto,
    analysis: AnalysisWindow,
    manifest: Manifest,
    evaluation: EvaluationReport,
) -> Path:
    """Writes the model, its analysis geometry, its manifest and its scores."""
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / MODEL_FILE).write_bytes(model.SerializeToString())
    (out_dir / ANALYSIS_FILE).write_text(
        json.dumps(analysis.as_dict(), indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / MANIFEST_FILE).write_text(
        json.dumps(manifest.as_dict(), indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / EVALUATION_FILE).write_text(
        json.dumps(evaluation.as_dict(), indent=2) + "\n", encoding="utf-8"
    )
    return out_dir


def archive_bundle(bundle_dir: Path, destination: Path) -> Path:
    """Packs a bundle as a gzipped tar the appliance accepts.

    Members are stored at the archive root rather than under a directory, so
    the appliance reads known names instead of guessing a prefix.
    """
    with tarfile.open(destination, "w:gz") as archive:
        for name in (MODEL_FILE, ANALYSIS_FILE, MANIFEST_FILE, EVALUATION_FILE):
            archive.add(bundle_dir / name, arcname=name)
    return destination
