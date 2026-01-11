use crate::error::AppError;
use crate::state::{
    AppState, OccupancyReading, OccupancyStatus, WaitTimeErrorCode, WaitTimeEstimate, WaitTimeStatus,
};
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

pub fn update_wait_time_from_occupancy(
    state: &Arc<RwLock<AppState>>,
) -> Result<WaitTimeEstimate, AppError> {
    let occupancy = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.occupancy().cloned()
    };

    let timestamp = SystemTime::now();
    let occupancy = occupancy.unwrap_or(OccupancyReading {
        occupancy_percent: None,
        timestamp,
        status: OccupancyStatus::NoData,
    });
    let wait_time = linear::compute_wait_time(&occupancy, timestamp);

    if matches!(wait_time.status, WaitTimeStatus::Degraded) {
        if matches!(wait_time.error_code, Some(WaitTimeErrorCode::NoData)) {
            warn!("No occupancy data available for wait time estimation");
        } else {
            warn!("Wait time estimation degraded due to occupancy errors");
        }
    }

    let mut guard = state.write().map_err(|_| AppError::StateLock)?;
    guard.set_wait_time(wait_time.clone())?;

    Ok(wait_time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, RwLock};

    #[test]
    fn update_wait_time_handles_missing_occupancy() {
        let state = Arc::new(RwLock::new(AppState::new()));
        let _receiver = state
            .read()
            .expect("state lock poisoned")
            .subscribe_wait_time();

        let estimate = update_wait_time_from_occupancy(&state).expect("wait time update failed");

        assert_eq!(estimate.wait_time_minutes, None);
        assert_eq!(estimate.status, WaitTimeStatus::Degraded);
        assert_eq!(estimate.error_code, Some(WaitTimeErrorCode::NoData));

        let guard = state.read().expect("state lock poisoned");
        assert_eq!(guard.wait_time(), Some(&estimate));
    }
}
