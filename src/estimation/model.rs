//! Estimation model trait for extensible wait time estimation.
//!
//! This module defines the `EstimationModel` trait that all estimation models must implement.
//! Models are selected via `calibration.json` and loaded at startup.

use crate::state::{OccupancyReading, SensorReading, WaitTimeEstimate};
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

/// Trait for estimation models that compute occupancy and wait time.
///
/// Implement this trait to add new estimation models. The model is selected
/// via the `model` field in `calibration.json`.
pub trait EstimationModel: Send + Sync + std::fmt::Debug {
    /// Compute occupancy percentage from sensor readings.
    fn compute_occupancy(
        &self,
        readings: &[SensorReading],
        timestamp: SystemTime,
    ) -> OccupancyReading;

    /// Compute estimated wait time from occupancy reading.
    fn compute_wait_time(
        &self,
        occupancy: &OccupancyReading,
        timestamp: SystemTime,
    ) -> WaitTimeEstimate;

    /// Returns the occupancy configuration for this model.
    fn occupancy_config(&self) -> &OccupancyConfig;
}
