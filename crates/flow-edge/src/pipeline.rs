//! Running the live inference chain inside the daemon.
//!
//! The appliance already carries every piece: `flow-ingest` reads frames
//! from a file, a pipe or a UDP socket, and `flow-infer` turns a window of
//! them into a wait-time estimate. What was missing is the part that owns
//! them at runtime — reading the source, publishing each estimate, folding
//! the stream into the minute series, and reporting how healthy the stream
//! is while it does.
//!
//! # Where it runs
//!
//! On its own thread, because reading frames is a blocking loop and the
//! async runtime must stay free to answer requests. Estimates reach the HTTP
//! layer through a `watch` channel: the server never waits on the pipeline,
//! and a slow client cannot back-pressure sensing. A client that misses
//! intermediate values loses nothing that matters — it wants the current
//! estimate, not every one ever produced.
//!
//! # What starting requires
//!
//! A model, a tuned site, and at least one receiver. Those are exactly the
//! facts [`Readiness`](crate::Readiness) already tracks, so an appliance
//! that cannot estimate is not a failure to report — it is an installation
//! that has not reached calibration yet. The daemon serves regardless.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use flow_core::CsiFrame;
use flow_infer::{DensityModel, LiveConfig, LivePipeline};
use flow_ingest::{FrameSource, MacAddr, SenderKey, SourceConfig};
use serde::Serialize;

use crate::api::EdgeState;
use crate::config::ApplianceConfig;
use crate::error::PipelineError;
use crate::history::MinuteAggregator;
use crate::journal::{Event, EventKind};

/// Window over which a per-node frame rate is measured, in µs.
///
/// Long enough that a momentary gap does not read as a dead node, short
/// enough that a node which stops is visible within seconds.
const RATE_WINDOW_US: u64 = 5_000_000;

/// Where the daemon reads frames from.
///
/// In production this is the UDP socket the receivers stream to, and the
/// sender mapping comes from the paired nodes. The other forms exist so the
/// whole chain can be exercised against a recorded session on a machine
/// with no sensors attached.
#[derive(Debug, Clone)]
pub struct LiveOptions {
    /// A capture file, `-` for stdin, or `udp://ADDR:PORT`.
    pub input: String,
    /// Receiving node id, required for line-based inputs only.
    pub node_id: Option<String>,
    /// Keep only frames sensed from this transmitter.
    pub tx_mac: Option<MacAddr>,
}

/// How well the stream is feeding the pipeline.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StreamHealth {
    /// Whether a pipeline is running at all.
    pub running: bool,
    /// Frames accepted since the pipeline started.
    pub frames: u64,
    /// Estimates emitted since the pipeline started.
    pub estimates: u64,
    /// Edge timestamp of the most recent frame, if any.
    pub last_frame_us: Option<u64>,
    /// Per receiving node, keyed by node id and ordered for a stable
    /// display.
    pub nodes: BTreeMap<String, NodeHealth>,
}

/// How well one receiver is feeding the pipeline.
///
/// This is the first thing to look at on site: a silent node or a rate that
/// has collapsed explains most of what goes wrong, and neither is visible
/// from the estimate alone.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeHealth {
    /// Frames accepted from this node.
    pub frames: u64,
    /// Frames per second over the last complete measurement window.
    pub frames_per_second: f32,
    /// Edge timestamp of the most recent frame from this node.
    pub last_frame_us: Option<u64>,
}

/// Per-node counters, kept on the pipeline thread.
#[derive(Debug, Default)]
struct NodeCounter {
    frames: u64,
    last_frame_us: Option<u64>,
    window_start_us: u64,
    window_frames: u64,
    rate: f32,
}

impl NodeCounter {
    fn observe(&mut self, ts_us: u64) {
        self.frames += 1;
        self.last_frame_us = Some(ts_us);

        if self.window_start_us == 0 {
            self.window_start_us = ts_us;
        }
        self.window_frames += 1;

        let elapsed = ts_us.saturating_sub(self.window_start_us);
        if elapsed >= RATE_WINDOW_US {
            // Rate over the window that just completed, by the stream's own
            // clock rather than the wall clock: a replayed capture then
            // reports the rate it was recorded at.
            let seconds = elapsed as f64 / 1_000_000.0;
            self.rate = (self.window_frames as f64 / seconds) as f32;
            self.window_start_us = ts_us;
            self.window_frames = 0;
        }
    }

    fn health(&self) -> NodeHealth {
        NodeHealth {
            frames: self.frames,
            frames_per_second: self.rate,
            last_frame_us: self.last_frame_us,
        }
    }
}

/// Builds the frame source and the pipeline the appliance configuration
/// calls for.
///
/// # Errors
///
/// [`PipelineError`] naming what is missing: a model, a tuned site, a
/// receiver, or a source that cannot be opened.
fn build(
    config: &ApplianceConfig,
    data_dir: &Path,
    options: &LiveOptions,
) -> Result<(FrameSource, LivePipeline), PipelineError> {
    let site = config.site.ok_or(PipelineError::NoSiteTuning)?;

    let model_path = data_dir.join(crate::ACTIVE_MODEL);
    if !model_path.is_file() {
        return Err(PipelineError::NoModel { path: model_path });
    }

    // Receivers are identified by their reserved address at UDP intake
    // (ADR 0007); a receiver without one cannot be told from its sibling.
    let mut nodes: HashMap<SenderKey, String> = HashMap::new();
    for node in &config.nodes {
        if let Some(address) = node.address {
            nodes.insert(SenderKey::Ip(address), node.node_id.clone());
        }
    }

    let source = FrameSource::open(SourceConfig {
        input: options.input.clone(),
        node_id: options.node_id.clone(),
        nodes,
        tx_mac: options.tx_mac,
        start_ts_us: None,
    })
    .map_err(|err| PipelineError::Source {
        input: options.input.clone(),
        detail: err.to_string(),
    })?;

    let rx_nodes = source.rx_node_ids();
    if rx_nodes.is_empty() {
        return Err(PipelineError::NoReceivers);
    }

    let model = DensityModel::load(&model_path).map_err(|err| PipelineError::Model {
        path: model_path,
        detail: err.to_string(),
    })?;

    let pipeline = LivePipeline::new(
        model,
        LiveConfig {
            window_us: site.window_us,
            hop_us: site.hop_us,
            rx_nodes,
            wait: site.wait_config(),
        },
    )
    .map_err(|err| PipelineError::Pipeline(err.to_string()))?;

    Ok((source, pipeline))
}

/// Starts the pipeline on its own thread.
///
/// # Errors
///
/// [`PipelineError`] if the appliance is not in a state to estimate. The
/// caller reports it and keeps serving: an uncalibrated appliance is a
/// normal stage of an installation.
pub fn spawn_pipeline(
    state: EdgeState,
    config: &ApplianceConfig,
    data_dir: &Path,
    options: LiveOptions,
) -> Result<(), PipelineError> {
    let (source, pipeline) = build(config, data_dir, &options)?;
    state.set_stream_running(true);
    std::thread::spawn(move || {
        run(&state, source, pipeline);
        state.set_stream_running(false);
    });
    Ok(())
}

/// The blocking loop: read, infer, publish, fold, record.
fn run(state: &EdgeState, mut source: FrameSource, mut pipeline: LivePipeline) {
    let mut counters: BTreeMap<String, NodeCounter> = BTreeMap::new();
    let mut aggregator = MinuteAggregator::new();
    let mut frames: u64 = 0;
    let mut estimates: u64 = 0;

    while let Some(result) = source.next_frame() {
        let frame = match result {
            Ok(frame) => frame,
            // A malformed frame is already counted and skipped by the
            // reader; an I/O error ends the stream, and the staleness rule
            // takes the estimate off the air on its own.
            Err(err) => {
                state.record(
                    Event::new(EventKind::Stopped).with_detail(format!("stream error: {err}")),
                );
                break;
            }
        };

        frames += 1;
        observe(&mut counters, &frame);

        match pipeline.push(frame) {
            Ok(Some(estimate)) => {
                estimates += 1;
                if let Some(minute) = aggregator.push(&estimate) {
                    state.write_minute(&minute);
                }
                state.publish_estimate(estimate);
            }
            Ok(None) => {}
            Err(err) => {
                state.record(
                    Event::new(EventKind::Stopped).with_detail(format!("inference error: {err}")),
                );
                break;
            }
        }

        state.set_stream_health(health(&counters, frames, estimates));
    }

    // The minute in progress is worth keeping: a capture that ends at
    // 12:34:20 still says something about 12:34.
    if let Some(minute) = aggregator.flush() {
        state.write_minute(&minute);
    }
    state.set_stream_health(health(&counters, frames, estimates));
}

fn observe(counters: &mut BTreeMap<String, NodeCounter>, frame: &CsiFrame) {
    counters
        .entry(frame.node_id.clone())
        .or_default()
        .observe(frame.ts_us);
}

fn health(counters: &BTreeMap<String, NodeCounter>, frames: u64, estimates: u64) -> StreamHealth {
    StreamHealth {
        running: true,
        frames,
        estimates,
        last_frame_us: counters.values().filter_map(|c| c.last_frame_us).max(),
        nodes: counters
            .iter()
            .map(|(node_id, counter)| (node_id.clone(), counter.health()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000_000;

    #[test]
    fn a_node_counts_its_frames_and_remembers_the_last_one() {
        let mut counter = NodeCounter::default();
        counter.observe(NOW);
        counter.observe(NOW + 10_000);

        let health = counter.health();
        assert_eq!(health.frames, 2);
        assert_eq!(health.last_frame_us, Some(NOW + 10_000));
    }

    #[test]
    fn no_rate_is_reported_before_a_window_completes() {
        let mut counter = NodeCounter::default();
        for step in 0..10 {
            counter.observe(NOW + step * 100_000);
        }
        assert_eq!(
            counter.health().frames_per_second,
            0.0,
            "a partial window would report a rate measured over an unknown span"
        );
    }

    #[test]
    fn the_rate_reflects_the_completed_window() {
        let mut counter = NodeCounter::default();
        // 100 frames per second of stream time, for slightly over a window.
        let mut ts = NOW;
        for _ in 0..=(RATE_WINDOW_US / 10_000) {
            counter.observe(ts);
            ts += 10_000;
        }
        let rate = counter.health().frames_per_second;
        assert!((rate - 100.0).abs() < 1.0, "measured {rate} frames/s");
    }

    #[test]
    fn the_rate_is_measured_on_stream_time_not_wall_clock() {
        // A capture replayed faster than real time must report the rate it
        // was recorded at, not the speed of the replay.
        let mut counter = NodeCounter::default();
        let mut ts = NOW;
        for _ in 0..=(RATE_WINDOW_US / 20_000) {
            counter.observe(ts);
            ts += 20_000;
        }
        let rate = counter.health().frames_per_second;
        assert!((rate - 50.0).abs() < 1.0, "measured {rate} frames/s");
    }

    #[test]
    fn a_node_that_falls_silent_keeps_its_last_rate_and_timestamp() {
        // The rate cannot decay on its own — nothing is observed — so the
        // display must reason from `last_frame_us`, which is why it is
        // reported alongside.
        let mut counter = NodeCounter::default();
        let mut ts = NOW;
        for _ in 0..=(RATE_WINDOW_US / 10_000) {
            counter.observe(ts);
            ts += 10_000;
        }
        let silent_since = counter.health().last_frame_us.unwrap();
        assert!(counter.health().frames_per_second > 0.0);
        assert_eq!(silent_since, ts - 10_000);
    }

    #[test]
    fn health_summarises_every_node_seen() {
        let mut counters = BTreeMap::new();
        let frame = |node: &str, ts: u64| {
            CsiFrame::new(ts, node, -50, 7, vec![1.0, 2.0], vec![0.0, 0.1]).unwrap()
        };
        observe(&mut counters, &frame("rx-1", NOW));
        observe(&mut counters, &frame("rx-2", NOW + 1_000));
        observe(&mut counters, &frame("rx-1", NOW + 2_000));

        let health = health(&counters, 3, 1);
        assert_eq!(health.frames, 3);
        assert_eq!(health.estimates, 1);
        assert_eq!(health.nodes.len(), 2);
        assert_eq!(health.nodes["rx-1"].frames, 2);
        assert_eq!(health.nodes["rx-2"].frames, 1);
        assert_eq!(health.last_frame_us, Some(NOW + 2_000));
    }

    #[test]
    fn an_idle_appliance_reports_no_stream() {
        let health = StreamHealth::default();
        assert!(!health.running);
        assert_eq!(health.frames, 0);
        assert!(health.nodes.is_empty());
    }
}
