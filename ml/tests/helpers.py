"""Builders writing synthetic on-disk sessions for tests.

Sessions are written as raw JSON, independently of any flow_ml code, so a
bug in the loader cannot hide behind the same bug in the test fixtures.
"""

from __future__ import annotations

import json
from pathlib import Path

type JsonObj = dict[str, object]


def frame_obj(
    ts_us: int,
    *,
    node_id: str = "rx-1",
    rssi: int = -52,
    mcs: int = 7,
    amp: tuple[float, ...] = (1.0, 2.0),
    phase: tuple[float, ...] | None = None,
) -> JsonObj:
    if phase is None:
        phase = tuple(0.0 for _ in amp)
    return {
        "ts_us": ts_us,
        "node_id": node_id,
        "rssi": rssi,
        "mcs": mcs,
        "len": len(amp),
        "amp": list(amp),
        "phase": list(phase),
    }


def label_obj(ts_us: int, density: int, count: int | None = None) -> JsonObj:
    obj: JsonObj = {"ts_us": ts_us, "class": density}
    if count is not None:
        obj["count"] = count
    return obj


def meta_obj(*, session_id: str = "s-test-001", rx_nodes: tuple[str, ...] = ("rx-1",)) -> JsonObj:
    nodes: list[JsonObj] = [{"node_id": "tx-1", "role": "tx", "position": "shelf"}]
    nodes.extend({"node_id": rx, "role": "rx", "position": "wall"} for rx in rx_nodes)
    return {
        "session_id": session_id,
        "site": "lab-a",
        "environment": "test fixture",
        "wifi_channel": 6,
        "nodes": nodes,
        "firmware_version": "0.1.0",
        "software_version": "0.1.0",
        "class_mapping": {
            "empty": "0 people",
            "low": "1 person",
            "medium": "2 people",
            "saturated": "3+ people",
        },
    }


def write_session(
    root: Path,
    *,
    frames: list[JsonObj],
    labels: list[JsonObj],
    meta: JsonObj | None = None,
    csi_lines: list[str] | None = None,
) -> Path:
    """Writes a session directory; ``csi_lines`` overrides ``frames`` with
    raw text lines (for canonical-string tests)."""
    if meta is None:
        meta = meta_obj()
    session_id = meta["session_id"]
    assert isinstance(session_id, str)
    directory = root / session_id
    directory.mkdir(parents=True)
    (directory / "meta.json").write_text(json.dumps(meta), encoding="utf-8")
    if csi_lines is None:
        csi_lines = [json.dumps(obj) for obj in frames]
    (directory / "csi.ndjson").write_text("\n".join(csi_lines) + "\n", encoding="utf-8")
    label_lines = [json.dumps(obj) for obj in labels]
    (directory / "labels.ndjson").write_text(
        "\n".join(label_lines) + "\n" if label_lines else "", encoding="utf-8"
    )
    return directory
