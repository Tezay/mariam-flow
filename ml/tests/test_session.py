"""Loader tests, including the cross-language canonical contract."""

from pathlib import Path

import pytest

from flow_ml import DensityClass, SessionFormatError, load_session
from tests.helpers import frame_obj, label_obj, meta_obj, write_session

# Exactly the canonical lines frozen by the Rust flow-core tests: both
# implementations of the format must accept the same bytes.
CANONICAL_FRAME = (
    '{"ts_us":1720000000000000,"node_id":"rx-1","rssi":-52,"mcs":7,'
    '"len":3,"amp":[1.0,2.5,0.25],"phase":[0.0,-1.5,3.1]}'
)
CANONICAL_LABEL_MANUAL = '{"ts_us":1720000000000001,"class":2}'
CANONICAL_LABEL_SENSOR = '{"ts_us":1720000000000002,"class":3,"count":47}'


def test_canonical_lines_from_flow_core_load(tmp_path: Path) -> None:
    directory = write_session(
        tmp_path,
        frames=[],
        csi_lines=[CANONICAL_FRAME],
        labels=[],
    )
    (directory / "labels.ndjson").write_text(
        CANONICAL_LABEL_MANUAL + "\n" + CANONICAL_LABEL_SENSOR + "\n", encoding="utf-8"
    )
    session = load_session(directory)

    frame = session.frames[0]
    assert frame.ts_us == 1_720_000_000_000_000
    assert frame.node_id == "rx-1"
    assert frame.rssi == -52
    assert frame.mcs == 7
    assert frame.amp == (1.0, 2.5, 0.25)
    assert frame.phase == (0.0, -1.5, 3.1)

    manual, sensor = session.labels
    assert manual.density is DensityClass.MEDIUM
    assert manual.count is None
    assert sensor.density is DensityClass.SATURATED
    assert sensor.count == 47


def test_full_session_loads(tmp_path: Path) -> None:
    directory = write_session(
        tmp_path,
        frames=[frame_obj(10), frame_obj(20), frame_obj(20)],
        labels=[label_obj(5, 1)],
    )
    session = load_session(directory)
    assert session.meta.session_id == "s-test-001"
    assert session.meta.wifi_channel == 6
    assert session.meta.class_mapping[DensityClass.EMPTY] == "0 people"
    assert session.rx_node_ids == ("rx-1",)
    assert len(session.frames) == 3
    assert len(session.frames_of("rx-1")) == 3
    assert session.labels[0].density is DensityClass.LOW


@pytest.mark.parametrize(
    ("frame", "message"),
    [
        (frame_obj(1, amp=(1.0, 2.0), phase=(0.0,)), "length mismatch"),
        (frame_obj(1) | {"len": 9}, "declared len"),
        (frame_obj(1, amp=(), phase=()), "no subcarrier data"),
        (frame_obj(1) | {"rssi": True}, "must be an integer"),
        (frame_obj(1, node_id="ghost"), "undeclared node"),
    ],
)
def test_invalid_frames_are_rejected(
    tmp_path: Path, frame: dict[str, object], message: str
) -> None:
    directory = write_session(tmp_path, frames=[frame], labels=[])
    with pytest.raises(SessionFormatError, match=message):
        load_session(directory)


def test_out_of_order_frames_are_rejected(tmp_path: Path) -> None:
    directory = write_session(tmp_path, frames=[frame_obj(20), frame_obj(10)], labels=[])
    with pytest.raises(SessionFormatError, match="out-of-order"):
        load_session(directory)


def test_invalid_density_class_is_rejected(tmp_path: Path) -> None:
    directory = write_session(tmp_path, frames=[frame_obj(1)], labels=[label_obj(1, 9)])
    with pytest.raises(SessionFormatError, match="invalid density class 9"):
        load_session(directory)


def test_invalid_node_role_is_rejected(tmp_path: Path) -> None:
    meta = meta_obj()
    meta["nodes"] = [{"node_id": "x", "role": "relay", "position": "?"}]
    directory = write_session(tmp_path, frames=[], labels=[], meta=meta)
    with pytest.raises(SessionFormatError, match="invalid node role"):
        load_session(directory)


def test_rx_node_ids_are_sorted(tmp_path: Path) -> None:
    directory = write_session(
        tmp_path,
        frames=[],
        labels=[],
        meta=meta_obj(rx_nodes=("rx-2", "rx-1")),
    )
    assert load_session(directory).rx_node_ids == ("rx-1", "rx-2")
