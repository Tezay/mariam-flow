//! Reconstruction of edge timestamps from node-local clocks.

use flow_core::TimestampUs;

/// Rebuilds monotonic edge timestamps from a node's wrapping local clock.
///
/// Sensing nodes report a `local_timestamp` from a 32-bit microsecond
/// clock that wraps roughly every 71 minutes and is synchronized to
/// nothing. When replaying a capture, the real inter-frame timing must be
/// preserved (features are computed over sliding time windows), so edge
/// timestamps are reconstructed as `base + (local − first_local)`,
/// extending the local clock across wrap-arounds.
///
/// A node reboot mid-capture is indistinguishable from a wrap given a
/// single clock stream and appears as a forward time jump; the stream
/// statistics' sequence resets ([`StreamStats::seq_resets`]) reveal it.
///
/// [`StreamStats::seq_resets`]: crate::StreamStats
#[derive(Debug)]
pub struct Timeline {
    base_us: TimestampUs,
    wraps: u64,
    prev_local: Option<u32>,
    first_extended: Option<u64>,
}

impl Timeline {
    /// Creates a timeline anchored at `base_us` — the Unix timestamp (µs)
    /// assigned to the first frame.
    #[must_use]
    pub fn new(base_us: TimestampUs) -> Self {
        Self {
            base_us,
            wraps: 0,
            prev_local: None,
            first_extended: None,
        }
    }

    /// Assigns the edge timestamp for a frame carrying `local_us`.
    ///
    /// Calls must follow stream order: consecutive local clocks are
    /// compared to detect wrap-around. The returned sequence is
    /// monotonically non-decreasing.
    pub fn assign(&mut self, local_us: u32) -> TimestampUs {
        if let Some(prev) = self.prev_local {
            if local_us < prev {
                self.wraps += 1;
            }
        }
        self.prev_local = Some(local_us);
        let extended = (self.wraps << 32) + u64::from(local_us);
        let first = *self.first_extended.get_or_insert(extended);
        self.base_us + (extended - first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_gets_the_base_timestamp() {
        let mut timeline = Timeline::new(1_720_000_000_000_000);
        assert_eq!(timeline.assign(123_456), 1_720_000_000_000_000);
    }

    #[test]
    fn preserves_inter_frame_deltas() {
        let mut timeline = Timeline::new(1_000_000);
        assert_eq!(timeline.assign(500), 1_000_000);
        assert_eq!(timeline.assign(1_500), 1_001_000);
        assert_eq!(timeline.assign(1_500), 1_001_000);
        assert_eq!(timeline.assign(2_000), 1_001_500);
    }

    #[test]
    fn survives_clock_wrap_around() {
        let mut timeline = Timeline::new(0);
        let start = u32::MAX - 10;
        assert_eq!(timeline.assign(start), 0);
        assert_eq!(timeline.assign(u32::MAX), 10);
        // Wrapped: 5 µs past zero = 16 µs after `start`.
        assert_eq!(timeline.assign(5), 16);
    }

    #[test]
    fn survives_multiple_wraps() {
        let mut timeline = Timeline::new(0);
        timeline.assign(10);
        timeline.assign(5); // wrap 1
        timeline.assign(3); // wrap 2
        let ts = timeline.assign(3);
        assert_eq!(ts, 2 * (1 << 32) + 3 - 10);
    }

    #[test]
    fn output_is_monotonic() {
        let mut timeline = Timeline::new(42);
        let locals = [100u32, 5_000, u32::MAX, 12, 12, 90, 4_000_000];
        let mut prev = 0;
        for local in locals {
            let ts = timeline.assign(local);
            assert!(ts >= prev, "timestamp went backwards: {ts} < {prev}");
            prev = ts;
        }
    }
}
