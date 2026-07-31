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
use std::time::Duration;

use flow_core::CsiFrame;
use flow_infer::{DensityModel, LiveConfig, LivePipeline};
use flow_ingest::{FrameSource, MacAddr, SenderKey, SourceConfig, SourceError};
use serde::Serialize;

use crate::api::EdgeState;
use crate::config::ApplianceConfig;
use crate::error::PipelineError;
use crate::history::MinuteAggregator;
use crate::journal::{Event, EventKind};

/// How often the service schedule is re-evaluated, in µs.
///
/// Converting an instant to a local time is cheap but not free, and at a
/// hundred frames a second it would be done a hundred times to answer a
/// question that changes twice a day. Between checks the last answer holds.
const SERVICE_CHECK_US: u64 = 1_000_000;

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
    /// Whether the intake is reading the stream.
    pub running: bool,
    /// Whether an estimator is attached to it.
    ///
    /// Distinct from `running`: an appliance still being installed, or one
    /// outside its service hours, reads without estimating. Which of those it
    /// is comes from the installation readiness and the service state.
    pub estimating: bool,
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

/// How long an intake read waits before reporting that nothing arrived.
///
/// The intake's periodic work must not wait for traffic: before any node is
/// paired, no frame is ever yielded.
const INTAKE_TICK: Duration = Duration::from_millis(250);

/// The estimator, or why the appliance has none.
enum Estimation {
    Ready(Box<LivePipeline>),
    /// Carries no reason: the installation readiness already reports which
    /// step is outstanding, and two answers to one question drift apart.
    NotConfigured,
    /// A model is installed but unusable — a fault, not a step left to do.
    Broken(String),
}

/// Opens the frame source, and assembles an estimator if the configuration
/// allows one.
///
/// An appliance still being installed has no model and no paired nodes, yet
/// must listen: listening is how the nodes to pair are found (ADR 0017).
///
/// # Errors
///
/// [`PipelineError`] only when the stream cannot be read at all.
fn build(
    config: &ApplianceConfig,
    data_dir: &Path,
    options: &LiveOptions,
) -> Result<(FrameSource, Estimation), PipelineError> {
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
        read_timeout: Some(INTAKE_TICK),
    })
    .map_err(|err| PipelineError::Source {
        input: options.input.clone(),
        detail: err.to_string(),
    })?;

    let estimation = assemble(config, data_dir, &source);
    Ok((source, estimation))
}

/// Assembles the estimator, reporting why it could not rather than failing.
fn assemble(config: &ApplianceConfig, data_dir: &Path, source: &FrameSource) -> Estimation {
    let rx_nodes = source.rx_node_ids();
    let model_path = data_dir.join(crate::ACTIVE_MODEL);
    let (Some(site), true, false) = (config.site, model_path.is_file(), rx_nodes.is_empty()) else {
        return Estimation::NotConfigured;
    };

    let model = match DensityModel::load(&model_path) {
        Ok(model) => model,
        Err(err) => return Estimation::Broken(format!("loading {}: {err}", model_path.display())),
    };

    match LivePipeline::new(
        model,
        LiveConfig {
            window_us: site.window_us,
            hop_us: site.hop_us,
            rx_nodes,
            wait: site.wait_config(),
        },
    ) {
        Ok(pipeline) => Estimation::Ready(Box::new(pipeline)),
        Err(err) => Estimation::Broken(err.to_string()),
    }
}

/// Why the intake loop returned.
enum Exit {
    /// The stream ended or failed; there is nothing left to read.
    StreamEnded,
    /// The configuration changed, so the intake must be rebuilt from it.
    ConfigurationChanged,
}

/// Starts the intake on its own thread, and keeps it in step with the
/// configuration.
///
/// It is rebuilt on every configuration change, because what it was built
/// from — the sender mapping, the model, the site tuning — is exactly what the
/// installation writes.
///
/// # Errors
///
/// [`PipelineError`] only when the stream cannot be read at all. A missing
/// model or an unpaired node is not a failure: the appliance listens, and the
/// installation readiness reports what is outstanding.
pub fn spawn_pipeline(
    state: EdgeState,
    config: &ApplianceConfig,
    data_dir: &Path,
    options: LiveOptions,
) -> Result<(), PipelineError> {
    // Built once here so an unreadable stream is reported to the caller
    // rather than dying silently on a thread.
    let built = build(config, data_dir, &options)?;
    let data_dir = data_dir.to_path_buf();
    std::thread::spawn(move || supervise(&state, built, &data_dir, &options));
    Ok(())
}

/// Runs the intake, rebuilding it each time the configuration changes.
fn supervise(
    state: &EdgeState,
    first: (FrameSource, Estimation),
    data_dir: &Path,
    options: &LiveOptions,
) {
    let mut built = first;
    loop {
        let generation = state.config_generation();
        let (source, estimation) = built;
        let estimator = match estimation {
            Estimation::Ready(pipeline) => Some(*pipeline),
            Estimation::NotConfigured => None,
            Estimation::Broken(detail) => {
                state.record(
                    Event::new(EventKind::Stopped).with_detail(format!("model unusable: {detail}")),
                );
                None
            }
        };

        state.set_stream_running(true);
        state.set_estimating(estimator.is_some());

        match run(state, source, estimator, generation) {
            Exit::StreamEnded => break,
            // The old source is dropped by `run` returning, so its socket is
            // free before the new one binds it.
            Exit::ConfigurationChanged => {
                match build(&state.config_snapshot(), data_dir, options) {
                    Ok(next) => built = next,
                    Err(err) => {
                        state.record(
                            Event::new(EventKind::Stopped)
                                .with_detail(format!("intake stopped: {err}")),
                        );
                        break;
                    }
                }
            }
        }
    }
    state.set_estimating(false);
    state.set_stream_running(false);
}

/// Whether a read simply found nothing within the intake tick.
///
/// A timeout is the loop's heartbeat, not a fault: it is how the periodic
/// work runs while the stream is quiet.
fn is_quiet(err: &SourceError) -> bool {
    matches!(
        err,
        SourceError::Io(io) if matches!(
            io.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        )
    )
}

/// The blocking loop: read, infer, publish, fold, record.
///
/// Estimation is one stage of the iteration, skipped when there is nothing to
/// estimate with or the site is closed; everything else happens regardless.
fn run(
    state: &EdgeState,
    mut source: FrameSource,
    mut estimator: Option<LivePipeline>,
    generation: u64,
) -> Exit {
    let mut counters: BTreeMap<String, NodeCounter> = BTreeMap::new();
    let mut aggregator = MinuteAggregator::new();
    let mut frames: u64 = 0;
    let mut estimates: u64 = 0;

    // Outside service hours the appliance reads its stream but does not
    // estimate: Little's Law assumes a settled queue, so a wait computed at
    // three in the morning would be a number with nothing behind it.
    let mut open = true;
    let mut next_service_check: u64 = 0;

    let mut exit = Exit::StreamEnded;
    loop {
        let frame = match source.next_frame() {
            // A line-based stream ended; a UDP source never does.
            None => break,
            Some(Ok(frame)) => {
                frames += 1;
                observe(&mut counters, &frame);
                Some(frame)
            }
            Some(Err(err)) if is_quiet(&err) => None,
            // A malformed frame is already counted and skipped by the reader,
            // so what reaches here ends the stream. The staleness rule takes
            // the estimate off the air on its own.
            Some(Err(err)) => {
                state.record(
                    Event::new(EventKind::Stopped).with_detail(format!("stream error: {err}")),
                );
                break;
            }
        };

        let now = crate::now_us();
        if now >= next_service_check {
            // On this tick rather than per frame: the list changes when a node
            // is powered, not a hundred times a second.
            state.set_observations(source.observations());

            if state.config_generation() != generation {
                exit = Exit::ConfigurationChanged;
                break;
            }

            let was_open = open;
            open = state.service_state().open;
            next_service_check = now + SERVICE_CHECK_US;
            if was_open != open {
                // The minute in progress belongs to the period that just
                // ended; closing it here keeps the two from mixing.
                if let Some(minute) = aggregator.flush() {
                    state.write_minute(&minute);
                }
                state.record(Event::new(if open {
                    EventKind::ServiceOpened
                } else {
                    EventKind::ServiceClosed
                }));
            }
        }

        // Recording is the other thing a frame can be for, and the two are
        // exclusive by the runtime's own rule: a capture takes the stream, and
        // the estimator simply resumes when it gives it back.
        let recording = state.is_recording();
        if let Some(frame) = frame.as_ref()
            && recording
        {
            state.record_frame(frame);
        }

        if let (Some(frame), true, false, Some(pipeline)) =
            (frame, open, recording, estimator.as_mut())
        {
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
                        Event::new(EventKind::Stopped)
                            .with_detail(format!("inference error: {err}")),
                    );
                    break;
                }
            }
        }

        state.set_stream_health(health(&counters, frames, estimates, estimator.is_some()));
    }

    // The minute in progress is worth keeping: a capture that ends at
    // 12:34:20 still says something about 12:34.
    if let Some(minute) = aggregator.flush() {
        state.write_minute(&minute);
    }
    state.set_stream_health(health(&counters, frames, estimates, estimator.is_some()));
    exit
}

fn observe(counters: &mut BTreeMap<String, NodeCounter>, frame: &CsiFrame) {
    counters
        .entry(frame.node_id.clone())
        .or_default()
        .observe(frame.ts_us);
}

fn health(
    counters: &BTreeMap<String, NodeCounter>,
    frames: u64,
    estimates: u64,
    estimating: bool,
) -> StreamHealth {
    StreamHealth {
        running: true,
        estimating,
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
    fn a_quiet_read_is_the_loop_heartbeat_not_a_fault() {
        // Before any node is paired no frame is ever yielded. Treating the
        // timeout as an error would stop the intake exactly when it is the
        // only thing that can find the nodes to pair.
        for kind in [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut] {
            let err = SourceError::Io(std::io::Error::new(kind, "no datagram"));
            assert!(is_quiet(&err), "{kind:?} should be a heartbeat");
        }
    }

    #[test]
    fn a_real_stream_failure_is_not_mistaken_for_quiet() {
        let err = SourceError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionReset,
            "socket gone",
        ));
        assert!(!is_quiet(&err));
        assert!(!is_quiet(&SourceError::Config("bad input".into())));
    }

    #[test]
    fn an_appliance_with_nothing_configured_still_reads_its_stream() {
        // No model, no paired node, so no estimator — but the stream is read.
        let dir = tempfile::tempdir().unwrap();
        let config =
            ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", "correct-horse-battery");
        let options = LiveOptions {
            input: "udp://127.0.0.1:0".into(),
            node_id: None,
            tx_mac: None,
        };

        let (source, estimation) = build(&config, dir.path(), &options).unwrap();

        assert!(matches!(estimation, Estimation::NotConfigured));
        assert!(source.rx_node_ids().is_empty());
    }

    #[test]
    fn a_broken_model_is_a_fault_not_a_step_left_to_do() {
        // "Not configured yet" and "installed but unusable" must not look the
        // same: the first is normal mid-installation, the second needs saying.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(crate::ACTIVE_MODEL), b"not an onnx graph").unwrap();
        let mut config =
            ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", "correct-horse-battery");
        config.site = Some(crate::config::SiteTuning {
            people_per_class: [0.0, 4.0, 12.0, 25.0],
            service_rate_per_min: 6.0,
            smoothing_tau_s: 30.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.5,
            window_us: 5_000_000,
            hop_us: 1_000_000,
        });
        config.nodes = vec![crate::config::PairedNode {
            node_id: "rx-1".into(),
            role: flow_core::NodeRole::Rx,
            mac: Some("aa:bb:cc:00:00:01".into()),
            address: Some("192.168.4.51".parse().unwrap()),
        }];
        let options = LiveOptions {
            input: "udp://127.0.0.1:0".into(),
            node_id: None,
            tx_mac: None,
        };

        let (_source, estimation) = build(&config, dir.path(), &options).unwrap();

        assert!(matches!(estimation, Estimation::Broken(_)));
    }

    #[test]
    fn an_unreadable_stream_is_still_reported_to_the_caller() {
        let dir = tempfile::tempdir().unwrap();
        let config =
            ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", "correct-horse-battery");
        let options = LiveOptions {
            input: "/nonexistent/capture.txt".into(),
            node_id: Some("rx-1".into()),
            tx_mac: None,
        };

        let outcome = build(&config, dir.path(), &options);

        assert!(matches!(outcome, Err(PipelineError::Source { .. })));
    }

    #[test]
    fn health_separates_reading_from_estimating() {
        let counters = BTreeMap::new();
        assert!(health(&counters, 0, 0, false).running);
        assert!(!health(&counters, 0, 0, false).estimating);
        assert!(health(&counters, 0, 0, true).estimating);
    }

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

        let health = health(&counters, 3, 1, true);
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
