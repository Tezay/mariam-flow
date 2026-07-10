"""Package sanity checks."""

import flow_ml


def test_version_is_exposed() -> None:
    assert flow_ml.__version__ == "0.1.0"
