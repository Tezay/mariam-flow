# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false
# pyright: reportUnknownVariableType=false, reportUnknownArgumentType=false
"""Structural tests of the visual reports (no image comparison — those
are brittle; we pin structure and that real PNG files come out)."""

from pathlib import Path

import pytest

from flow_ml import evaluate_grouped, synthetic_session
from flow_ml.report import (
    demo_sessions,
    evaluation_figure,
    load_sessions_dir,
    main,
    session_figure,
)
from tests.helpers import frame_obj, label_obj, write_session

SECOND = 1_000_000


def small_session(seed: int = 1):
    return synthetic_session(
        f"tiny-{seed}",
        seed=seed,
        seconds_per_class=6.0,
        frame_rate_hz=8.0,
        subcarriers=6,
    )


def test_session_figure_structure(tmp_path: Path) -> None:
    figure = session_figure(small_session(), hop_us=2 * SECOND)
    # One heatmap per RX node + label band + features panel.
    assert len(figure.axes) >= 3
    assert "tiny-1" in figure.get_suptitle()

    target = tmp_path / "session.png"
    figure.savefig(target)
    assert target.stat().st_size > 10_000, "expected a real PNG"


def test_session_figure_rejects_empty_sessions(tmp_path: Path) -> None:
    directory = write_session(tmp_path, frames=[], labels=[])
    from flow_ml import load_session

    with pytest.raises(ValueError, match="no frames"):
        session_figure(load_session(directory))


def test_evaluation_figure_renders_scores(tmp_path: Path) -> None:
    sessions = [small_session(seed) for seed in range(4)]
    report = evaluate_grouped(sessions, n_splits=2, hop_us=2 * SECOND)
    figure = evaluation_figure(report)
    title = figure.axes[0].get_title()
    assert "accuracy" in title
    assert "baseline" in title

    target = tmp_path / "evaluation.png"
    figure.savefig(target)
    assert target.stat().st_size > 10_000


def test_main_demo_writes_all_reports(tmp_path: Path) -> None:
    out = tmp_path / "report"
    main(["--demo", "2", "--out", str(out), "--hop-us", str(2 * SECOND)])
    assert (out / "demo-00.png").exists()
    assert (out / "demo-01.png").exists()
    assert (out / "evaluation.png").exists()


def test_main_reads_sessions_directory(tmp_path: Path) -> None:
    frames = [frame_obj(t * SECOND // 2) for t in range(41)]
    write_session(tmp_path / "sessions", frames=frames, labels=[label_obj(0, 2)])
    out = tmp_path / "report"
    main(["--sessions", str(tmp_path / "sessions"), "--out", str(out)])
    assert (out / "s-test-001.png").exists()
    # A single labeled session cannot be cross-validated.
    assert not (out / "evaluation.png").exists()


def test_load_sessions_dir_skips_recording_directories(tmp_path: Path) -> None:
    frames = [frame_obj(0), frame_obj(1)]
    write_session(tmp_path, frames=frames, labels=[])
    (tmp_path / "crashed.recording").mkdir()
    sessions = load_sessions_dir(tmp_path)
    assert [s.meta.session_id for s in sessions] == ["s-test-001"]


def test_demo_sessions_are_labeled_and_deterministic() -> None:
    a = demo_sessions(2)
    b = demo_sessions(2)
    assert a == b
    assert all(session.labels for session in a)
