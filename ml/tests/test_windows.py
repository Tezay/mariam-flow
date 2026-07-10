"""Windowing and label-alignment tests with hand-computed expectations."""

from pathlib import Path

import pytest

from flow_ml import DensityClass, Label, label_at, load_session, sliding_windows
from tests.helpers import frame_obj, label_obj, write_session

SECOND = 1_000_000


def test_label_at_is_a_step_function() -> None:
    labels = (
        Label(ts_us=10, density=DensityClass.LOW),
        Label(ts_us=20, density=DensityClass.SATURATED),
    )
    assert label_at(labels, 9) is None
    assert label_at(labels, 10) is DensityClass.LOW
    assert label_at(labels, 19) is DensityClass.LOW
    assert label_at(labels, 20) is DensityClass.SATURATED
    assert label_at(labels, 1_000_000) is DensityClass.SATURATED


def test_window_count_and_boundaries(tmp_path: Path) -> None:
    # One frame per second from t=0 s to t=12 s inclusive (13 frames).
    # Window 5 s, hop 1 s: valid starts are 0..7 s → 8 windows.
    frames = [frame_obj(t * SECOND) for t in range(13)]
    directory = write_session(tmp_path, frames=frames, labels=[label_obj(0, 2)])
    session = load_session(directory)

    windows = sliding_windows(session)
    assert len(windows) == 8
    first = windows[0]
    assert (first.start_us, first.end_us) == (0, 5 * SECOND)
    assert len(first.frames) == 5  # t = 0..4 s; end is exclusive
    assert first.duration_us == 5 * SECOND
    assert all(w.label is DensityClass.MEDIUM for w in windows)


def test_window_label_is_taken_at_center(tmp_path: Path) -> None:
    frames = [frame_obj(t * SECOND) for t in range(13)]
    # State becomes LOW at t=3 s: window [0,5) has center 2.5 s → unlabeled.
    directory = write_session(tmp_path, frames=frames, labels=[label_obj(3 * SECOND, 1)])
    session = load_session(directory)

    windows = sliding_windows(session)
    assert windows[0].label is None
    assert windows[1].label is DensityClass.LOW  # center 3.5 s


def test_capture_gaps_produce_no_empty_windows(tmp_path: Path) -> None:
    # Frames during [0,2] s and [20,22] s; nothing in between.
    frames = [frame_obj(t * SECOND) for t in (0, 1, 2, 20, 21, 22)]
    directory = write_session(tmp_path, frames=frames, labels=[label_obj(0, 0)])
    session = load_session(directory)

    windows = sliding_windows(session)
    assert windows, "expected some windows"
    assert all(w.frames for w in windows)


def test_invalid_parameters_are_rejected(tmp_path: Path) -> None:
    directory = write_session(tmp_path, frames=[frame_obj(0)], labels=[])
    session = load_session(directory)
    with pytest.raises(ValueError, match="positive"):
        sliding_windows(session, window_us=0)
    with pytest.raises(ValueError, match="positive"):
        sliding_windows(session, hop_us=-1)


def test_empty_session_yields_no_windows(tmp_path: Path) -> None:
    directory = write_session(tmp_path, frames=[], labels=[])
    assert sliding_windows(load_session(directory)) == ()
