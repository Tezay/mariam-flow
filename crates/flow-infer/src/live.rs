//! Live inference engine: a real-time sliding window over the frame
//! stream, driving the full chain features → model → wait estimate.
//!
//! [`LivePipeline`] is event-driven: feed it frames as they arrive
//! ([`LivePipeline::push`]) and it emits a [`WaitEstimate`] every `hop`
//! of stream time once the first window has filled. The window is the
//! trailing time span `(t_newest − window, t_newest]` — statistically
//! equivalent to the training windows, whose anchoring is irrelevant as
//! long as the *duration* matches the one the model was trained with.
//!
//! After a capture gap, at most one estimate is emitted per pushed frame:
//! missed hops are skipped, never replayed (stale estimates are worse
//! than none).

use std::collections::VecDeque;

use flow_core::{CsiFrame, FrameError, TimestampUs};
use thiserror::Error;

use crate::features::{FeatureError, NODE_FEATURES, window_vector};
use crate::model::{DensityModel, InferError};
use crate::wait::{ConfigError, WaitConfig, WaitEstimate, WaitEstimator};

/// Configuration of the live pipeline.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveConfig {
    /// Window duration in µs — **must match the training window**.
    pub window_us: u64,
    /// Emission period in µs of stream time.
    pub hop_us: u64,
    /// Receiving nodes, in the sorted order the model was trained with.
    pub rx_nodes: Vec<String>,
    /// Per-site wait-estimation parameters.
    pub wait: WaitConfig,
}

/// Failure while building or running the live pipeline.
#[derive(Debug, Error)]
pub enum LiveError {
    /// Window or hop duration is zero.
    #[error("window and hop durations must be positive")]
    InvalidTiming,
    /// No receiving node configured.
    #[error("at least one RX node is required")]
    NoRxNodes,
    /// The model's input width does not match the configured nodes.
    #[error("model expects {model} features but {nodes} nodes yield {expected}")]
    FeatureCountMismatch {
        /// Feature count declared by the model.
        model: usize,
        /// Number of configured RX nodes.
        nodes: usize,
        /// Feature count the configuration would produce.
        expected: usize,
    },
    /// Invalid wait-estimation parameters.
    #[error(transparent)]
    Wait(#[from] ConfigError),
    /// A pushed frame is structurally invalid.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// A pushed frame goes back in time.
    #[error("out-of-order frame: {got} after {last}")]
    OutOfOrder {
        /// Newest timestamp seen so far.
        last: TimestampUs,
        /// The offending timestamp.
        got: TimestampUs,
    },
    /// Feature extraction failed (e.g. inconsistent subcarrier counts).
    #[error(transparent)]
    Feature(#[from] FeatureError),
    /// Model execution failed.
    #[error(transparent)]
    Infer(#[from] InferError),
}

/// Counters accumulated by the pipeline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveStats {
    /// Frames accepted into the buffer.
    pub frames: u64,
    /// Estimates emitted.
    pub estimates: u64,
    /// Due emissions skipped because the window was incomplete for at
    /// least one node.
    pub incomplete_windows: u64,
}

/// The live inference engine. See the module documentation.
#[derive(Debug)]
pub struct LivePipeline {
    config: LiveConfig,
    model: DensityModel,
    estimator: WaitEstimator,
    buffer: VecDeque<CsiFrame>,
    next_emit_us: Option<TimestampUs>,
    newest_us: Option<TimestampUs>,
    stats: LiveStats,
}

impl LivePipeline {
    /// Builds a pipeline, validating the configuration against the model.
    ///
    /// # Errors
    ///
    /// See [`LiveError`]; in particular the model input width must equal
    /// `rx_nodes.len() × 7`.
    pub fn new(model: DensityModel, config: LiveConfig) -> Result<Self, LiveError> {
        if config.window_us == 0 || config.hop_us == 0 {
            return Err(LiveError::InvalidTiming);
        }
        if config.rx_nodes.is_empty() {
            return Err(LiveError::NoRxNodes);
        }
        let expected = config.rx_nodes.len() * NODE_FEATURES.len();
        if model.n_features() != expected {
            return Err(LiveError::FeatureCountMismatch {
                model: model.n_features(),
                nodes: config.rx_nodes.len(),
                expected,
            });
        }
        let estimator = WaitEstimator::new(config.wait)?;
        Ok(Self {
            config,
            model,
            estimator,
            buffer: VecDeque::new(),
            next_emit_us: None,
            newest_us: None,
            stats: LiveStats::default(),
        })
    }

    /// Counters accumulated so far.
    #[must_use]
    pub fn stats(&self) -> LiveStats {
        self.stats
    }

    /// Feeds one frame (in stream order) and returns an estimate when one
    /// is due.
    ///
    /// # Errors
    ///
    /// Structural frame errors, ordering violations, and inference
    /// failures; see [`LiveError`].
    pub fn push(&mut self, frame: CsiFrame) -> Result<Option<WaitEstimate>, LiveError> {
        frame.validate()?;
        if let Some(newest) = self.newest_us {
            if frame.ts_us < newest {
                return Err(LiveError::OutOfOrder {
                    last: newest,
                    got: frame.ts_us,
                });
            }
        }
        let now = frame.ts_us;
        self.newest_us = Some(now);
        if self.next_emit_us.is_none() {
            self.next_emit_us = Some(now + self.config.window_us);
        }
        self.buffer.push_back(frame);
        self.stats.frames += 1;

        // Evict frames that fell out of the trailing window (t−window, t].
        while let Some(front) = self.buffer.front() {
            if now - front.ts_us >= self.config.window_us {
                self.buffer.pop_front();
            } else {
                break;
            }
        }

        let Some(mut next) = self.next_emit_us else {
            return Ok(None);
        };
        if now < next {
            return Ok(None);
        }
        // Skip missed hops after a gap: schedule strictly after `now`.
        while next <= now {
            next += self.config.hop_us;
        }
        self.next_emit_us = Some(next);

        let frames: &[CsiFrame] = self.buffer.make_contiguous();
        let Some(vector) = window_vector(frames, &self.config.rx_nodes, self.config.window_us)?
        else {
            self.stats.incomplete_windows += 1;
            return Ok(None);
        };
        let features: Vec<f32> = vector.iter().map(|v| *v as f32).collect();
        let prediction = self.model.predict(&features)?;
        let estimate = self.estimator.update(now, &prediction);
        self.stats.estimates += 1;
        Ok(Some(estimate))
    }
}
