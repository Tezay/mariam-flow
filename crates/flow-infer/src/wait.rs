//! Wait-time estimation: Little's Law over the classifier's probability
//! distribution, with time-aware exponential smoothing and display-class
//! hysteresis.
//!
//! The chain (ADR 0002, `docs/architecture.md`):
//!
//! ```text
//! probabilities ─ E[L] = Σ pᵢ·L(classᵢ) ─ EMA ─ ÷λ ─→ wait (minutes)
//!              └─ expected level ──────── EMA ─ hysteresis ─→ display class
//! ```
//!
//! `L(class)` (people represented by each class) and `λ` (service rate)
//! are per-site calibrated parameters. Little's Law `W = L/λ` assumes a
//! stable regime — the estimate degrades gracefully when the queue is
//! still growing, which the displayed range absorbs.

use flow_core::{DensityClass, TimestampUs};
use thiserror::Error;

use crate::model::Prediction;

/// Per-site calibration and tuning of the wait-time estimator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaitConfig {
    /// Calibrated people count represented by each density class at this
    /// site, in class order (`empty`, `low`, `medium`, `saturated`).
    pub people_per_class: [f32; 4],
    /// Service rate λ in people served per minute, calibrated per time
    /// slot; must be positive.
    pub service_rate_per_min: f32,
    /// Time constant τ of the exponential smoothing, in seconds: after τ
    /// seconds, ~63 % of a step change is absorbed (~95 % after 3τ).
    pub smoothing_tau_s: f32,
    /// Hysteresis half-width on the 0–3 level scale. The display class
    /// only changes when the smoothed level crosses a class boundary by
    /// more than this margin; must be in `[0, 0.5)`.
    pub hysteresis_margin: f32,
    /// Estimates with a classifier confidence below this threshold are
    /// flagged unreliable (the publishing layer hides them).
    pub min_confidence: f32,
}

/// Invalid [`WaitConfig`] value.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum ConfigError {
    /// The service rate is not a positive, finite number.
    #[error("service rate must be finite and > 0, got {0}")]
    InvalidServiceRate(f32),
    /// The smoothing time constant is not a positive, finite number.
    #[error("smoothing time constant must be finite and > 0, got {0}")]
    InvalidTau(f32),
    /// The hysteresis margin is outside `[0, 0.5)`.
    #[error("hysteresis margin must be in [0, 0.5), got {0}")]
    InvalidMargin(f32),
    /// A per-class people count is negative or not finite.
    #[error("people per class must be finite and non-negative")]
    InvalidPeople,
    /// The confidence threshold is outside `[0, 1]`.
    #[error("min confidence must be in [0, 1], got {0}")]
    InvalidConfidence(f32),
}

impl WaitConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if !(self.service_rate_per_min.is_finite() && self.service_rate_per_min > 0.0) {
            return Err(ConfigError::InvalidServiceRate(self.service_rate_per_min));
        }
        if !(self.smoothing_tau_s.is_finite() && self.smoothing_tau_s > 0.0) {
            return Err(ConfigError::InvalidTau(self.smoothing_tau_s));
        }
        if !(self.hysteresis_margin.is_finite() && (0.0..0.5).contains(&self.hysteresis_margin)) {
            return Err(ConfigError::InvalidMargin(self.hysteresis_margin));
        }
        if self
            .people_per_class
            .iter()
            .any(|count| !count.is_finite() || *count < 0.0)
        {
            return Err(ConfigError::InvalidPeople);
        }
        if !(self.min_confidence.is_finite() && (0.0..=1.0).contains(&self.min_confidence)) {
            return Err(ConfigError::InvalidConfidence(self.min_confidence));
        }
        Ok(())
    }
}

/// Expected people count under the classifier's distribution:
/// `E[L] = Σ pᵢ · L(classᵢ)`, using the site's calibrated mapping.
#[must_use]
pub fn expected_people(prediction: &Prediction, people_per_class: &[f32; 4]) -> f32 {
    prediction
        .probabilities
        .iter()
        .zip(people_per_class)
        .map(|(p, people)| p * people)
        .sum()
}

/// Time-aware exponential moving average (first-order low-pass filter).
///
/// The continuous filter `τ·ds/dt = x − s` is discretized exactly for the
/// elapsed time between samples: `s ← s + (1 − e^(−Δt/τ))·(x − s)`. A
/// fixed-α EMA would smooth more or less depending on the frame rate;
/// this form gives the same time constant regardless of sampling
/// irregularity. A non-advancing or backwards timestamp leaves the state
/// unchanged (Δt clamps to 0).
#[derive(Debug, Clone, Copy)]
pub struct Ema {
    tau_s: f32,
    state: Option<(TimestampUs, f32)>,
}

impl Ema {
    /// Creates a filter with time constant `tau_s` (seconds, > 0).
    #[must_use]
    pub fn new(tau_s: f32) -> Self {
        Self { tau_s, state: None }
    }

    /// Feeds one sample and returns the smoothed value. The first sample
    /// initializes the state as-is.
    pub fn update(&mut self, ts_us: TimestampUs, value: f32) -> f32 {
        let smoothed = match self.state {
            None => value,
            Some((last_ts, previous)) => {
                let dt_s = ts_us.saturating_sub(last_ts) as f32 / 1e6;
                let alpha = 1.0 - (-dt_s / self.tau_s).exp();
                previous + alpha * (value - previous)
            }
        };
        self.state = Some((ts_us.max(self.state.map_or(0, |(t, _)| t)), smoothed));
        smoothed
    }
}

/// Hysteresis on the displayed density class (a Schmitt trigger on the
/// continuous 0–3 level).
///
/// The class only changes when the level crosses the boundary to a
/// neighboring class by more than `margin`; as long as the smoothed
/// level's noise stays below `margin`, the display cannot flap at a
/// boundary. Large jumps move directly to the nearest class.
#[derive(Debug, Clone, Copy)]
pub struct ClassHysteresis {
    margin: f32,
    current: Option<DensityClass>,
}

impl ClassHysteresis {
    /// Creates a trigger with the given half-width margin (`[0, 0.5)`).
    #[must_use]
    pub fn new(margin: f32) -> Self {
        Self {
            margin,
            current: None,
        }
    }

    /// Feeds the current smoothed level and returns the class to display.
    pub fn update(&mut self, level: f32) -> DensityClass {
        let next = match self.current {
            None => nearest_class(level),
            Some(current) => {
                let center = f32::from(current.as_u8());
                if level > center + 0.5 + self.margin || level < center - 0.5 - self.margin {
                    nearest_class(level)
                } else {
                    current
                }
            }
        };
        self.current = Some(next);
        next
    }
}

fn nearest_class(level: f32) -> DensityClass {
    let index = level.round().clamp(0.0, 3.0) as usize;
    DensityClass::ALL[index]
}

/// One published estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaitEstimate {
    /// Estimated waiting time in minutes (`W = E[L]smoothed / λ`).
    pub wait_minutes: f32,
    /// Smoothed expected people count in the zone.
    pub people: f32,
    /// Smoothed density level on the continuous 0–3 scale.
    pub level: f32,
    /// Class to display, stabilized by hysteresis.
    pub display_class: DensityClass,
    /// Classifier confidence behind this estimate.
    pub confidence: f32,
    /// Whether the estimate meets the confidence threshold; unreliable
    /// estimates are computed but must not be shown to end users.
    pub reliable: bool,
}

/// Stateful estimator turning classifier outputs into wait estimates.
#[derive(Debug)]
pub struct WaitEstimator {
    config: WaitConfig,
    people_ema: Ema,
    level_ema: Ema,
    hysteresis: ClassHysteresis,
}

impl WaitEstimator {
    /// Builds an estimator from a validated configuration.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] describing the first invalid parameter.
    pub fn new(config: WaitConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        Ok(Self {
            config,
            people_ema: Ema::new(config.smoothing_tau_s),
            level_ema: Ema::new(config.smoothing_tau_s),
            hysteresis: ClassHysteresis::new(config.hysteresis_margin),
        })
    }

    /// Feeds one classifier output (with its edge timestamp) and returns
    /// the current estimate.
    pub fn update(&mut self, ts_us: TimestampUs, prediction: &Prediction) -> WaitEstimate {
        let people = self.people_ema.update(
            ts_us,
            expected_people(prediction, &self.config.people_per_class),
        );
        let level = self.level_ema.update(ts_us, prediction.expected_level());
        let display_class = self.hysteresis.update(level);
        let confidence = prediction.confidence();
        WaitEstimate {
            wait_minutes: people / self.config.service_rate_per_min,
            people,
            level,
            display_class,
            confidence,
            reliable: confidence >= self.config.min_confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    fn config() -> WaitConfig {
        WaitConfig {
            people_per_class: [0.0, 2.0, 12.0, 30.0],
            service_rate_per_min: 4.0,
            smoothing_tau_s: 10.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.5,
        }
    }

    fn prediction(probabilities: [f32; 4]) -> Prediction {
        Prediction { probabilities }
    }

    #[test]
    fn expected_people_is_the_mapped_expectation() {
        let p = prediction([0.1, 0.2, 0.6, 0.1]);
        // 0·0.1 + 2·0.2 + 12·0.6 + 30·0.1 = 10.6
        assert!((expected_people(&p, &config().people_per_class) - 10.6).abs() < EPS);
    }

    #[test]
    fn first_update_applies_littles_law_directly() {
        let mut estimator = WaitEstimator::new(config()).unwrap();
        let estimate = estimator.update(0, &prediction([0.1, 0.2, 0.6, 0.1]));
        assert!((estimate.people - 10.6).abs() < EPS);
        // W = 10.6 people / 4 people·min⁻¹ = 2.65 min
        assert!((estimate.wait_minutes - 2.65).abs() < EPS);
        assert!(estimate.reliable);
    }

    #[test]
    fn ema_first_sample_initializes_state() {
        let mut ema = Ema::new(10.0);
        assert!((ema.update(0, 10.0) - 10.0).abs() < EPS);
    }

    #[test]
    fn ema_absorbs_63_percent_after_one_time_constant() {
        let mut ema = Ema::new(10.0);
        ema.update(0, 10.0);
        // Δt = τ = 10 s → α = 1 − e⁻¹ ≈ 0.63212
        let smoothed = ema.update(10_000_000, 20.0);
        assert!((smoothed - 16.3212).abs() < 1e-3, "got {smoothed}");
    }

    #[test]
    fn ema_is_sampling_rate_independent() {
        // Same 10 s of the same signal, sampled once vs. ten times.
        let mut coarse = Ema::new(10.0);
        coarse.update(0, 0.0);
        let once = coarse.update(10_000_000, 1.0);

        let mut fine = Ema::new(10.0);
        fine.update(0, 0.0);
        let mut many = 0.0;
        for step in 1..=10 {
            many = fine.update(step * 1_000_000, 1.0);
        }
        assert!((once - many).abs() < 1e-4, "coarse {once} vs fine {many}");
    }

    #[test]
    fn ema_ignores_non_advancing_timestamps() {
        let mut ema = Ema::new(10.0);
        ema.update(1_000_000, 10.0);
        assert!((ema.update(1_000_000, 100.0) - 10.0).abs() < EPS);
        assert!((ema.update(500_000, 100.0) - 10.0).abs() < EPS);
    }

    #[test]
    fn hysteresis_filters_boundary_noise() {
        let mut trigger = ClassHysteresis::new(0.15);
        assert_eq!(trigger.update(1.2), DensityClass::Low);
        // Boundary Low/Medium is 1.5; with margin the switch needs > 1.65.
        assert_eq!(trigger.update(1.6), DensityClass::Low);
        assert_eq!(trigger.update(1.7), DensityClass::Medium);
        // Dropping back needs < 1.35.
        assert_eq!(trigger.update(1.4), DensityClass::Medium);
        assert_eq!(trigger.update(1.3), DensityClass::Low);
    }

    #[test]
    fn hysteresis_follows_large_jumps_immediately() {
        let mut trigger = ClassHysteresis::new(0.15);
        assert_eq!(trigger.update(0.2), DensityClass::Empty);
        assert_eq!(trigger.update(2.9), DensityClass::Saturated);
    }

    #[test]
    fn low_confidence_is_flagged_unreliable() {
        let mut estimator = WaitEstimator::new(config()).unwrap();
        let estimate = estimator.update(0, &prediction([0.3, 0.3, 0.25, 0.15]));
        assert!((estimate.confidence - 0.3).abs() < EPS);
        assert!(!estimate.reliable);
    }

    #[test]
    fn smoothing_dampens_a_sudden_spike() {
        let mut estimator = WaitEstimator::new(config()).unwrap();
        estimator.update(0, &prediction([1.0, 0.0, 0.0, 0.0]));
        // One second later the classifier claims saturated; τ = 10 s keeps
        // the published estimate far below the raw 30-people reading.
        let estimate = estimator.update(1_000_000, &prediction([0.0, 0.0, 0.0, 1.0]));
        assert!(estimate.people < 4.0, "people = {}", estimate.people);
        assert_eq!(estimate.display_class, DensityClass::Empty);
    }

    #[test]
    fn invalid_configs_are_rejected() {
        type Case = (fn(&mut WaitConfig), ConfigError);
        let valid = config();
        let cases: [Case; 5] = [
            (
                |c| c.service_rate_per_min = 0.0,
                ConfigError::InvalidServiceRate(0.0),
            ),
            (|c| c.smoothing_tau_s = -1.0, ConfigError::InvalidTau(-1.0)),
            (
                |c| c.hysteresis_margin = 0.5,
                ConfigError::InvalidMargin(0.5),
            ),
            (
                |c| c.people_per_class[2] = f32::NAN,
                ConfigError::InvalidPeople,
            ),
            (
                |c| c.min_confidence = 1.5,
                ConfigError::InvalidConfidence(1.5),
            ),
        ];
        for (mutate, expected) in cases {
            let mut broken = valid;
            mutate(&mut broken);
            assert_eq!(WaitEstimator::new(broken).unwrap_err(), expected);
        }
    }
}
