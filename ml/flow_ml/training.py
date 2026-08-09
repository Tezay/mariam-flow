# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false
# pyright: reportUnknownVariableType=false, reportUnknownArgumentType=false
"""Training and honest evaluation of the v1 density classifier.

The model is a multinomial logistic regression over standardized features:
per window it produces a probability for each of the four classes (the
softmax of four linear scores), which is exactly the output contract of
ADR 0002 — the discrete class is the argmax, downstream consumers use the
full distribution.

Evaluation follows two non-negotiable rules:

- **Split by session** (``GroupKFold``): adjacent windows of one capture
  overlap and are heavily correlated; letting one session straddle the
  train/test boundary would leak and inflate accuracy. Every session is
  tested exactly once, by a model that never saw it.
- **Beat the baseline**: accuracy is only meaningful against the dumbest
  strategy (always predict the training set's majority class). A model
  that does not clearly beat it has learned nothing.

The pyright relaxations at the top of this file are the boundary with
scikit-learn, which ships no type information; the rest of the package
stays strictly typed.
"""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from typing import cast

import numpy as np
import numpy.typing as npt
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import confusion_matrix
from sklearn.model_selection import GroupKFold
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler

from flow_ml.features import session_dataset
from flow_ml.session import DensityClass, Session
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US

EVALUATION_SCHEMA = 1
"""Version of the `evaluation.json` payload.

The appliance reading it may be older than the run that wrote it, and reports
a version it does not know as no evaluation rather than guessing at fields.
"""


def make_classifier() -> Pipeline:
    """Standardization followed by multinomial logistic regression.

    Standardization (z-score per feature) matters because the features
    live on wildly different scales (mean amplitude ≈ 10, correlation in
    [−1, 1], frame rate ≈ 20): L2-regularized logistic regression
    penalizes all coefficients uniformly, so unscaled features would be
    regularized arbitrarily harder or softer depending on their unit.
    """
    return Pipeline(
        [
            ("scale", StandardScaler()),
            ("model", LogisticRegression(max_iter=1000)),
        ]
    )


def build_dataset(
    sessions: Sequence[Session],
    *,
    window_us: int = DEFAULT_WINDOW_US,
    hop_us: int = DEFAULT_HOP_US,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.int64], npt.NDArray[np.int64]]:
    """Stacks the per-session datasets into ``(X, y, groups)``.

    ``groups[i]`` is the index of the session that produced row ``i`` —
    the unit of the train/test split.
    """
    xs: list[npt.NDArray[np.float64]] = []
    ys: list[npt.NDArray[np.int64]] = []
    gs: list[npt.NDArray[np.int64]] = []
    for index, session in enumerate(sessions):
        x, y = session_dataset(session, window_us=window_us, hop_us=hop_us)
        if y.shape[0] == 0:
            continue
        xs.append(x)
        ys.append(y)
        gs.append(np.full(y.shape[0], index, dtype=np.int64))
    if not xs:
        raise ValueError("no usable labeled windows in any session")
    return np.vstack(xs), np.concatenate(ys), np.concatenate(gs)


@dataclass(frozen=True)
class SessionBreakdown:
    """What one recorded capture contributed to a run."""

    session_id: str
    windows: int
    support: tuple[int, ...]
    """Windows per class, in 0..3 order."""


@dataclass(frozen=True)
class EvaluationReport:
    """Pooled result of a session-grouped cross-validation."""

    accuracy: float
    baseline_accuracy: float
    confusion: npt.NDArray[np.int64]
    """Rows: true class, columns: predicted class, in 0..3 order."""
    n_windows: int
    splits: int
    receivers: tuple[str, ...]
    sessions: tuple[SessionBreakdown, ...]

    @property
    def n_sessions(self) -> int:
        """Sessions that produced at least one usable window."""
        return len(self.sessions)

    def format(self) -> str:
        """Human-readable summary with the confusion matrix."""
        names = [density.name.lower() for density in DensityClass]
        width = max(len(name) for name in names) + 2
        lines = [
            f"accuracy: {self.accuracy:.3f}   "
            f"baseline (majority class): {self.baseline_accuracy:.3f}",
            f"windows: {self.n_windows}   sessions: {self.n_sessions}",
            "confusion (rows = truth, columns = prediction):",
            " " * width + "".join(name.rjust(width) for name in names),
        ]
        for i, name in enumerate(names):
            row = "".join(str(int(v)).rjust(width) for v in self.confusion[i])
            lines.append(name.rjust(width) + row)
        return "\n".join(lines)

    def as_dict(self) -> dict[str, object]:
        """The `evaluation.json` payload, field for field as it ships.

        Carries the confusion matrix in the orientation this module computes
        it — rows are truth — because a matrix read the other way round
        inverts every conclusion drawn from it.
        """
        return {
            "schema": EVALUATION_SCHEMA,
            "accuracy": self.accuracy,
            "baseline_accuracy": self.baseline_accuracy,
            "confusion": [[int(v) for v in row] for row in self.confusion],
            "windows": self.n_windows,
            "splits": self.splits,
            "receivers": list(self.receivers),
            "sessions": [
                {
                    "session_id": session.session_id,
                    "windows": session.windows,
                    "support": list(session.support),
                }
                for session in self.sessions
            ],
        }


def evaluate_grouped(
    sessions: Sequence[Session],
    *,
    n_splits: int = 3,
    window_us: int = DEFAULT_WINDOW_US,
    hop_us: int = DEFAULT_HOP_US,
) -> EvaluationReport:
    """Cross-validates the classifier with sessions as split units.

    Predictions of all folds are pooled into one report, so every window
    is predicted exactly once by a model that never saw its session.
    """
    x, y, groups = build_dataset(sessions, window_us=window_us, hop_us=hop_us)
    n_groups = int(np.unique(groups).shape[0])
    if n_splits > n_groups:
        raise ValueError(f"n_splits={n_splits} but only {n_groups} sessions with usable windows")

    y_true: list[npt.NDArray[np.int64]] = []
    y_pred: list[npt.NDArray[np.int64]] = []
    y_base: list[npt.NDArray[np.int64]] = []
    for train_index, test_index in GroupKFold(n_splits=n_splits).split(x, y, groups):
        classifier = make_classifier()
        classifier.fit(x[train_index], y[train_index])
        predictions = cast(
            npt.NDArray[np.int64],
            np.asarray(classifier.predict(x[test_index]), dtype=np.int64),
        )
        majority = np.bincount(y[train_index]).argmax()
        y_true.append(y[test_index])
        y_pred.append(predictions)
        y_base.append(np.full(test_index.shape[0], majority, dtype=np.int64))

    truth = np.concatenate(y_true)
    predicted = np.concatenate(y_pred)
    baseline = np.concatenate(y_base)
    matrix = cast(
        npt.NDArray[np.int64],
        np.asarray(
            confusion_matrix(truth, predicted, labels=[int(d) for d in DensityClass]),
            dtype=np.int64,
        ),
    )
    return EvaluationReport(
        accuracy=float(np.mean(truth == predicted)),
        baseline_accuracy=float(np.mean(truth == baseline)),
        confusion=matrix,
        n_windows=int(truth.shape[0]),
        splits=n_splits,
        receivers=_receivers(sessions, groups),
        sessions=_breakdown(sessions, y, groups),
    )


def _breakdown(
    sessions: Sequence[Session],
    y: npt.NDArray[np.int64],
    groups: npt.NDArray[np.int64],
) -> tuple[SessionBreakdown, ...]:
    """Per-session window counts, in the order the sessions were read.

    ``groups`` holds the index a row came from in `sessions`, so a capture
    that yielded nothing is absent here rather than present with zeroes —
    it took no part in the evaluation.
    """
    classes = len(DensityClass)
    return tuple(
        SessionBreakdown(
            session_id=sessions[index].meta.session_id,
            windows=int(np.count_nonzero(groups == index)),
            support=tuple(
                int(count) for count in np.bincount(y[groups == index], minlength=classes)
            ),
        )
        for index in sorted(set(groups.tolist()))
    )


def _receivers(sessions: Sequence[Session], groups: npt.NDArray[np.int64]) -> tuple[str, ...]:
    """Receivers the run was trained against.

    Taken from the first contributing session: the feature matrix is one
    row per window with one block per receiver, so sessions declaring
    different receivers could not have been stacked in the first place.
    """
    return sessions[int(groups[0])].rx_node_ids
