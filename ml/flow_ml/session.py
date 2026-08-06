"""Loading and validation of canonical capture sessions.

This module is the Python mirror of the Rust ``flow-core`` data model. The
on-disk session format (``meta.json``, ``csi.ndjson``, ``labels.ndjson``)
is the contract between the edge (Rust, writes) and the ML pipeline
(Python, reads); field names and encodings are frozen, and both sides pin
them with tests on the same canonical examples.

Everything read from disk crosses a trust boundary, so the loader applies
the same validations as the Rust side: declared subcarrier counts,
timestamp ordering, class range, and node declarations.
"""

from __future__ import annotations

import json
import tarfile
import tempfile
from dataclasses import dataclass
from enum import IntEnum
from pathlib import Path
from typing import cast

META_FILE = "meta.json"
CSI_FILE = "csi.ndjson"
LABELS_FILE = "labels.ndjson"

type JsonObj = dict[str, object]


class SessionFormatError(ValueError):
    """A session file violates the canonical format."""


class DensityClass(IntEnum):
    """Frozen 4-class output space of the density classifier.

    The integer encoding (0..3) is part of the on-disk format and must
    never change. The order follows increasing density, which downstream
    smoothing relies on.
    """

    EMPTY = 0
    LOW = 1
    MEDIUM = 2
    SATURATED = 3


@dataclass(frozen=True, slots=True)
class Frame:
    """One CSI measurement — one line of ``csi.ndjson``."""

    ts_us: int
    node_id: str
    rssi: int
    mcs: int
    amp: tuple[float, ...]
    phase: tuple[float, ...]


@dataclass(frozen=True, slots=True)
class Label:
    """One ground-truth annotation — one line of ``labels.ndjson``.

    ``density`` maps to the JSON key ``class`` (a Python keyword).
    ``count`` is the exact people count when a reference sensor measured
    it, ``None`` for manually produced labels.
    """

    ts_us: int
    density: DensityClass
    count: int | None = None


@dataclass(frozen=True, slots=True)
class NodePlacement:
    """Physical placement of one sensing node."""

    node_id: str
    role: str
    position: str


@dataclass(frozen=True, slots=True)
class SessionMeta:
    """Session metadata — ``meta.json``."""

    session_id: str
    site: str
    environment: str
    wifi_channel: int
    nodes: tuple[NodePlacement, ...]
    firmware_version: str
    software_version: str
    class_mapping: dict[DensityClass, str]


@dataclass(frozen=True, slots=True)
class Session:
    """One loaded capture session (immutable, ordered)."""

    meta: SessionMeta
    frames: tuple[Frame, ...]
    labels: tuple[Label, ...]

    @property
    def rx_node_ids(self) -> tuple[str, ...]:
        """Ids of the receiving nodes, in stable (sorted) order."""
        return tuple(sorted(n.node_id for n in self.meta.nodes if n.role == "rx"))

    def frames_of(self, node_id: str) -> tuple[Frame, ...]:
        """Frames received by one node, in timestamp order."""
        return tuple(f for f in self.frames if f.node_id == node_id)


def load_session(path: Path) -> Session:
    """Loads and validates one session directory.

    Raises :class:`SessionFormatError` on any deviation from the canonical
    format — a session that loads is safe to consume downstream.
    """
    meta = _parse_meta(_read_json(path / META_FILE))
    node_ids = {n.node_id for n in meta.nodes}

    frames: list[Frame] = []
    for where, obj in _ndjson_objects(path / CSI_FILE):
        frame = _parse_frame(obj, where)
        if frame.node_id not in node_ids:
            raise SessionFormatError(f"{where}: undeclared node {frame.node_id!r}")
        if frames and frame.ts_us < frames[-1].ts_us:
            raise SessionFormatError(f"{where}: out-of-order timestamp {frame.ts_us}")
        frames.append(frame)

    labels: list[Label] = []
    for where, obj in _ndjson_objects(path / LABELS_FILE):
        label = _parse_label(obj, where)
        if labels and label.ts_us < labels[-1].ts_us:
            raise SessionFormatError(f"{where}: out-of-order timestamp {label.ts_us}")
        labels.append(label)

    return Session(meta=meta, frames=tuple(frames), labels=tuple(labels))


def load_sessions(root: Path) -> list[Session]:
    """Loads every sealed session under `root`, sorted by name.

    Accepts both shapes a capture arrives in: a session directory, and the
    gzipped tar the appliance exports, whose members sit under the session
    identifier.
    """
    sessions: list[Session] = []
    for child in sorted(root.iterdir()):
        if child.is_dir() and not child.name.endswith(".recording"):
            if (child / META_FILE).exists():
                sessions.append(load_session(child))
        elif child.name.endswith(".tar.gz"):
            sessions.append(_load_archive(child))
    return sessions


def _load_archive(path: Path) -> Session:
    with tempfile.TemporaryDirectory() as staging:
        with tarfile.open(path, "r:gz") as archive:
            archive.extractall(staging, filter="data")
        found = [d for d in Path(staging).iterdir() if (d / META_FILE).exists()]
        if len(found) != 1:
            raise SessionFormatError(
                f"{path}: expected one session directory in the archive, found {len(found)}"
            )
        return load_session(found[0])


def _read_json(path: Path) -> JsonObj:
    try:
        parsed: object = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SessionFormatError(f"{path.name}: invalid JSON: {exc}") from exc
    if not isinstance(parsed, dict):
        raise SessionFormatError(f"{path.name}: expected a JSON object")
    return cast(JsonObj, parsed)


def _ndjson_objects(path: Path) -> list[tuple[str, JsonObj]]:
    out: list[tuple[str, JsonObj]] = []
    for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        where = f"{path.name}:{lineno}"
        try:
            parsed: object = json.loads(line)
        except json.JSONDecodeError as exc:
            raise SessionFormatError(f"{where}: invalid JSON: {exc}") from exc
        if not isinstance(parsed, dict):
            raise SessionFormatError(f"{where}: expected a JSON object")
        out.append((where, cast(JsonObj, parsed)))
    return out


def _int(obj: JsonObj, key: str, where: str) -> int:
    value = obj.get(key)
    # In Python, bool is a subclass of int: reject it explicitly so that
    # `"rssi": true` does not silently pass as 1.
    if isinstance(value, bool) or not isinstance(value, int):
        raise SessionFormatError(f"{where}: field {key!r} must be an integer")
    return value


def _str(obj: JsonObj, key: str, where: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str):
        raise SessionFormatError(f"{where}: field {key!r} must be a string")
    return value


def _float_seq(obj: JsonObj, key: str, where: str) -> tuple[float, ...]:
    value = obj.get(key)
    if not isinstance(value, list):
        raise SessionFormatError(f"{where}: field {key!r} must be an array")
    out: list[float] = []
    for item in cast(list[object], value):
        if isinstance(item, bool) or not isinstance(item, int | float):
            raise SessionFormatError(f"{where}: field {key!r} must contain numbers")
        out.append(float(item))
    return tuple(out)


def _parse_frame(obj: JsonObj, where: str) -> Frame:
    amp = _float_seq(obj, "amp", where)
    phase = _float_seq(obj, "phase", where)
    declared = _int(obj, "len", where)
    if len(amp) != len(phase):
        raise SessionFormatError(f"{where}: amp/phase length mismatch ({len(amp)}/{len(phase)})")
    if declared != len(amp):
        raise SessionFormatError(f"{where}: declared len {declared} does not match {len(amp)}")
    if not amp:
        raise SessionFormatError(f"{where}: frame carries no subcarrier data")
    return Frame(
        ts_us=_int(obj, "ts_us", where),
        node_id=_str(obj, "node_id", where),
        rssi=_int(obj, "rssi", where),
        mcs=_int(obj, "mcs", where),
        amp=amp,
        phase=phase,
    )


def _parse_label(obj: JsonObj, where: str) -> Label:
    raw_class = _int(obj, "class", where)
    try:
        density = DensityClass(raw_class)
    except ValueError as exc:
        raise SessionFormatError(f"{where}: invalid density class {raw_class}") from exc
    count = _int(obj, "count", where) if "count" in obj else None
    return Label(ts_us=_int(obj, "ts_us", where), density=density, count=count)


def _parse_meta(obj: JsonObj) -> SessionMeta:
    where = META_FILE

    raw_nodes = obj.get("nodes")
    if not isinstance(raw_nodes, list):
        raise SessionFormatError(f"{where}: field 'nodes' must be an array")
    nodes: list[NodePlacement] = []
    for raw_node in cast(list[object], raw_nodes):
        if not isinstance(raw_node, dict):
            raise SessionFormatError(f"{where}: each node must be an object")
        raw = cast(JsonObj, raw_node)
        role = _str(raw, "role", where)
        if role not in ("tx", "rx"):
            raise SessionFormatError(f"{where}: invalid node role {role!r}")
        nodes.append(
            NodePlacement(
                node_id=_str(raw, "node_id", where),
                role=role,
                position=_str(raw, "position", where),
            )
        )

    raw_mapping = obj.get("class_mapping")
    if not isinstance(raw_mapping, dict):
        raise SessionFormatError(f"{where}: field 'class_mapping' must be an object")
    mapping_obj = cast(JsonObj, raw_mapping)
    mapping: dict[DensityClass, str] = {}
    for density in DensityClass:
        mapping[density] = _str(mapping_obj, density.name.lower(), where)

    return SessionMeta(
        session_id=_str(obj, "session_id", where),
        site=_str(obj, "site", where),
        environment=_str(obj, "environment", where),
        wifi_channel=_int(obj, "wifi_channel", where),
        nodes=tuple(nodes),
        firmware_version=_str(obj, "firmware_version", where),
        software_version=_str(obj, "software_version", where),
        class_mapping=mapping,
    )
