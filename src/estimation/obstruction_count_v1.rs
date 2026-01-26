//! Simple obstruction-count estimation model.
//!
//! Formula: wait_time = base_minutes + per_obstruction_minutes * obstructed_count

use crate::estimation::model::{EstimationModel, OccupancyConfig};
use crate::state::{SensorObstruction, WaitTimeErrorCode, WaitTimeEstimate, WaitTimeStatus};
use serde::Deserialize;
use std::time::SystemTime;

/// Parameters for the obstruction-count model.
#[derive(Debug, Clone, Deserialize)]
pub struct ObstructionCountParams {
    pub base_minutes: f64,
    pub per_obstruction_minutes: f64,
    pub min_wait_minutes: Option<u32>,
    pub max_wait_minutes: Option<u32>,
}

impl Default for ObstructionCountParams {
    fn default() -> Self {
        Self {
            base_minutes: 0.0,
            per_obstruction_minutes: 2.0,
            min_wait_minutes: None,
            max_wait_minutes: None,
        }
    }
}

/// Estimation model that scales with the number of obstructed sensors.
#[derive(Debug)]
pub struct ObstructionCountModel {
    pub params: ObstructionCountParams,
    pub occupancy_config: OccupancyConfig,
}

impl ObstructionCountModel {
    pub fn new(params: ObstructionCountParams, occupancy_config: OccupancyConfig) -> Self {
        Self {
            params,
            occupancy_config,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(ObstructionCountParams::default(), OccupancyConfig::default())
    }
}

impl EstimationModel for ObstructionCountModel {
    fn compute_wait_time(
        &self,
        obstructions: &[SensorObstruction],
        timestamp: SystemTime,
    ) -> WaitTimeEstimate {
        let mut valid_count = 0u32;
        let mut obstructed_count = 0u32;
        let mut error_count = 0u32;

        for obstruction in obstructions {
            match obstruction.obstructed {
                Some(true) => {
                    valid_count += 1;
                    obstructed_count += 1;
                }
                Some(false) => {
                    valid_count += 1;
                }
                None => {
                    error_count += 1;
                }
            }
        }

        if valid_count == 0 {
            return WaitTimeEstimate {
                wait_time_minutes: None,
                timestamp,
                status: WaitTimeStatus::Degraded,
                error_code: Some(WaitTimeErrorCode::NoData),
            };
        }

        let mut wait_time = self.params.base_minutes
            + (obstructed_count as f64 * self.params.per_obstruction_minutes);

        if let Some(min) = self.params.min_wait_minutes {
            wait_time = wait_time.max(min as f64);
        }
        if let Some(max) = self.params.max_wait_minutes {
            wait_time = wait_time.min(max as f64);
        }

        let status = if error_count > 0 {
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
    fn wait_time_scales_with_obstructions() {
        let model = ObstructionCountModel::new(
            ObstructionCountParams {
                base_minutes: 2.0,
                per_obstruction_minutes: 3.0,
                min_wait_minutes: None,
                max_wait_minutes: None,
            },
            OccupancyConfig::default(),
        );

        let obstructions = vec![
            obstruction(1, Some(true)),
            obstruction(2, Some(false)),
            obstruction(3, Some(true)),
        ];

        let estimate = model.compute_wait_time(&obstructions, UNIX_EPOCH);
        assert_eq!(estimate.wait_time_minutes, Some(8.0));
        assert_eq!(estimate.status, WaitTimeStatus::Ok);
    }

    #[test]
    fn no_data_returns_degraded() {
        let model = ObstructionCountModel::with_defaults();
        let obstructions = vec![obstruction(1, None), obstruction(2, None)];

        let estimate = model.compute_wait_time(&obstructions, UNIX_EPOCH);
        assert_eq!(estimate.wait_time_minutes, None);
        assert_eq!(estimate.status, WaitTimeStatus::Degraded);
        assert_eq!(estimate.error_code, Some(WaitTimeErrorCode::NoData));
    }
}
