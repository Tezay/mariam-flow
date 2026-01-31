from __future__ import annotations

from typing import Any, Dict, List, Optional

from models.utils import compute_occupancy, get_float


def predict(
    *,
    obstructions: List[Dict[str, Any]],
    params: Dict[str, Any],
    timestamp: str,
) -> Dict[str, Optional[object]]:
    """
    Linear V2 model.

    Params:
      - wait_time_at_empty (float, default 0.0)
      - wait_time_at_full (float, default 20.0)
    """
    occupancy_percent, valid_count, error_count = compute_occupancy(obstructions)

    if valid_count == 0 or occupancy_percent is None:
        return {
            "wait_time_minutes": None,
            "status": "degraded",
            "error_code": "NO_DATA",
            "timestamp": timestamp,
        }

    wait_time_at_empty = get_float(params, "wait_time_at_empty", 0.0)
    wait_time_at_full = get_float(params, "wait_time_at_full", 20.0)

    wait_time = wait_time_at_empty + (occupancy_percent / 100.0) * (
        wait_time_at_full - wait_time_at_empty
    )

    status = "degraded" if error_count > 0 else "ok"
    return {
        "wait_time_minutes": wait_time,
        "status": status,
        "error_code": None,
        "timestamp": timestamp,
    }
