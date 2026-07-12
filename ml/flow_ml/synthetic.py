"""Synthetic capture sessions with separable density classes.

These sessions exist to validate the *plumbing* of the pipeline — loading,
windowing, features, training, evaluation — end to end, deterministically,
without hardware. They do not imitate real CSI, and accuracy measured on
them says nothing about field accuracy.

The signal model encodes the same physical intuitions the v1 features
target, so that the classes are cleanly separable:

- mean amplitude *decreases* with density (bodies absorb energy);
- temporal fluctuation *increases* with density (bodies move);
- fluctuation shifts from shared across subcarriers (a coherent, empty
  channel) to independent per subcarrier (bodies disturb distinct
  multipath components) as density grows;
- RSSI drifts down with density.

Each seed also draws a fixed per-subcarrier offset — a crude "room
signature", so two seeds behave like two different sites.
"""

from __future__ import annotations

from typing import cast

import numpy as np

from flow_ml.session import (
    DensityClass,
    Frame,
    Label,
    NodePlacement,
    Session,
    SessionMeta,
)


def synthetic_session(
    session_id: str,
    *,
    seed: int,
    seconds_per_class: float = 30.0,
    frame_rate_hz: float = 20.0,
    subcarriers: int = 16,
    node_id: str = "rx-1",
) -> Session:
    """Generates one deterministic session covering the four classes.

    The session runs through ``empty → low → medium → saturated``, each
    segment lasting ``seconds_per_class``, with one label at each segment
    start. The same ``seed`` always yields the same session.
    """
    rng = np.random.default_rng(seed)
    frame_interval_us = round(1e6 / frame_rate_hz)
    frames_per_class = round(seconds_per_class * frame_rate_hz)
    room = rng.normal(0.0, 0.5, subcarriers)

    frames: list[Frame] = []
    labels: list[Label] = []
    ts_us = 0
    phase = tuple(0.0 for _ in range(subcarriers))

    for density in DensityClass:
        labels.append(Label(ts_us=ts_us, density=density))

        base = 10.0 - 1.5 * density
        motion = 0.05 + 0.5 * density
        independent_weight = 0.15 + 0.25 * density

        shared = rng.normal(0.0, 1.0, frames_per_class)
        independent = rng.normal(0.0, 1.0, (frames_per_class, subcarriers))
        fluctuation = (1.0 - independent_weight) * shared[:, None]
        fluctuation = fluctuation + independent_weight * independent
        amp = base + room[None, :] + motion * fluctuation

        for i in range(frames_per_class):
            row = cast(list[float], amp[i].tolist())
            rssi = -50 - 2 * int(density) + int(rng.integers(-1, 2))
            frames.append(
                Frame(
                    ts_us=ts_us,
                    node_id=node_id,
                    rssi=rssi,
                    mcs=7,
                    amp=tuple(row),
                    phase=phase,
                )
            )
            ts_us += frame_interval_us

    meta = SessionMeta(
        session_id=session_id,
        site=f"synthetic-{seed}",
        environment="synthetic pipeline-validation data",
        wifi_channel=6,
        nodes=(
            NodePlacement(node_id="tx-1", role="tx", position="synthetic"),
            NodePlacement(node_id=node_id, role="rx", position="synthetic"),
        ),
        firmware_version="synthetic",
        software_version="synthetic",
        class_mapping={
            DensityClass.EMPTY: "synthetic empty",
            DensityClass.LOW: "synthetic low",
            DensityClass.MEDIUM: "synthetic medium",
            DensityClass.SATURATED: "synthetic saturated",
        },
    )
    return Session(meta=meta, frames=tuple(frames), labels=tuple(labels))
