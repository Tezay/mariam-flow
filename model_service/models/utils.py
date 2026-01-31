from __future__ import annotations

from typing import Any, Dict, List, Optional, Tuple


def compute_occupancy(
    obstructions: List[Dict[str, Any]],
) -> Tuple[Optional[float], int, int]:
    valid_count = 0
    occupied_count = 0
    error_count = 0

    for obstruction in obstructions:
        value = obstruction.get("obstructed")
        if value is True:
            valid_count += 1
            occupied_count += 1
        elif value is False:
            valid_count += 1
        else:
            error_count += 1

    if valid_count == 0:
        return None, valid_count, error_count

    occupancy_percent = max(0.0, min(100.0, (occupied_count / valid_count) * 100.0))
    return occupancy_percent, valid_count, error_count


def get_float(params: Dict[str, Any], key: str, default: float) -> float:
    value = params.get(key)
    if value is None:
        return default
    try:
        return float(value)
    except (TypeError, ValueError):
        return default


def get_optional_int(params: Dict[str, Any], key: str) -> Optional[int]:
    value = params.get(key)
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def apply_bounds(wait_time: float, params: Dict[str, Any]) -> float:
    min_wait = get_optional_int(params, "min_wait_minutes")
    max_wait = get_optional_int(params, "max_wait_minutes")

    if min_wait is not None:
        wait_time = max(wait_time, float(min_wait))
    if max_wait is not None:
        wait_time = min(wait_time, float(max_wait))

    return wait_time
