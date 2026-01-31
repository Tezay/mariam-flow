from __future__ import annotations

import argparse
import os
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

from fastapi import FastAPI
from pydantic import BaseModel, Field
import uvicorn

from models import (
    linear_v1,
    linear_v2, 
    obstruction_count_v1,
)

API_VERSION = "1.0"

app = FastAPI(title="Mariam Flow Model Service", version=API_VERSION)


class Obstruction(BaseModel):
    sensor_id: int
    obstructed: Optional[bool] = None
    timestamp: Optional[str] = None


class PredictRequest(BaseModel):
    api_version: str = Field(default=API_VERSION)
    model_id: str
    params: Dict[str, Any] = Field(default_factory=dict)
    obstructions: List[Obstruction]
    timestamp: Optional[str] = None


class PredictResponse(BaseModel):
    wait_time_minutes: Optional[float]
    status: str
    error_code: Optional[str] = None
    timestamp: str


class HealthResponse(BaseModel):
    status: str
    timestamp: str


def now_rfc3339() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def predict_unknown(request: PredictRequest) -> PredictResponse:
    timestamp = request.timestamp or now_rfc3339()
    return PredictResponse(
        wait_time_minutes=None,
        status="degraded",
        error_code="NO_DATA",
        timestamp=timestamp,
    )


MODEL_HANDLERS = {
    "linear_v1": lambda request: _wrap_external_model(linear_v1, request),
    "linear_v2": lambda request: _wrap_external_model(linear_v2, request),
    "obstruction_count_v1": lambda request: _wrap_external_model(obstruction_count_v1, request),
}


def _wrap_external_model(handler, request: PredictRequest) -> PredictResponse:
    timestamp = request.timestamp or now_rfc3339()
    payload = handler(
        obstructions=[obs.model_dump() for obs in request.obstructions],
        params=request.params,
        timestamp=timestamp,
    )
    return PredictResponse(**payload)


@app.post("/predict", response_model=PredictResponse)
async def predict(request: PredictRequest) -> PredictResponse:
    handler = MODEL_HANDLERS.get(request.model_id, predict_unknown)
    return handler(request)


@app.get("/health", response_model=HealthResponse)
async def health() -> HealthResponse:
    return HealthResponse(status="ok", timestamp=now_rfc3339())


def main() -> None:
    parser = argparse.ArgumentParser(description="Mariam Flow Model Service")
    parser.add_argument(
        "--host",
        default=os.getenv("MARIAM_MODEL_HOST", "127.0.0.1"),
        help="Host to bind the model service",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=int(os.getenv("MARIAM_MODEL_PORT", "5001")),
        help="Port to bind the model service",
    )
    parser.add_argument(
        "--log-level",
        default=os.getenv("MARIAM_MODEL_LOG_LEVEL", "info"),
        help="Uvicorn log level",
    )

    args = parser.parse_args()

    uvicorn.run(
        app,
        host=args.host,
        port=args.port,
        log_level=args.log_level,
        access_log=False,
    )


if __name__ == "__main__":
    main()
