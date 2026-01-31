from __future__ import annotations

from typing import Any, Dict, List, Optional

from models.utils import apply_bounds, get_float


def predict(
    *,
    obstructions: List[Dict[str, Any]],
    params: Dict[str, Any],
    timestamp: str,
) -> Dict[str, Optional[object]]:
    """
    Obstruction-count model.

    Params:
      - base_minutes (float, default 0.0)
      - per_obstruction_minutes (float, default 2.0)
      - min_wait_minutes (optional int)
      - max_wait_minutes (optional int)
    """
    valid_count = 0
    obstructed_count = 0
    error_count = 0

    for obstruction in obstructions:
        value = obstruction.get("obstructed")
        if value is True:
            valid_count += 1
            obstructed_count += 1
        elif value is False:
            valid_count += 1
        else:
            error_count += 1

    if valid_count == 0:
        return {
            "wait_time_minutes": None,
            "status": "degraded",
            "error_code": "NO_DATA",
            "timestamp": timestamp,
        }

    base_minutes = get_float(params, "base_minutes", 0.0)
    per_obstruction_minutes = get_float(params, "per_obstruction_minutes", 2.0)

    wait_time = base_minutes + (obstructed_count * per_obstruction_minutes)
    wait_time = apply_bounds(wait_time, params)

    status = "degraded" if error_count > 0 else "ok"
    return {
        "wait_time_minutes": wait_time,
        "status": status,
        "error_code": None,
        "timestamp": timestamp,
    }
