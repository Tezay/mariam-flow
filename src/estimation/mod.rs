use crate::bus::readings::read_and_store_distances;
use crate::error::AppError;
use crate::sensor::SensorDriverFactory;
use crate::state::{
    AppState, OccupancyReading, OccupancyStatus, WaitTimeErrorCode, WaitTimeEstimate,
    WaitTimeStatus,
};
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tracing::{info, warn};

pub mod linear_v1;
pub mod linear_v2;
pub mod model;

use linear_v1::{LinearV1Model, LinearV1Params};
use linear_v2::{LinearV2Model, LinearV2Params};
use model::{EstimationModel, OccupancyConfig};

pub const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

// Model Factory
pub fn create_model(
    config: &CalibrationFile,
) -> Result<Box<dyn EstimationModel>, CalibrationError> {
    let occupancy_config = OccupancyConfig {
        threshold_mm: config.occupancy_threshold_mm.unwrap_or(1200),
        sensor_min_mm: config.sensor_min_mm.unwrap_or(40),
        sensor_max_mm: config.sensor_max_mm.unwrap_or(4000),
    };

    match config.model.as_str() {
        "linear_v1" => {
            let params: LinearV1Params = serde_json::from_value(config.params.clone())?;
            Ok(Box::new(LinearV1Model::new(params, occupancy_config)))
        }
        "linear_v2" => {
            let params: LinearV2Params = serde_json::from_value(config.params.clone())?;
            Ok(Box::new(LinearV2Model::new(params, occupancy_config)))
        }
        other => Err(CalibrationError::Invalid(format!("unknown model: {other}"))),
    }
}

pub fn update_occupancy_from_readings(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
) -> Result<OccupancyReading, AppError> {
    update_occupancy_from_readings_at(state, model, SystemTime::now())
}

fn update_occupancy_from_readings_at(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
    timestamp: SystemTime,
) -> Result<OccupancyReading, AppError> {
    let readings = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.readings().to_vec()
    };

    let occupancy = model.compute_occupancy(&readings, timestamp);

    if matches!(occupancy.status, OccupancyStatus::NoData) {
        warn!("No valid sensor readings available for occupancy calculation");
    } else if matches!(occupancy.status, OccupancyStatus::Degraded) {
        warn!("Occupancy calculation degraded due to sensor errors");
    }

    let mut guard = state.write().map_err(|_| AppError::StateLock)?;
    guard.set_occupancy(occupancy.clone())?;

    Ok(occupancy)
}

pub fn update_wait_time_from_occupancy(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
) -> Result<WaitTimeEstimate, AppError> {
    update_wait_time_from_occupancy_at(state, model, SystemTime::now())
}

fn update_wait_time_from_occupancy_at(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
    timestamp: SystemTime,
) -> Result<WaitTimeEstimate, AppError> {
    let occupancy = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        // If occupancy is None, create a dummy one with NoData status
        guard.occupancy().cloned().unwrap_or(OccupancyReading {
            occupancy_percent: None,
            timestamp,
            status: OccupancyStatus::NoData,
        })
    };

    let wait_time = model.compute_wait_time(&occupancy, timestamp);

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

#[derive(Debug, Deserialize)]
pub struct CalibrationFile {
    pub model: String,
    pub occupancy_threshold_mm: Option<u16>,
    pub sensor_min_mm: Option<u16>,
    pub sensor_max_mm: Option<u16>,
    pub params: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum CalibrationError {
    #[error("failed to read calibration file: {0}")]
    Read(#[from] std::io::Error),
    #[error("failed to parse calibration file: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid calibration: {0}")]
    Invalid(String),
}

pub fn load_calibration_from_path(
    path: impl AsRef<Path>,
) -> Result<Box<dyn EstimationModel>, CalibrationError> {
    let contents = std::fs::read_to_string(path)?;
    let config: CalibrationFile = serde_json::from_str(&contents)?;
    create_model(&config)
}

pub fn run_refresh_cycle(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
) -> Result<(), AppError> {
    update_occupancy_from_readings(state, model)?;
    update_wait_time_from_occupancy(state, model)?;
    Ok(())
}

pub fn spawn_refresh_thread<F>(
    mut sensor_factory: F,
    state: Arc<RwLock<AppState>>,
    interval: Duration,
    stop: Arc<AtomicBool>,
    model: Arc<dyn EstimationModel>,
) -> std::thread::JoinHandle<()>
where
    F: SensorDriverFactory + Send + 'static,
{
    std::thread::spawn(move || {
        let mut sensors = {
            let guard = state.read().expect("state lock poisoned");
            guard.sensors().to_vec()
        };

        if sensors.is_empty() {
            warn!("Refresh thread started with no sensors discovered");
        }

        while !stop.load(Ordering::Relaxed) {
            let cycle_start = Instant::now();

            if let Err(e) =
                read_and_store_distances(&mut sensor_factory, &mut sensors, &state, model.as_ref())
            {
                warn!("Error reading sensors: {}", e);
            }

            if let Err(e) = run_refresh_cycle(&state, model.as_ref()) {
                warn!("Error running estimation cycle: {}", e);
            }

            sleep_with_stop(interval, &stop, cycle_start);
        }
    })
}

fn sleep_with_stop(duration: Duration, stop: &AtomicBool, start: Instant) {
    let elapsed = start.elapsed();
    if elapsed >= duration {
        return;
    }
    let remaining = duration - elapsed;
    let step = Duration::from_millis(100);
    let mut slept = Duration::ZERO;

    while slept < remaining {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(step);
        slept += step;
    }
}
