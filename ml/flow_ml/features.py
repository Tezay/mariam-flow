"""Hand-crafted v1 features: per-window, per-RX-node statistics.

Each feature summarizes one physical intuition about how bodies in the
zone disturb the Wi-Fi channel:

- ``amp_mean`` — overall attenuation level (static bodies absorb energy);
- ``amp_std`` — temporal variability of each subcarrier's amplitude,
  averaged across subcarriers (moving bodies make the channel fluctuate);
- ``motion_energy`` — mean absolute frame-to-frame amplitude change
  (short-term agitation, the strongest motion signal in the literature);
- ``subcarrier_corr`` — mean pairwise correlation between subcarriers over
  the window (an empty channel evolves coherently, bodies decorrelate the
  subcarriers by disturbing distinct multipath components);
- ``rssi_mean`` / ``rssi_std`` — coarse received-power summary;
- ``frame_rate`` — observed frames per second (occupancy can also degrade
  reception itself).

Phase is deliberately not used in v1: on unsynchronized commodity radios
the raw phase carries large offsets and drift, and is unusable without a
dedicated sanitization step (a documented v2 candidate).

A window yields one vector per RX node; multi-node sessions concatenate
the per-node vectors in stable node order.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import cast

import numpy as np
import numpy.typing as npt

from flow_ml.session import Frame, Session
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US, Window, sliding_windows

NODE_FEATURES: tuple[str, ...] = (
    "amp_mean",
    "amp_std",
    "motion_energy",
    "subcarrier_corr",
    "rssi_mean",
    "rssi_std",
    "frame_rate",
)

MIN_FRAMES_PER_NODE = 2
"""Below this, temporal statistics (std, motion) are meaningless."""


def feature_names(rx_nodes: Sequence[str]) -> tuple[str, ...]:
    """Column names of the feature matrix, ``<node>:<feature>``."""
    return tuple(f"{node}:{name}" for node in rx_nodes for name in NODE_FEATURES)


def node_features(frames: Sequence[Frame], duration_us: int) -> npt.NDArray[np.float64]:
    """Feature vector for one node's frames within one window.

    Requires at least :data:`MIN_FRAMES_PER_NODE` frames sharing one
    subcarrier count.
    """
    if len(frames) < MIN_FRAMES_PER_NODE:
        raise ValueError(f"need at least {MIN_FRAMES_PER_NODE} frames, got {len(frames)}")
    widths = {len(frame.amp) for frame in frames}
    if len(widths) != 1:
        raise ValueError(f"inconsistent subcarrier counts in window: {sorted(widths)}")

    amp = np.array([frame.amp for frame in frames], dtype=np.float64)  # (time, subcarrier)
    rssi = np.array([frame.rssi for frame in frames], dtype=np.float64)
    seconds = duration_us / 1e6

    return np.array(
        [
            float(amp.mean()),
            float(amp.std(axis=0).mean()),
            float(np.abs(np.diff(amp, axis=0)).mean()),
            _mean_pairwise_correlation(amp),
            float(rssi.mean()),
            float(rssi.std()),
            len(frames) / seconds,
        ],
        dtype=np.float64,
    )


def window_vector(window: Window, rx_nodes: Sequence[str]) -> npt.NDArray[np.float64] | None:
    """Concatenated per-node features for one window.

    Returns ``None`` when any RX node lacks enough frames in the window —
    an incomplete observation, dropped rather than half-filled.
    """
    parts: list[npt.NDArray[np.float64]] = []
    for node_id in rx_nodes:
        frames = [frame for frame in window.frames if frame.node_id == node_id]
        if len(frames) < MIN_FRAMES_PER_NODE:
            return None
        parts.append(node_features(frames, window.duration_us))
    return np.concatenate(parts)


def session_dataset(
    session: Session,
    *,
    window_us: int = DEFAULT_WINDOW_US,
    hop_us: int = DEFAULT_HOP_US,
) -> tuple[npt.NDArray[np.float64], npt.NDArray[np.int64]]:
    """Builds the supervised dataset ``(X, y)`` of one session.

    ``X`` has one row per usable window (labeled, and complete for every
    RX node) and one column per feature; ``y`` holds the integer density
    classes. Unlabeled or incomplete windows are dropped.
    """
    rx_nodes = session.rx_node_ids
    rows: list[npt.NDArray[np.float64]] = []
    classes: list[int] = []
    for window in sliding_windows(session, window_us=window_us, hop_us=hop_us):
        if window.label is None:
            continue
        vector = window_vector(window, rx_nodes)
        if vector is None:
            continue
        rows.append(vector)
        classes.append(int(window.label))
    width = len(feature_names(rx_nodes))
    x = np.vstack(rows) if rows else np.empty((0, width), dtype=np.float64)
    y = np.array(classes, dtype=np.int64)
    return x, y


def _mean_pairwise_correlation(amp: npt.NDArray[np.float64]) -> float:
    """Mean correlation between subcarrier time series.

    Constant subcarriers have undefined correlation (zero variance); their
    NaN entries are treated as zero — no variation carries no correlation
    information.
    """
    count = amp.shape[1]
    if count < 2:
        return 0.0
    with np.errstate(invalid="ignore", divide="ignore"):
        raw = np.corrcoef(amp.T)
    corr = cast(npt.NDArray[np.float64], np.nan_to_num(raw))
    # Mean of the off-diagonal entries; by symmetry this equals the mean of
    # the strict upper triangle.
    total = float(corr.sum()) - float(np.trace(corr))
    return total / (count * count - count)
