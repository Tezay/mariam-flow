//! Estimation model trait for extensible wait time estimation.
//!
//! This module defines the `EstimationModel` trait that all estimation models must implement.
//! Models are selected via `calibration.json` and loaded at startup.

use crate::state::{OccupancyReading, OccupancyStatus, SensorObstruction, WaitTimeEstimate};
use std::time::SystemTime;

/// Configuration for sensor occupancy detection, loaded from calibration.json.
#[derive(Debug, Clone)]
pub struct OccupancyConfig {
    /// Distance threshold (mm) below which a sensor is considered occupied.
    pub threshold_mm: u16,
    /// Minimum valid sensor reading (hardware limit).
    pub sensor_min_mm: u16,
    /// Maximum valid sensor reading (hardware limit).
    pub sensor_max_mm: u16,
}

impl Default for OccupancyConfig {
    fn default() -> Self {
        Self {
            threshold_mm: 1200,
            sensor_min_mm: 10,
            sensor_max_mm: 4000,
        }
    }
}

/// Trait for estimation models that compute wait time from per-sensor obstructions.
///
/// Implement this trait to add new estimation models. The model is selected
/// via the `model` field in `calibration.json`.
pub trait EstimationModel: Send + Sync + std::fmt::Debug {
    /// Compute estimated wait time from sensor obstruction readings.
    fn compute_wait_time(
        &self,
        obstructions: &[SensorObstruction],
        timestamp: SystemTime,
    ) -> WaitTimeEstimate;

    /// Returns the occupancy configuration for this model.
    fn occupancy_config(&self) -> &OccupancyConfig;
}

/// Helper to compute occupancy from per-sensor obstruction readings.
pub fn occupancy_from_obstructions(
    obstructions: &[SensorObstruction],
    timestamp: SystemTime,
) -> OccupancyReading {
    let mut valid_count = 0u32;
    let mut occupied_count = 0u32;
    let mut error_count = 0u32;

    for obstruction in obstructions {
        match obstruction.obstructed {
            Some(true) => {
                valid_count += 1;
                occupied_count += 1;
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
