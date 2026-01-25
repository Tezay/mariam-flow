//! Linear V2 estimation model using endpoint values.
//!
//! Formula: Interpolate between wait_time_at_empty and wait_time_at_full
//! based on occupancy percentage.

use crate::estimation::model::{EstimationModel, OccupancyConfig};
use crate::state::{
    OccupancyReading, OccupancyStatus, ReadingStatus, SensorReading, WaitTimeErrorCode,
    WaitTimeEstimate, WaitTimeStatus,
};
use std::time::SystemTime;

use serde::Deserialize;

/// Linear V2 model parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct LinearV2Params {
    /// Wait time in minutes when occupancy is 0%
    pub wait_time_at_empty: f64,
    /// Wait time in minutes when occupancy is 100%
    pub wait_time_at_full: f64,
}

impl Default for LinearV2Params {
    fn default() -> Self {
        Self {
            wait_time_at_empty: 0.0,
            wait_time_at_full: 20.0,
        }
    }
}

/// Linear V2 estimation model.
///
/// Computes wait time by linear interpolation between empty and full states.
#[derive(Debug)]
pub struct LinearV2Model {
    pub params: LinearV2Params,
    pub occupancy_config: OccupancyConfig,
}

impl LinearV2Model {
    pub fn new(params: LinearV2Params, occupancy_config: OccupancyConfig) -> Self {
        Self {
            params,
            occupancy_config,
        }
    }
}

impl EstimationModel for LinearV2Model {
    fn compute_occupancy(
        &self,
        readings: &[SensorReading],
        timestamp: SystemTime,
    ) -> OccupancyReading {
        let mut valid_count = 0u32;
        let mut occupied_count = 0u32;
        let mut error_count = 0u32;

        for reading in readings {
            match &reading.status {
                ReadingStatus::Ok { .. } => {
                    valid_count += 1;
                    if reading.distance_mm <= self.occupancy_config.threshold_mm {
                        occupied_count += 1;
                    }
                }
                ReadingStatus::Error { .. } => {
                    error_count += 1;
                }
            }
        }

        if valid_count == 0 {
            return OccupancyReading {
                occupancy_percent: None,
                timestamp,
                status: OccupancyStatus::NoData,
            };
        }

        let occupancy_percent =
            ((occupied_count as f64 / valid_count as f64) * 100.0).clamp(0.0, 100.0);
        let status = if error_count == 0 {
            OccupancyStatus::Ok
        } else {
            OccupancyStatus::Degraded
        };

        OccupancyReading {
            occupancy_percent: Some(occupancy_percent),
            timestamp,
            status,
        }
    }

    fn compute_wait_time(
        &self,
        occupancy: &OccupancyReading,
        timestamp: SystemTime,
    ) -> WaitTimeEstimate {
        if occupancy.occupancy_percent.is_none()
            || matches!(occupancy.status, OccupancyStatus::NoData)
        {
            return WaitTimeEstimate {
                wait_time_minutes: None,
                timestamp,
                status: WaitTimeStatus::Degraded,
                error_code: Some(WaitTimeErrorCode::NoData),
            };
        }

        let occupancy_percent = occupancy.occupancy_percent.unwrap_or(0.0).clamp(0.0, 100.0);

        // Linear interpolation
        let wait_time = self.params.wait_time_at_empty
            + (occupancy_percent / 100.0)
                * (self.params.wait_time_at_full - self.params.wait_time_at_empty);

        let status = if matches!(occupancy.status, OccupancyStatus::Degraded) {
            WaitTimeStatus::Degraded
        } else {
            WaitTimeStatus::Ok
        };

        WaitTimeEstimate {
            wait_time_minutes: Some(wait_time),
            timestamp,
            status,
            error_code: None,
        }
    }

    fn occupancy_config(&self) -> &OccupancyConfig {
        &self.occupancy_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn wait_time_interpolates_correctly() {
        let model = LinearV2Model::new(
            LinearV2Params {
                wait_time_at_empty: 5.0,
                wait_time_at_full: 15.0,
            },
            OccupancyConfig::default(),
        );

        // 0% -> 5.0
        let empty = OccupancyReading {
            occupancy_percent: Some(0.0),
            timestamp: UNIX_EPOCH,
            status: OccupancyStatus::Ok,
        };
        assert_eq!(
            model
                .compute_wait_time(&empty, UNIX_EPOCH)
                .wait_time_minutes,
            Some(5.0)
        );

        // 50% -> 10.0
        let half = OccupancyReading {
            occupancy_percent: Some(50.0),
            timestamp: UNIX_EPOCH,
            status: OccupancyStatus::Ok,
        };
        assert_eq!(
            model.compute_wait_time(&half, UNIX_EPOCH).wait_time_minutes,
            Some(10.0)
        );

        // 100% -> 15.0
        let full = OccupancyReading {
            occupancy_percent: Some(100.0),
            timestamp: UNIX_EPOCH,
            status: OccupancyStatus::Ok,
        };
        assert_eq!(
            model.compute_wait_time(&full, UNIX_EPOCH).wait_time_minutes,
            Some(15.0)
        );
    }
}
