//! Linear V2 estimation model using endpoint values.
//!
//! Formula: Interpolate between wait_time_at_empty and wait_time_at_full
//! based on occupancy percentage.

use crate::estimation::model::{occupancy_from_obstructions, EstimationModel, OccupancyConfig};
use crate::state::{
    OccupancyStatus, SensorObstruction, WaitTimeErrorCode, WaitTimeEstimate, WaitTimeStatus,
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

    fn obstruction(sensor_id: u32, obstructed: Option<bool>) -> SensorObstruction {
        SensorObstruction {
            sensor_id,
            obstructed,
            timestamp: UNIX_EPOCH,
        }
    }

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
        let empty = vec![obstruction(1, Some(false)), obstruction(2, Some(false))];
        assert_eq!(
            model.compute_wait_time(&empty, UNIX_EPOCH).wait_time_minutes,
            Some(5.0)
        );

        // 50% -> 10.0
        let half = vec![obstruction(1, Some(true)), obstruction(2, Some(false))];
        assert_eq!(
            model.compute_wait_time(&half, UNIX_EPOCH).wait_time_minutes,
            Some(10.0)
        );

        // 100% -> 15.0
        let full = vec![obstruction(1, Some(true)), obstruction(2, Some(true))];
        assert_eq!(
            model.compute_wait_time(&full, UNIX_EPOCH).wait_time_minutes,
            Some(15.0)
        );
    }
}
