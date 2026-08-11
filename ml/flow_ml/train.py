# pyright: reportUnknownMemberType=false
"""Turning recorded captures into a bundle an appliance can import.

Runs off the appliance. A deployed unit carries no Python runtime, and the
board it runs on has neither the memory nor the time for a training run:
captures are exported from the dashboard, trained on here, and the resulting
bundle is imported back.

    uv run python -m flow_ml.train --sessions data/sessions \\
                                   --out models --name campagne-juin
    uv run python -m flow_ml.train --demo 6 --out /tmp/models --name essai
"""

from __future__ import annotations

import argparse
import datetime
import sys
from collections.abc import Sequence
from pathlib import Path

import numpy as np
import numpy.typing as npt

from flow_ml.bundle import AnalysisWindow, Manifest, archive_bundle, write_bundle
from flow_ml.export import export_pipeline
from flow_ml.session import DensityClass, Session, load_sessions
from flow_ml.synthetic import synthetic_session
from flow_ml.training import build_dataset, evaluate_grouped, make_classifier
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US


def train(
    sessions: Sequence[Session],
    *,
    name: str,
    out: Path,
    window_us: int = DEFAULT_WINDOW_US,
    hop_us: int = DEFAULT_HOP_US,
    splits: int = 3,
) -> Path:
    """Evaluates, fits on everything, and writes the bundle archive."""
    report = evaluate_grouped(sessions, n_splits=splits, window_us=window_us, hop_us=hop_us)
    print(report.format())

    x, y, _ = build_dataset(sessions, window_us=window_us, hop_us=hop_us)
    refuse_missing_classes(y)
    classifier = make_classifier()
    classifier.fit(x, y)

    # The window the run was trained under travels with the weights: a model
    # fed windows of another length sees a signal it was never shown.
    manifest = Manifest(
        name=name,
        trained_at=datetime.date.today().isoformat(),
        sessions=report.n_sessions,
    )
    directory = write_bundle(
        out / name,
        export_pipeline(classifier),
        AnalysisWindow(window_us=window_us, hop_us=hop_us),
        manifest,
        report,
    )
    return archive_bundle(directory, out / f"{name}.tar.gz")


def refuse_missing_classes(y: npt.NDArray[np.int64]) -> None:
    """Refuses a campaign that never observed one of the four classes.

    The classifier fits the classes it is shown, so one absent from the labels
    is absent from the exported graph's output — which the appliance refuses
    (ADR 0002).
    """
    seen = set(y.tolist())
    missing = [density for density in DensityClass if int(density) not in seen]
    if missing:
        names = ", ".join(density.name.lower() for density in missing)
        raise ValueError(
            f"no window is labelled {names}: the four classes are the model's fixed "
            f"output, so a campaign that never observed one cannot produce a usable "
            f"model — record that level, or widen the windows over the marks you have"
        )


def main(argv: Sequence[str] | None = None) -> None:
    """Entry point: ``python -m flow_ml.train``."""
    parser = argparse.ArgumentParser(
        prog="python -m flow_ml.train",
        description="Train the density classifier and write an importable bundle.",
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument(
        "--sessions",
        type=Path,
        help="root holding session directories or exported .tar.gz archives",
    )
    source.add_argument("--demo", type=int, metavar="N", help="use N synthetic sessions")
    parser.add_argument("--out", type=Path, required=True, help="where the bundle is written")
    parser.add_argument("--name", required=True, help="name the model carries in its manifest")
    parser.add_argument("--window-us", type=int, default=DEFAULT_WINDOW_US)
    parser.add_argument("--hop-us", type=int, default=DEFAULT_HOP_US)
    parser.add_argument(
        "--splits",
        type=int,
        default=3,
        help="cross-validation folds; needs at least this many sessions",
    )
    args = parser.parse_args(argv)

    if args.demo is not None:
        sessions = [synthetic_session(f"demo-{i}", seed=i) for i in range(args.demo)]
    else:
        sessions = load_sessions(args.sessions)
    if not sessions:
        parser.error(f"no session found under {args.sessions}")
    if len(sessions) < args.splits:
        parser.error(
            f"{len(sessions)} session(s) but --splits {args.splits}: a model is evaluated by "
            f"holding whole sessions out, so record more or lower --splits"
        )

    print(f"{len(sessions)} session(s): {', '.join(s.meta.session_id for s in sessions)}\n")
    # A dataset the run cannot use is the operator's problem to fix, not a
    # traceback: every refusal below names what is missing and what to do.
    try:
        archive = train(
            sessions,
            name=args.name,
            out=args.out,
            window_us=args.window_us,
            hop_us=args.hop_us,
            splits=args.splits,
        )
    except ValueError as refusal:
        parser.error(str(refusal))
    print(f"\nbundle: {archive}")
    print("import it from the dashboard, Calibration → Models")


if __name__ == "__main__":  # pragma: no cover
    main(sys.argv[1:])
