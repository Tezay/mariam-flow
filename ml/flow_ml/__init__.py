"""Feature engineering, model training, and ONNX export for Mariam Flow.

Implemented: loading of canonical capture sessions (:mod:`flow_ml.session`),
time-based sliding windows (:mod:`flow_ml.windows`), and hand-crafted v1
feature extraction into supervised ``(X, y)`` datasets
(:mod:`flow_ml.features`).

Planned: classifier training and evaluation (grouped by session), and ONNX
export with the Python/Rust parity test.
"""

from flow_ml.features import (
    NODE_FEATURES,
    feature_names,
    node_features,
    session_dataset,
    window_vector,
)
from flow_ml.session import (
    DensityClass,
    Frame,
    Label,
    NodePlacement,
    Session,
    SessionFormatError,
    SessionMeta,
    load_session,
)
from flow_ml.windows import Window, label_at, sliding_windows

__all__ = [
    "NODE_FEATURES",
    "DensityClass",
    "Frame",
    "Label",
    "NodePlacement",
    "Session",
    "SessionFormatError",
    "SessionMeta",
    "Window",
    "feature_names",
    "label_at",
    "load_session",
    "node_features",
    "session_dataset",
    "sliding_windows",
    "window_vector",
]

__version__ = "0.1.0"
