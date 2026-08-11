"""The command that turns recorded captures into an importable bundle."""

from __future__ import annotations

import json
import tarfile
from pathlib import Path

import pytest

from flow_ml.bundle import ANALYSIS_FILE, EVALUATION_FILE, MANIFEST_FILE, MODEL_FILE
from flow_ml.session import load_sessions
from flow_ml.train import main
from tests.helpers import frame_obj, label_obj, meta_obj, write_session


def _session(root: Path, session_id: str) -> Path:
    """One session with two labelled stretches, enough to window over."""
    frames = [
        frame_obj(1_000_000 + step * 50_000, amp=(1.0 + step % 3, 2.0)) for step in range(240)
    ]
    labels = [label_obj(1_000_000, 0), label_obj(7_000_000, 2)]
    return write_session(root, frames=frames, labels=labels, meta=meta_obj(session_id=session_id))


def _archive(directory: Path, destination: Path) -> Path:
    """Packs a session the way the appliance exports it: under its own id."""
    with tarfile.open(destination, "w:gz") as archive:
        archive.add(directory, arcname=directory.name)
    return destination


def test_a_session_loads_from_the_archive_the_appliance_exports(tmp_path: Path) -> None:
    # What lands in a downloads folder is the gzipped tar, not a directory.
    source = _session(tmp_path / "source", "s-001")
    exported = tmp_path / "captures"
    exported.mkdir()
    _archive(source, exported / "s-001.tar.gz")

    sessions = load_sessions(exported)

    assert [s.meta.session_id for s in sessions] == ["s-001"]


def test_directories_and_archives_load_side_by_side(tmp_path: Path) -> None:
    root = tmp_path / "captures"
    root.mkdir()
    _session(root, "s-directory")
    _archive(_session(tmp_path / "source", "s-archived"), root / "s-archived.tar.gz")

    sessions = load_sessions(root)

    assert [s.meta.session_id for s in sessions] == ["s-archived", "s-directory"]


def test_an_unfinished_capture_is_skipped(tmp_path: Path) -> None:
    root = tmp_path / "captures"
    root.mkdir()
    _session(root, "s-sealed")
    (root / "s-running.recording").mkdir()

    assert [s.meta.session_id for s in load_sessions(root)] == ["s-sealed"]


def test_the_command_writes_a_bundle_the_appliance_accepts(tmp_path: Path) -> None:
    main(["--demo", "4", "--out", str(tmp_path), "--name", "essai", "--splits", "2"])

    archive = tmp_path / "essai.tar.gz"
    with tarfile.open(archive) as opened:
        names = sorted(opened.getnames())
        analysis = json.load(opened.extractfile(ANALYSIS_FILE))  # type: ignore[arg-type]
        manifest = json.load(opened.extractfile(MANIFEST_FILE))  # type: ignore[arg-type]

    assert names == sorted([MODEL_FILE, ANALYSIS_FILE, MANIFEST_FILE, EVALUATION_FILE])
    assert set(analysis) == {"window_us", "hop_us"}
    assert manifest["name"] == "essai"
    assert manifest["sessions"] == 4


def test_the_bundle_carries_the_scores_the_run_printed(tmp_path: Path) -> None:
    # The evaluation the command prints and the one it ships are the same
    # object: a second computation could disagree with the first.
    main(["--demo", "4", "--out", str(tmp_path), "--name", "essai", "--splits", "2"])

    with tarfile.open(tmp_path / "essai.tar.gz") as opened:
        evaluation = json.load(opened.extractfile(EVALUATION_FILE))  # type: ignore[arg-type]

    assert evaluation["splits"] == 2
    assert 0.0 <= evaluation["accuracy"] <= 1.0
    assert sum(sum(row) for row in evaluation["confusion"]) == evaluation["windows"]
    assert [entry["session_id"] for entry in evaluation["sessions"]] == [
        f"demo-{index}" for index in range(4)
    ]


def test_the_manifest_counts_the_captures_that_actually_trained_it(tmp_path: Path) -> None:
    # The manifest's count and the evaluation's list must not disagree: a
    # capture with no usable window took no part and belongs in neither.
    main(["--demo", "3", "--out", str(tmp_path), "--name", "essai", "--splits", "3"])

    with tarfile.open(tmp_path / "essai.tar.gz") as opened:
        manifest = json.load(opened.extractfile(MANIFEST_FILE))  # type: ignore[arg-type]
        evaluation = json.load(opened.extractfile(EVALUATION_FILE))  # type: ignore[arg-type]

    assert manifest["sessions"] == len(evaluation["sessions"])


def test_the_window_trained_under_is_the_one_recorded(tmp_path: Path) -> None:
    # A model fed windows of another length sees a signal it was never shown,
    # so the geometry has to reach the bundle rather than a default.
    main(
        ["--demo", "3", "--out", str(tmp_path), "--name", "wide", "--splits", "3"]
        + ["--window-us", "7000000", "--hop-us", "2000000"]
    )

    with tarfile.open(tmp_path / "wide.tar.gz") as opened:
        analysis = json.load(opened.extractfile(ANALYSIS_FILE))  # type: ignore[arg-type]

    assert analysis == {"window_us": 7_000_000, "hop_us": 2_000_000}


def test_a_campaign_that_never_observed_a_class_is_refused_by_name(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    # `_session` marks empty and medium only, which is what makes it the
    # fixture for this case.
    root = tmp_path / "captures"
    root.mkdir()
    for index in range(3):
        _session(root, f"s-{index}")

    with pytest.raises(SystemExit):
        main(["--sessions", str(root), "--out", str(tmp_path), "--name", "trou", "--splits", "3"])

    named = capsys.readouterr().err.split("labelled ", 1)[1].split(":", 1)[0]
    assert named == "low, saturated", "the refusal names exactly what is missing"
    assert not (tmp_path / "trou.tar.gz").exists(), "a refused run leaves nothing behind"


def test_fewer_sessions_than_folds_is_refused_by_name(tmp_path: Path) -> None:
    # Sessions are the split unit, so two of them cannot be cut into three
    # folds — and the message has to say what to do rather than raise.
    with pytest.raises(SystemExit):
        main(["--demo", "2", "--out", str(tmp_path), "--name", "trop-peu", "--splits", "3"])
