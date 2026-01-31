from __future__ import annotations

from typing import Any, Dict, List, Optional

from models.utils import apply_bounds, compute_occupancy, get_float


def predict(
    *,
    obstructions: List[Dict[str, Any]],
    params: Dict[str, Any],
    timestamp: str,
) -> Dict[str, Optional[object]]:
    """
    Linear V1 model.

    Params:
      - slope (float, default 0.2)
      - intercept (float, default 0.0)
      - min_wait_minutes (optional int)
      - max_wait_minutes (optional int)
    """
    occupancy_percent, valid_count, error_count = compute_occupancy(obstructions)

    if valid_count == 0 or occupancy_percent is None:
        return {
            "wait_time_minutes": None,
            "status": "degraded",
            "error_code": "NO_DATA",
            "timestamp": timestamp,
        }

    slope = get_float(params, "slope", 0.2)
    intercept = get_float(params, "intercept", 0.0)

    wait_time = intercept + slope * occupancy_percent
    wait_time = apply_bounds(wait_time, params)

    status = "degraded" if error_count > 0 else "ok"
    return {
        "wait_time_minutes": wait_time,
        "status": status,
        "error_code": None,
        "timestamp": timestamp,
    }
