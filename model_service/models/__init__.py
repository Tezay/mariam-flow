"""Model handlers for the Mariam Flow Python model service."""

from models.linear_v1 import predict as linear_v1
from models.linear_v2 import predict as linear_v2
from models.obstruction_count_v1 import predict as obstruction_count_v1

__all__ = [
    "linear_v1",
    "linear_v2",
    "obstruction_count_v1",
]
