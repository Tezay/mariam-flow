# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false
# pyright: reportUnknownVariableType=false, reportUnknownArgumentType=false
"""Visual reports: session portraits and evaluation summaries.

The session figure is the first thing to look at after any capture: an
amplitude heatmap per RX node (time × subcarrier — bodies moving in the
zone appear as vertical texture), the label band, and the v1 features
over time. The evaluation figure renders the confusion matrix of a
session-grouped cross-validation next to its accuracy and baseline.

Everything uses matplotlib's object-oriented API (`Figure` built
directly, no `pyplot`): no global state, no display backend — safe for
headless use and libraries. Heatmaps use ``pcolormesh`` with the real
frame timestamps, so capture gaps stay visible instead of being silently
stretched, as ``imshow`` would do.

Command line::

    uv run python -m flow_ml.report --sessions data/sessions --out report/
    uv run python -m flow_ml.report --demo 6 --out /tmp/report   # synthetic
"""

from __future__ import annotations

import argparse
from collections.abc import Sequence
from pathlib import Path

import numpy as np
from matplotlib.figure import Figure

from flow_ml.features import MIN_FRAMES_PER_NODE, NODE_FEATURES, node_features
from flow_ml.session import DensityClass, Session, load_session
from flow_ml.synthetic import synthetic_session
from flow_ml.training import EvaluationReport, evaluate_grouped
from flow_ml.windows import DEFAULT_HOP_US, DEFAULT_WINDOW_US, sliding_windows

CLASS_COLORS: dict[DensityClass, str] = {
    DensityClass.EMPTY: "#1e7e34",
    DensityClass.LOW: "#b8860b",
    DensityClass.MEDIUM: "#cc5500",
    DensityClass.SATURATED: "#b02a37",
}
"""Semantic class colors — the same palette as the labeling page."""


def session_figure(
    session: Session,
    *,
    window_us: int = DEFAULT_WINDOW_US,
    hop_us: int = DEFAULT_HOP_US,
) -> Figure:
    """Builds the portrait of one session.

    One amplitude heatmap per RX node, the ground-truth band, and selected
    v1 features over time (motion energy and inter-subcarrier
    correlation).
    """
    if not session.frames:
        raise ValueError("session has no frames")
    rx_nodes = [n for n in session.rx_node_ids if session.frames_of(n)]
    if not rx_nodes:
        raise ValueError("no RX node has frames")

    t0_us = session.frames[0].ts_us
    n_nodes = len(rx_nodes)
    fig = Figure(figsize=(11.0, 2.8 * n_nodes + 3.6), constrained_layout=True)
    axes = list(
        np.atleast_1d(
            fig.subplots(
                n_nodes + 2,
                1,
                sharex=True,
                gridspec_kw={"height_ratios": [3.0] * n_nodes + [0.5, 2.2]},
            )
        )
    )

    for ax, node_id in zip(axes[:n_nodes], rx_nodes, strict=False):
        frames = session.frames_of(node_id)
        times_s = np.array([f.ts_us - t0_us for f in frames], dtype=np.float64) / 1e6
        amp = np.array([f.amp for f in frames], dtype=np.float64).T  # (subcarrier, time)
        mesh = ax.pcolormesh(times_s, np.arange(amp.shape[0]), amp, shading="nearest")
        fig.colorbar(mesh, ax=ax, label="amplitude")
        ax.set_ylabel(f"{node_id}\nsubcarrier")

    band = axes[n_nodes]
    for start_s, end_s, density in _label_segments(session):
        band.axvspan(start_s, end_s, color=CLASS_COLORS[density])
    band.set_yticks([])
    band.set_ylabel("label", rotation=0, ha="right", va="center")

    features_ax = axes[n_nodes + 1]
    motion_index = NODE_FEATURES.index("motion_energy")
    corr_index = NODE_FEATURES.index("subcarrier_corr")
    windows = sliding_windows(session, window_us=window_us, hop_us=hop_us)
    for node_id in rx_nodes:
        centers: list[float] = []
        motion: list[float] = []
        correlation: list[float] = []
        for window in windows:
            frames = [f for f in window.frames if f.node_id == node_id]
            if len(frames) < MIN_FRAMES_PER_NODE:
                continue
            vector = node_features(frames, window.duration_us)
            center_us = window.start_us + window.duration_us // 2
            centers.append((center_us - t0_us) / 1e6)
            motion.append(float(vector[motion_index]))
            correlation.append(float(vector[corr_index]))
        features_ax.plot(centers, motion, label=f"{node_id} motion")
        features_ax.plot(centers, correlation, linestyle="--", label=f"{node_id} corr")
    features_ax.set_xlabel("time (s)")
    features_ax.set_ylabel("feature value")
    features_ax.legend(loc="upper right", fontsize="small")
    features_ax.grid(True, alpha=0.25)

    fig.suptitle(
        f"{session.meta.session_id} — {session.meta.site} · "
        f"{len(session.frames)} frames · {len(session.labels)} labels"
    )
    return fig


def evaluation_figure(report: EvaluationReport) -> Figure:
    """Renders a session-grouped evaluation: confusion matrix and scores."""
    fig = Figure(figsize=(6.6, 5.8), constrained_layout=True)
    ax = fig.subplots()
    matrix = report.confusion
    image = ax.imshow(matrix, cmap="Blues")
    fig.colorbar(image, ax=ax, label="windows")

    names = [density.name.lower() for density in DensityClass]
    ax.set_xticks(range(4), names)
    ax.set_yticks(range(4), names)
    ax.set_xlabel("predicted")
    ax.set_ylabel("truth")

    threshold = matrix.max() / 2 if matrix.max() > 0 else 0
    for row in range(4):
        for column in range(4):
            count = int(matrix[row, column])
            color = "white" if count > threshold else "black"
            ax.text(column, row, str(count), ha="center", va="center", color=color)

    ax.set_title(
        f"accuracy {report.accuracy:.3f} — baseline {report.baseline_accuracy:.3f}\n"
        f"{report.n_windows} windows across {report.n_sessions} sessions"
    )
    return fig


def _label_segments(session: Session) -> list[tuple[float, float, DensityClass]]:
    """Step-function label segments in seconds relative to the first frame."""
    if not session.labels or not session.frames:
        return []
    t0_us = session.frames[0].ts_us
    end_us = session.frames[-1].ts_us
    segments: list[tuple[float, float, DensityClass]] = []
    for index, label in enumerate(session.labels):
        stop_us = session.labels[index + 1].ts_us if index + 1 < len(session.labels) else end_us
        if stop_us <= label.ts_us:
            continue
        segments.append(((label.ts_us - t0_us) / 1e6, (stop_us - t0_us) / 1e6, label.density))
    return segments


def load_sessions_dir(root: Path) -> list[Session]:
    """Loads every sealed session directory under `root`, sorted by id."""
    sessions: list[Session] = []
    for child in sorted(root.iterdir()):
        sealed = child.is_dir() and not child.name.endswith(".recording")
        if sealed and (child / "meta.json").exists():
            sessions.append(load_session(child))
    return sessions


def demo_sessions(count: int) -> list[Session]:
    """Synthetic sessions for trying the reports without any capture."""
    return [
        synthetic_session(
            f"demo-{seed:02}",
            seed=seed,
            seconds_per_class=20.0,
            frame_rate_hz=10.0,
            subcarriers=16,
        )
        for seed in range(count)
    ]


def main(argv: Sequence[str] | None = None) -> None:
    """Entry point: ``python -m flow_ml.report``."""
    parser = argparse.ArgumentParser(
        prog="python -m flow_ml.report",
        description="Render session portraits and an evaluation report.",
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--sessions", type=Path, help="root directory of recorded sessions")
    source.add_argument("--demo", type=int, metavar="N", help="use N synthetic sessions")
    parser.add_argument("--out", type=Path, required=True, help="output directory")
    parser.add_argument("--window-us", type=int, default=DEFAULT_WINDOW_US)
    parser.add_argument("--hop-us", type=int, default=DEFAULT_HOP_US)
    args = parser.parse_args(argv)

    sessions = demo_sessions(args.demo) if args.demo else load_sessions_dir(args.sessions)
    if not sessions:
        raise SystemExit("no session found")
    args.out.mkdir(parents=True, exist_ok=True)

    for session in sessions:
        figure = session_figure(session, window_us=args.window_us, hop_us=args.hop_us)
        target = args.out / f"{session.meta.session_id}.png"
        figure.savefig(target, dpi=130)
        print(f"wrote {target}")

    labeled = [session for session in sessions if session.labels]
    if len(labeled) < 2:
        print("fewer than 2 labeled sessions: skipping the evaluation report")
        return
    try:
        report = evaluate_grouped(
            labeled,
            n_splits=min(3, len(labeled)),
            window_us=args.window_us,
            hop_us=args.hop_us,
        )
    except ValueError as exc:
        # The session portraits (the main output) are already written; a
        # cross-validation fold can still be untrainable when a session
        # covers a single density class. Report it instead of crashing.
        print(f"skipping the evaluation report: {exc}")
        print("(training needs each session to cover at least two density classes)")
        return
    target = args.out / "evaluation.png"
    evaluation_figure(report).savefig(target, dpi=130)
    print(f"wrote {target}")
    print(report.format())


if __name__ == "__main__":
    main()
