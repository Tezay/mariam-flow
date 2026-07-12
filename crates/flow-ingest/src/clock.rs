//! Edge clock: the single time source frames are stamped with.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current Unix time in microseconds.
#[must_use]
pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
        })
}
