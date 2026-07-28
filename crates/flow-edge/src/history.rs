//! Folding the live estimate stream into one row per minute.
//!
//! The pipeline emits an estimate roughly every second. Keeping every one of
//! them would cost about 1.9 GB a year on a card shared with capture
//! sessions, to answer questions — how busy was last Tuesday, is the queue
//! worse this term than last — that a per-minute resolution already answers:
//! any chart of a day's activity paints several minutes to a pixel.
//! Aggregating here rather than at query time also means the cost is paid
//! once, on a machine that has little to spare.
//!
//! Averaging over a minute is straightforward for the continuous values.
//! The displayed class is not averaged but taken as the **most frequent** of
//! the minute: classes are ordinal labels, and the mean of `empty` and
//! `saturated` is not `medium` — it is nothing at all. The mean *level* is
//! kept alongside, which is the continuous quantity a caller should use when
//! it wants a smooth curve.
//!
//! Reliability is carried as a count rather than a flag. A minute where two
//! samples in sixty were trustworthy is not a reliable minute, and only the
//! ratio can say so.

use flow_core::{DensityClass, TimestampUs};
use flow_infer::WaitEstimate;
use serde::Serialize;

/// Length of an aggregation bucket, in µs.
pub const MINUTE_US: u64 = 60 * 1_000_000;

/// One minute of estimates, folded.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MinuteSummary {
    /// Start of the minute, in µs since the Unix epoch.
    pub minute_us: TimestampUs,
    /// Estimates folded into this row.
    pub samples: u32,
    /// Of those, how many met the site's confidence threshold.
    pub reliable_samples: u32,
    /// Mean waiting time over the minute, in minutes.
    pub wait_minutes: f32,
    /// Mean density level on the continuous 0–3 scale.
    pub level: f32,
    /// Most frequent displayed class of the minute.
    pub class: DensityClass,
    /// Mean classifier confidence.
    pub confidence: f32,
}

impl MinuteSummary {
    /// Whether most of the minute was trustworthy.
    ///
    /// A publishing layer needs one bit, and half the samples is the
    /// natural place to draw it: below that, the minute says more about the
    /// model's uncertainty than about the queue.
    #[must_use]
    pub fn is_reliable(&self) -> bool {
        self.samples > 0 && self.reliable_samples * 2 >= self.samples
    }
}

/// Accumulates estimates and yields one [`MinuteSummary`] per minute.
#[derive(Debug, Default)]
pub struct MinuteAggregator {
    open: Option<Bucket>,
}

#[derive(Debug)]
struct Bucket {
    minute_us: TimestampUs,
    samples: u32,
    reliable_samples: u32,
    wait_sum: f64,
    level_sum: f64,
    confidence_sum: f64,
    /// Occurrences of each displayed class, indexed by its integer value.
    class_counts: [u32; 4],
}

impl MinuteAggregator {
    /// An aggregator with no minute open.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Folds one estimate in.
    ///
    /// Returns the summary of the previous minute when this estimate opens a
    /// new one — so a caller writes to storage exactly when a minute closes,
    /// and never has to watch a clock itself.
    ///
    /// Estimates are expected in stream order. One arriving late, after its
    /// minute has closed, is counted in the currently open minute rather
    /// than dropped: it is at most a second out of place, and losing it
    /// would be a worse distortion than misplacing it.
    pub fn push(&mut self, estimate: &WaitEstimate) -> Option<MinuteSummary> {
        let minute_us = estimate.ts_us - (estimate.ts_us % MINUTE_US);

        // Only a *later* minute closes the open one. An estimate belonging
        // to an already-closed minute is folded into the open bucket
        // instead of reopening the past: rewinding would emit rows out of
        // order and collide with the one already written for that minute.
        let closed = match &self.open {
            Some(bucket) if minute_us > bucket.minute_us => self.open.take().map(Bucket::summarize),
            _ => None,
        };
        let bucket = self.open.get_or_insert_with(|| Bucket::new(minute_us));
        bucket.add(estimate);
        closed
    }

    /// Closes the minute in progress, if any.
    ///
    /// Called when the stream ends or the daemon shuts down, so the last
    /// partial minute is not lost.
    pub fn flush(&mut self) -> Option<MinuteSummary> {
        self.open.take().map(Bucket::summarize)
    }

    /// Whether a minute is currently accumulating.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

impl Bucket {
    fn new(minute_us: TimestampUs) -> Self {
        Self {
            minute_us,
            samples: 0,
            reliable_samples: 0,
            wait_sum: 0.0,
            level_sum: 0.0,
            confidence_sum: 0.0,
            class_counts: [0; 4],
        }
    }

    fn add(&mut self, estimate: &WaitEstimate) {
        self.samples += 1;
        if estimate.reliable {
            self.reliable_samples += 1;
        }
        self.wait_sum += f64::from(estimate.wait_minutes);
        self.level_sum += f64::from(estimate.level);
        self.confidence_sum += f64::from(estimate.confidence);
        self.class_counts[usize::from(estimate.display_class.as_u8())] += 1;
    }

    fn summarize(self) -> MinuteSummary {
        let samples = f64::from(self.samples.max(1));
        MinuteSummary {
            minute_us: self.minute_us,
            samples: self.samples,
            reliable_samples: self.reliable_samples,
            wait_minutes: (self.wait_sum / samples) as f32,
            level: (self.level_sum / samples) as f32,
            class: self.dominant_class(),
            confidence: (self.confidence_sum / samples) as f32,
        }
    }

    /// The most frequent class, ties going to the denser one.
    ///
    /// Understating a queue is the more damaging error: someone told to
    /// expect a short wait and met with a long one loses trust in the
    /// system, which is the whole product.
    fn dominant_class(&self) -> DensityClass {
        let mut best = 0usize;
        for (value, count) in self.class_counts.iter().enumerate() {
            if *count >= self.class_counts[best] {
                best = value;
            }
        }
        DensityClass::try_from(u8::try_from(best).unwrap_or(0)).unwrap_or(DensityClass::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000_000;

    fn at(ts_us: u64, class: DensityClass, wait: f32, reliable: bool) -> WaitEstimate {
        WaitEstimate {
            ts_us,
            wait_minutes: wait,
            people: 10.0,
            level: f32::from(class.as_u8()),
            display_class: class,
            confidence: if reliable { 0.9 } else { 0.2 },
            reliable,
        }
    }

    /// Start of the minute containing `NOW`.
    fn minute_of(ts: u64) -> u64 {
        ts - (ts % MINUTE_US)
    }

    #[test]
    fn nothing_closes_until_a_minute_turns() {
        let mut aggregator = MinuteAggregator::new();
        for second in 0..30 {
            let estimate = at(NOW + second * 1_000_000, DensityClass::Low, 2.0, true);
            assert!(aggregator.push(&estimate).is_none(), "second {second}");
        }
        assert!(aggregator.is_open());
    }

    #[test]
    fn a_minute_closes_when_the_next_one_opens() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        for second in 0..60 {
            aggregator.push(&at(
                start + second * 1_000_000,
                DensityClass::Medium,
                6.0,
                true,
            ));
        }
        let closed = aggregator
            .push(&at(start + MINUTE_US, DensityClass::Medium, 6.0, true))
            .expect("the full minute closes");

        assert_eq!(closed.minute_us, start);
        assert_eq!(closed.samples, 60);
        assert_eq!(closed.class, DensityClass::Medium);
        assert!((closed.wait_minutes - 6.0).abs() < 1e-5);
        assert!(aggregator.is_open(), "the new minute is accumulating");
    }

    #[test]
    fn the_minute_boundary_is_absolute_not_relative_to_the_first_sample() {
        // Starting mid-minute must not shift every later boundary.
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        aggregator.push(&at(start + 45_000_000, DensityClass::Low, 1.0, true));
        let closed = aggregator
            .push(&at(
                start + MINUTE_US + 1_000_000,
                DensityClass::Low,
                1.0,
                true,
            ))
            .expect("closes on the true boundary");
        assert_eq!(closed.minute_us, start);
        assert_eq!(closed.samples, 1, "a partial minute is still a minute");
    }

    #[test]
    fn continuous_values_are_averaged() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        aggregator.push(&at(start, DensityClass::Low, 2.0, true));
        aggregator.push(&at(start + 1_000_000, DensityClass::Low, 4.0, true));
        aggregator.push(&at(start + 2_000_000, DensityClass::Low, 6.0, true));

        let closed = aggregator.flush().unwrap();
        assert!((closed.wait_minutes - 4.0).abs() < 1e-5, "mean of 2, 4, 6");
        assert_eq!(closed.samples, 3);
    }

    #[test]
    fn the_class_is_the_most_frequent_not_the_mean() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        // Two empties and one saturated: the mean level would suggest the
        // middle, but the minute was mostly empty.
        aggregator.push(&at(start, DensityClass::Empty, 0.0, true));
        aggregator.push(&at(start + 1_000_000, DensityClass::Empty, 0.0, true));
        aggregator.push(&at(start + 2_000_000, DensityClass::Saturated, 20.0, true));

        let closed = aggregator.flush().unwrap();
        assert_eq!(closed.class, DensityClass::Empty);
        assert!(
            (closed.level - 1.0).abs() < 1e-5,
            "the mean level is kept too"
        );
    }

    #[test]
    fn a_tie_resolves_towards_the_busier_class() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        aggregator.push(&at(start, DensityClass::Low, 2.0, true));
        aggregator.push(&at(start + 1_000_000, DensityClass::Medium, 8.0, true));

        assert_eq!(
            aggregator.flush().unwrap().class,
            DensityClass::Medium,
            "understating a queue costs more trust than overstating it"
        );
    }

    #[test]
    fn reliability_is_a_ratio_not_a_flag() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        for second in 0..10 {
            let reliable = second < 2;
            aggregator.push(&at(
                start + second * 1_000_000,
                DensityClass::Low,
                2.0,
                reliable,
            ));
        }
        let closed = aggregator.flush().unwrap();

        assert_eq!(closed.samples, 10);
        assert_eq!(closed.reliable_samples, 2);
        assert!(
            !closed.is_reliable(),
            "two samples in ten is not a reliable minute"
        );
    }

    #[test]
    fn a_majority_of_reliable_samples_makes_the_minute_reliable() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        for second in 0..10 {
            aggregator.push(&at(
                start + second * 1_000_000,
                DensityClass::Low,
                2.0,
                second < 5,
            ));
        }
        assert!(
            aggregator.flush().unwrap().is_reliable(),
            "exactly half counts"
        );
    }

    #[test]
    fn a_late_estimate_lands_in_the_open_minute_rather_than_being_dropped() {
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        aggregator.push(&at(start, DensityClass::Low, 2.0, true));
        aggregator.push(&at(start + MINUTE_US, DensityClass::Low, 2.0, true));
        // Arrives after its own minute closed.
        aggregator.push(&at(start + 30_000_000, DensityClass::Low, 2.0, true));

        let closed = aggregator.flush().unwrap();
        assert_eq!(closed.minute_us, start + MINUTE_US);
        assert_eq!(closed.samples, 2, "counted, not discarded");
    }

    #[test]
    fn flushing_an_idle_aggregator_yields_nothing() {
        let mut aggregator = MinuteAggregator::new();
        assert!(aggregator.flush().is_none());
        assert!(!aggregator.is_open());
    }

    #[test]
    fn skipping_several_minutes_closes_only_the_one_that_was_open() {
        // A capture gap must not fabricate rows for the silent minutes.
        let mut aggregator = MinuteAggregator::new();
        let start = minute_of(NOW);
        aggregator.push(&at(start, DensityClass::Low, 2.0, true));
        let closed = aggregator
            .push(&at(start + 10 * MINUTE_US, DensityClass::Low, 2.0, true))
            .expect("the open minute closes");

        assert_eq!(closed.minute_us, start);
        assert!(aggregator.flush().unwrap().minute_us == start + 10 * MINUTE_US);
    }
}
