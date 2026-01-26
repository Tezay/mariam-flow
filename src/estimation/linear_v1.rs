//! Linear V1 estimation model using slope/intercept formula.
//!
//! Formula: wait_time = intercept + slope * occupancy_percent

use crate::estimation::model::{occupancy_from_obstructions, EstimationModel, OccupancyConfig};
use crate::state::{
    OccupancyStatus, SensorObstruction, WaitTimeErrorCode, WaitTimeEstimate, WaitTimeStatus,
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
    fn compute_wait_time(
        &self,
        obstructions: &[SensorObstruction],
        timestamp: SystemTime,
    ) -> WaitTimeEstimate {
        let occupancy = occupancy_from_obstructions(obstructions, timestamp);
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
    use std::time::UNIX_EPOCH;

    fn obstruction(sensor_id: u32, obstructed: Option<bool>) -> SensorObstruction {
        SensorObstruction {
            sensor_id,
            obstructed,
            timestamp: UNIX_EPOCH,
        }
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
        let obstructions = vec![obstruction(1, Some(true)), obstruction(2, Some(false))];

        let estimate = model.compute_wait_time(&obstructions, UNIX_EPOCH);

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

        let low = vec![obstruction(1, Some(false)), obstruction(2, Some(false))];
        let high = vec![obstruction(1, Some(true)), obstruction(2, Some(true))];

        let low_est = model.compute_wait_time(&low, UNIX_EPOCH);
        let high_est = model.compute_wait_time(&high, UNIX_EPOCH);

        assert_eq!(low_est.wait_time_minutes, Some(5.0));
        assert_eq!(high_est.wait_time_minutes, Some(30.0));
    }

    #[test]
    fn no_data_returns_degraded() {
        let model = LinearV1Model::with_defaults();
        let obstructions = vec![obstruction(1, None), obstruction(2, None)];
        let estimate = model.compute_wait_time(&obstructions, UNIX_EPOCH);

        assert_eq!(estimate.status, WaitTimeStatus::Degraded);
        assert_eq!(estimate.error_code, Some(WaitTimeErrorCode::NoData));
    }
}
