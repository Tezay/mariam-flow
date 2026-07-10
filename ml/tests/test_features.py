"""Feature extraction tests against hand-computed values."""

import math
from pathlib import Path

import numpy as np
import pytest

from flow_ml import (
    NODE_FEATURES,
    Frame,
    feature_names,
    load_session,
    node_features,
    session_dataset,
    sliding_windows,
    window_vector,
)
from tests.helpers import frame_obj, label_obj, meta_obj, write_session

SECOND = 1_000_000


def make_frame(ts_us: int, amp: tuple[float, ...], *, node_id: str = "rx-1") -> Frame:
    return Frame(
        ts_us=ts_us,
        node_id=node_id,
        rssi=-52,
        mcs=7,
        amp=amp,
        phase=tuple(0.0 for _ in amp),
    )


def test_constant_signal_has_zero_agitation() -> None:
    frames = [make_frame(t * SECOND, (2.0, 2.0)) for t in range(3)]
    vector = node_features(frames, duration_us=3 * SECOND)

    expected = {
        "amp_mean": 2.0,
        "amp_std": 0.0,
        "motion_energy": 0.0,
        "subcarrier_corr": 0.0,  # zero variance -> correlation carries nothing
        "rssi_mean": -52.0,
        "rssi_std": 0.0,
        "frame_rate": 1.0,
    }
    for name, value in expected.items():
        assert vector[NODE_FEATURES.index(name)] == pytest.approx(value), name


def test_linear_ramp_features_match_hand_computation() -> None:
    # Two identical subcarriers ramping 0 → 1 → 2.
    frames = [make_frame(t * SECOND, (float(t), float(t))) for t in range(3)]
    vector = node_features(frames, duration_us=3 * SECOND)

    assert vector[NODE_FEATURES.index("amp_mean")] == pytest.approx(1.0)
    # Population std of [0, 1, 2] is sqrt(2/3).
    assert vector[NODE_FEATURES.index("amp_std")] == pytest.approx(math.sqrt(2 / 3))
    assert vector[NODE_FEATURES.index("motion_energy")] == pytest.approx(1.0)
    assert vector[NODE_FEATURES.index("subcarrier_corr")] == pytest.approx(1.0)


def test_too_few_frames_are_rejected() -> None:
    with pytest.raises(ValueError, match="at least"):
        node_features([make_frame(0, (1.0, 2.0))], duration_us=SECOND)


def test_inconsistent_subcarrier_counts_are_rejected() -> None:
    frames = [make_frame(0, (1.0, 2.0)), make_frame(1, (1.0, 2.0, 3.0))]
    with pytest.raises(ValueError, match="inconsistent subcarrier counts"):
        node_features(frames, duration_us=SECOND)


def test_feature_names_follow_node_order() -> None:
    names = feature_names(("rx-1", "rx-2"))
    assert len(names) == 2 * len(NODE_FEATURES)
    assert names[0] == "rx-1:amp_mean"
    assert names[len(NODE_FEATURES)] == "rx-2:amp_mean"


def test_window_vector_is_none_when_a_node_is_missing(tmp_path: Path) -> None:
    # Two RX nodes declared, but only rx-1 has frames.
    frames = [frame_obj(t * SECOND) for t in range(6)]
    directory = write_session(
        tmp_path,
        frames=frames,
        labels=[label_obj(0, 1)],
        meta=meta_obj(rx_nodes=("rx-1", "rx-2")),
    )
    session = load_session(directory)
    windows = sliding_windows(session)
    assert windows
    assert window_vector(windows[0], session.rx_node_ids) is None


def test_session_dataset_shapes_and_labels(tmp_path: Path) -> None:
    # Two frames per second from 0 to 10 s, labeled MEDIUM with a count.
    frames = [frame_obj(t * SECOND // 2) for t in range(21)]
    directory = write_session(tmp_path, frames=frames, labels=[label_obj(0, 2, count=18)])
    session = load_session(directory)

    x, y = session_dataset(session)
    # Valid window starts: 0..5 s → 6 windows, all labeled and complete.
    assert x.shape == (6, len(NODE_FEATURES))
    assert x.dtype == np.float64
    assert y.tolist() == [2] * 6


def test_unlabeled_windows_are_dropped(tmp_path: Path) -> None:
    frames = [frame_obj(t * SECOND // 2) for t in range(21)]
    # First label only at t=6 s: windows centered earlier have no truth.
    directory = write_session(tmp_path, frames=frames, labels=[label_obj(6 * SECOND, 3)])
    session = load_session(directory)

    x, y = session_dataset(session)
    assert x.shape[0] < 6
    assert set(y.tolist()) == {3}


def test_session_without_usable_windows_yields_empty_dataset(tmp_path: Path) -> None:
    directory = write_session(tmp_path, frames=[frame_obj(0)], labels=[])
    x, y = session_dataset(load_session(directory))
    assert x.shape == (0, len(NODE_FEATURES))
    assert y.shape == (0,)
