# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false
# pyright: reportUnknownVariableType=false, reportUnknownArgumentType=false
"""End-to-end pipeline validation on synthetic separable sessions.

This is the plumbing test of the whole v1 approach: if a simple classifier
cannot separate cleanly separable synthetic classes through our windows and
features, the pipeline is broken somewhere.
"""

import numpy as np
import pytest

from flow_ml import (
    DensityClass,
    Session,
    build_dataset,
    evaluate_grouped,
    make_classifier,
    synthetic_session,
)

SECOND = 1_000_000


def make_sessions(count: int) -> list[Session]:
    return [
        synthetic_session(
            f"synthetic-{seed:03}",
            seed=seed,
            seconds_per_class=12.0,
            frame_rate_hz=10.0,
            subcarriers=8,
        )
        for seed in range(count)
    ]


def test_synthetic_session_is_deterministic_and_well_formed() -> None:
    a = synthetic_session("s", seed=7, seconds_per_class=2.0, frame_rate_hz=10.0)
    b = synthetic_session("s", seed=7, seconds_per_class=2.0, frame_rate_hz=10.0)
    assert a == b

    other = synthetic_session("s", seed=8, seconds_per_class=2.0, frame_rate_hz=10.0)
    assert other != a

    assert [label.density for label in a.labels] == list(DensityClass)
    timestamps = [frame.ts_us for frame in a.frames]
    assert timestamps == sorted(timestamps)
    assert len(a.frames) == 4 * 20  # 4 classes × 2 s × 10 Hz


def test_build_dataset_stacks_sessions_with_groups() -> None:
    sessions = make_sessions(2)
    x, y, groups = build_dataset(sessions, window_us=5 * SECOND, hop_us=2 * SECOND)
    assert x.shape[0] == y.shape[0] == groups.shape[0]
    assert set(groups.tolist()) == {0, 1}
    assert set(y.tolist()) <= {0, 1, 2, 3}


def test_classifier_separates_synthetic_classes_across_sessions() -> None:
    report = evaluate_grouped(make_sessions(6), n_splits=3, hop_us=2 * SECOND)

    # Balanced classes: the majority baseline sits near chance (~0.25).
    assert report.baseline_accuracy < 0.35
    # Cleanly separable synthetic data: anything below this means the
    # plumbing (windows, features, split) is broken, not the model.
    assert report.accuracy > 0.9
    assert report.accuracy > report.baseline_accuracy + 0.4

    assert report.confusion.shape == (4, 4)
    total = int(report.confusion.sum())
    assert total == report.n_windows
    diagonal = int(np.trace(report.confusion))
    assert diagonal / total == pytest.approx(report.accuracy)
    assert report.n_sessions == 6

    text = report.format()
    assert "accuracy" in text
    assert "saturated" in text


def test_the_report_names_the_sessions_it_covered() -> None:
    report = evaluate_grouped(make_sessions(3), n_splits=3, hop_us=2 * SECOND)

    assert [entry.session_id for entry in report.sessions] == [
        "synthetic-000",
        "synthetic-001",
        "synthetic-002",
    ]
    assert sum(entry.windows for entry in report.sessions) == report.n_windows
    assert all(sum(entry.support) == entry.windows for entry in report.sessions)


def test_the_report_records_the_receivers_it_was_trained_against() -> None:
    report = evaluate_grouped(make_sessions(3), n_splits=3, hop_us=2 * SECOND)

    assert report.receivers == make_sessions(1)[0].rx_node_ids


def test_a_session_with_no_usable_window_is_absent_rather_than_empty() -> None:
    # It took no part in the evaluation, so listing it with zeroes would
    # suggest a capture that contributed nothing rather than one that was
    # never read.
    sessions = make_sessions(3)
    unlabelled = Session(meta=sessions[0].meta, frames=sessions[0].frames, labels=())
    report = evaluate_grouped([*sessions[1:], unlabelled], n_splits=2, hop_us=2 * SECOND)

    assert [entry.session_id for entry in report.sessions] == [
        "synthetic-001",
        "synthetic-002",
    ]


def test_evaluate_rejects_more_splits_than_sessions() -> None:
    with pytest.raises(ValueError, match="only 2 sessions"):
        evaluate_grouped(make_sessions(2), n_splits=3, hop_us=2 * SECOND)


def test_fitted_classifier_exposes_probabilities() -> None:
    # The ADR 0002 contract: the model's canonical output is a probability
    # distribution over the four classes.
    sessions = make_sessions(2)
    x, y, _ = build_dataset(sessions, hop_us=2 * SECOND)
    classifier = make_classifier().fit(x, y)
    probabilities = np.asarray(classifier.predict_proba(x))
    assert probabilities.shape == (x.shape[0], 4)
    assert np.allclose(probabilities.sum(axis=1), 1.0)
