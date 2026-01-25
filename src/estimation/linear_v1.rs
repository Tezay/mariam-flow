//! Linear V1 estimation model using slope/intercept formula.
//!
//! Formula: wait_time = intercept + slope * occupancy_percent

use crate::estimation::model::{EstimationModel, OccupancyConfig};
use crate::state::{
    OccupancyReading, OccupancyStatus, ReadingStatus, SensorReading, WaitTimeErrorCode,
    WaitTimeEstimate, WaitTimeStatus,
};
use std::time::SystemTime;

use serde::Deserialize;

/// Linear V1 model parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct LinearV1Params {
    pub slope: f64,
    pub intercept: f64,
    pub min_wait_minutes: Option<u32>,
    pub max_wait_minutes: Option<u32>,
}

impl Default for LinearV1Params {
    fn default() -> Self {
        Self {
            slope: 0.2, // 20 min at 100% occupancy
            intercept: 0.0,
            min_wait_minutes: None,
            max_wait_minutes: None,
        }
    }
}

/// Linear V1 estimation model.
///
/// Computes wait time using: `wait = intercept + slope * occupancy_percent`
#[derive(Debug)]
pub struct LinearV1Model {
    pub params: LinearV1Params,
    pub occupancy_config: OccupancyConfig,
}

impl LinearV1Model {
    pub fn new(params: LinearV1Params, occupancy_config: OccupancyConfig) -> Self {
        Self {
            params,
            occupancy_config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(LinearV1Params::default(), OccupancyConfig::default())
    }
}

impl EstimationModel for LinearV1Model {
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
        let mut wait_time = self.params.intercept + self.params.slope * occupancy_percent;

        // Apply bounds
        if let Some(min) = self.params.min_wait_minutes {
            wait_time = wait_time.max(min as f64);
        }
        if let Some(max) = self.params.max_wait_minutes {
            wait_time = wait_time.min(max as f64);
        }

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
    use crate::sensor::SensorRangeStatus;
    use std::time::UNIX_EPOCH;

    fn ok_reading(sensor_id: u32, distance_mm: u16) -> SensorReading {
        SensorReading {
            sensor_id,
            distance_mm,
            timestamp: UNIX_EPOCH,
            status: ReadingStatus::Ok {
                range_status: SensorRangeStatus::Valid,
            },
        }
    }

    fn error_reading(sensor_id: u32) -> SensorReading {
        SensorReading {
            sensor_id,
            distance_mm: 0,
            timestamp: UNIX_EPOCH,
            status: ReadingStatus::Error {
                reason: "read failed".to_string(),
            },
        }
    }

    #[test]
    fn occupancy_uses_configured_threshold() {
        let model = LinearV1Model::new(
            LinearV1Params::default(),
            OccupancyConfig {
                threshold_mm: 1000,
                ..OccupancyConfig::default()
            },
        );
        let readings = vec![ok_reading(1, 999), ok_reading(2, 1001)];

        let occupancy = model.compute_occupancy(&readings, UNIX_EPOCH);

        assert_eq!(occupancy.occupancy_percent, Some(50.0));
    }

    #[test]
    fn wait_time_uses_slope_intercept() {
        let model = LinearV1Model::new(
            LinearV1Params {
                slope: 0.5,
                intercept: 5.0,
                ..LinearV1Params::default()
            },
            OccupancyConfig::default(),
        );
        let occupancy = OccupancyReading {
            occupancy_percent: Some(50.0),
            timestamp: UNIX_EPOCH,
            status: OccupancyStatus::Ok,
        };

        let estimate = model.compute_wait_time(&occupancy, UNIX_EPOCH);

        // 5.0 + 0.5 * 50 = 30.0
        assert_eq!(estimate.wait_time_minutes, Some(30.0));
    }

    #[test]
    fn wait_time_respects_bounds() {
        let model = LinearV1Model::new(
            LinearV1Params {
                slope: 1.0,
                intercept: 0.0,
                min_wait_minutes: Some(5),
                max_wait_minutes: Some(30),
            },
            OccupancyConfig::default(),
        );

        let low = OccupancyReading {
            occupancy_percent: Some(0.0),
            timestamp: UNIX_EPOCH,
            status: OccupancyStatus::Ok,
        };
        let high = OccupancyReading {
            occupancy_percent: Some(100.0),
            timestamp: UNIX_EPOCH,
            status: OccupancyStatus::Ok,
        };

        let low_est = model.compute_wait_time(&low, UNIX_EPOCH);
        let high_est = model.compute_wait_time(&high, UNIX_EPOCH);

        assert_eq!(low_est.wait_time_minutes, Some(5.0));
        assert_eq!(high_est.wait_time_minutes, Some(30.0));
    }

    #[test]
    fn no_data_returns_degraded() {
        let model = LinearV1Model::with_defaults();
        let readings = vec![error_reading(1), error_reading(2)];

        let occupancy = model.compute_occupancy(&readings, UNIX_EPOCH);
        let estimate = model.compute_wait_time(&occupancy, UNIX_EPOCH);

        assert_eq!(occupancy.status, OccupancyStatus::NoData);
        assert_eq!(estimate.status, WaitTimeStatus::Degraded);
        assert_eq!(estimate.error_code, Some(WaitTimeErrorCode::NoData));
    }
}
