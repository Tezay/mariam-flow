use crate::error::AppError;
use crate::state::{AppState, OccupancyReading, OccupancyStatus};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tracing::warn;

pub mod linear;

pub fn update_occupancy_from_readings(
    state: &Arc<RwLock<AppState>>,
) -> Result<OccupancyReading, AppError> {
    let readings = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.readings().to_vec()
    };

    let occupancy = linear::compute_occupancy(&readings, SystemTime::now());
    if matches!(occupancy.status, OccupancyStatus::NoData) {
        warn!("No valid sensor readings available for occupancy calculation");
    }

    let mut guard = state.write().map_err(|_| AppError::StateLock)?;
    guard.set_occupancy(occupancy.clone())?;

    Ok(occupancy)
}
