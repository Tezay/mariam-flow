"""Time-based sliding windows over a session's frames.

Features are computed over fixed *time* windows (default 5 s, hop 1 s),
never over fixed frame counts: the frame rate varies with radio conditions
and losses, and a time window is what the physics of the problem defines
(how much motion happened during these seconds).

Ground truth is treated as a step function: a label declares the state of
the zone from its timestamp until the next label. A window takes the state
at its center; windows starting before the first label have no ground
truth (``label is None``) and are excluded from training datasets.
"""

from __future__ import annotations

from bisect import bisect_left, bisect_right
from collections.abc import Sequence
from dataclasses import dataclass

from flow_ml.session import DensityClass, Frame, Label, Session

DEFAULT_WINDOW_US = 5_000_000
DEFAULT_HOP_US = 1_000_000


@dataclass(frozen=True, slots=True)
class Window:
    """One time slice of a session, with its ground truth if known."""

    start_us: int
    end_us: int
    """End of the window (exclusive)."""
    frames: tuple[Frame, ...]
    label: DensityClass | None

    @property
    def duration_us(self) -> int:
        """Window length in microseconds."""
        return self.end_us - self.start_us


def label_at(labels: Sequence[Label], ts_us: int) -> DensityClass | None:
    """State of the zone at ``ts_us``: the latest label at or before it.

    ``labels`` must be ordered by timestamp (guaranteed by the loader).
    Returns ``None`` before the first label.
    """
    times = [label.ts_us for label in labels]
    idx = bisect_right(times, ts_us)
    if idx == 0:
        return None
    return labels[idx - 1].density


def sliding_windows(
    session: Session,
    *,
    window_us: int = DEFAULT_WINDOW_US,
    hop_us: int = DEFAULT_HOP_US,
) -> tuple[Window, ...]:
    """Cuts the session into fixed-duration windows.

    Windows are anchored at the first frame's timestamp and advance by
    ``hop_us`` (overlapping when ``hop_us < window_us``); only windows
    fully contained in the observed time span are produced, and windows
    containing no frame (capture gaps) are skipped.
    """
    if window_us <= 0 or hop_us <= 0:
        raise ValueError("window_us and hop_us must be positive")
    if not session.frames:
        return ()

    frame_times = [frame.ts_us for frame in session.frames]
    label_times = [label.ts_us for label in session.labels]
    span_end = frame_times[-1] + 1  # exclusive end covering the last frame

    windows: list[Window] = []
    start = frame_times[0]
    while start + window_us <= span_end:
        end = start + window_us
        lo = bisect_left(frame_times, start)
        hi = bisect_left(frame_times, end)
        if hi > lo:
            center = start + window_us // 2
            idx = bisect_right(label_times, center)
            label = session.labels[idx - 1].density if idx > 0 else None
            windows.append(
                Window(
                    start_us=start,
                    end_us=end,
                    frames=session.frames[lo:hi],
                    label=label,
                )
            )
        start += hop_us
    return tuple(windows)
