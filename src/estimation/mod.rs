use crate::bus::readings::read_and_store_distances;
use crate::bus::xshut::{XshutController, reinitialize_sensor};
use crate::error::AppError;
use crate::sensor::SensorDriverFactory;
use crate::state::{
    AppState, ReadingStatus, SensorObstruction, SensorReading, WaitTimeErrorCode, WaitTimeEstimate,
    WaitTimeStatus,
};
use serde::Deserialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use thiserror::Error;
use tracing::warn;

pub mod linear_v1;
pub mod linear_v2;
pub mod model;
pub mod obstruction_count_v1;

use linear_v1::{LinearV1Model, LinearV1Params};
use linear_v2::{LinearV2Model, LinearV2Params};
use model::{EstimationModel, OccupancyConfig};
use obstruction_count_v1::{ObstructionCountModel, ObstructionCountParams};

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
        "obstruction_count_v1" => {
            let params: ObstructionCountParams = serde_json::from_value(config.params.clone())?;
            Ok(Box::new(ObstructionCountModel::new(
                params,
                occupancy_config,
            )))
        }
        other => Err(CalibrationError::Invalid(format!("unknown model: {other}"))),
    }
}

pub fn update_obstructions_from_readings(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
) -> Result<Vec<SensorObstruction>, AppError> {
    update_obstructions_from_readings_at(state, model, SystemTime::now())
}

fn update_obstructions_from_readings_at(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
    _timestamp: SystemTime,
) -> Result<Vec<SensorObstruction>, AppError> {
    let readings = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.readings().to_vec()
    };

    let threshold_mm = model.occupancy_config().threshold_mm;
    let (obstructions, valid_count, error_count) =
        obstructions_from_readings(&readings, threshold_mm);

    if valid_count == 0 {
        warn!("No valid sensor readings available for obstruction calculation");
    } else if error_count > 0 {
        warn!("Obstruction calculation degraded due to sensor errors");
    }

    let mut guard = state.write().map_err(|_| AppError::StateLock)?;
    guard.set_obstructions(obstructions.clone())?;

    Ok(obstructions)
}

pub fn update_wait_time_from_obstructions(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
) -> Result<WaitTimeEstimate, AppError> {
    update_wait_time_from_obstructions_at(state, model, SystemTime::now())
}

fn update_wait_time_from_obstructions_at(
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
    timestamp: SystemTime,
) -> Result<WaitTimeEstimate, AppError> {
    let obstructions = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.obstructions().to_vec()
    };

    let wait_time = model.compute_wait_time(&obstructions, timestamp);

    if matches!(wait_time.status, WaitTimeStatus::Degraded) {
        if matches!(wait_time.error_code, Some(WaitTimeErrorCode::NoData)) {
            warn!("No obstruction data available for wait time estimation");
        } else {
            warn!("Wait time estimation degraded due to obstruction errors");
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
    update_obstructions_from_readings(state, model)?;
    update_wait_time_from_obstructions(state, model)?;
    Ok(())
}

fn obstructions_from_readings(
    readings: &[SensorReading],
    threshold_mm: u16,
) -> (Vec<SensorObstruction>, u32, u32) {
    let mut valid_count = 0u32;
    let mut error_count = 0u32;
    let mut obstructions = Vec::with_capacity(readings.len());

    for reading in readings {
        let obstructed = match &reading.status {
            ReadingStatus::Ok { .. } => {
                valid_count += 1;
                Some(reading.distance_mm <= threshold_mm)
            }
            ReadingStatus::Error { .. } => {
                error_count += 1;
                None
            }
        };

        obstructions.push(SensorObstruction {
            sensor_id: reading.sensor_id,
            obstructed,
            timestamp: reading.timestamp,
        });
    }

    (obstructions, valid_count, error_count)
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
    fn obstructions_use_threshold_and_track_errors() {
        let readings = vec![ok_reading(1, 999), ok_reading(2, 1001), error_reading(3)];

        let (obstructions, valid_count, error_count) = obstructions_from_readings(&readings, 1000);

        assert_eq!(valid_count, 2);
        assert_eq!(error_count, 1);
        assert_eq!(obstructions.len(), 3);
        assert_eq!(obstructions[0].sensor_id, 1);
        assert_eq!(obstructions[0].obstructed, Some(true));
        assert_eq!(obstructions[1].sensor_id, 2);
        assert_eq!(obstructions[1].obstructed, Some(false));
        assert_eq!(obstructions[2].sensor_id, 3);
        assert_eq!(obstructions[2].obstructed, None);
    }
}

pub fn spawn_refresh_thread<F, X>(
    mut sensor_factory: F,
    mut xshut_controller: Option<X>,
    state: Arc<RwLock<AppState>>,
    interval: Duration,
    stop: Arc<AtomicBool>,
    model: Arc<dyn EstimationModel>,
) -> std::thread::JoinHandle<()>
where
    F: SensorDriverFactory + Send + 'static,
    X: XshutController + Send + 'static,
{
    std::thread::spawn(move || {
        let mut sensors = {
            let guard = state.read().expect("state lock poisoned");
            guard.sensors().to_vec()
        };

        if sensors.is_empty() {
            warn!("Refresh thread started with no sensors discovered");
        }

        // Track consecutive errors per sensor
        let mut error_counts = std::collections::HashMap::new();
        const MAX_CONSECUTIVE_ERRORS: u32 = 5;

        while !stop.load(Ordering::Relaxed) {
            let cycle_start = Instant::now();

            let readings_result =
                read_and_store_distances(&mut sensor_factory, &mut sensors, &state, model.as_ref());

            match readings_result {
                Ok(readings) => {
                    for reading in readings {
                        match reading.status {
                            ReadingStatus::Ok { .. } => {
                                error_counts.insert(reading.sensor_id, 0);
                            }
                            ReadingStatus::Error { .. } => {
                                let count = error_counts.entry(reading.sensor_id).or_insert(0);
                                *count += 1;

                                if *count >= MAX_CONSECUTIVE_ERRORS {
                                    if let Some(ref mut xshut) = xshut_controller {
                                        warn!(
                                            sensor_id = reading.sensor_id,
                                            count = *count,
                                            "Sensor exceeded error limit - triggering reset"
                                        );
                                        // Find sensor info to get pins/address
                                        if let Some(sensor_info) = sensors
                                            .iter()
                                            .find(|s| s.sensor_id == reading.sensor_id)
                                        {
                                            match reinitialize_sensor(
                                                xshut,
                                                &mut sensor_factory,
                                                sensor_info,
                                            ) {
                                                Ok(_) => {
                                                    *count = 0; // Reset error counter on success
                                                }
                                                Err(e) => {
                                                    warn!(
                                                        sensor_id = reading.sensor_id,
                                                        error = %e,
                                                        "Failed to reset sensor"
                                                    );
                                                    // Loop will retry next cycle
                                                }
                                            }
                                        }
                                    } else {
                                        if *count == MAX_CONSECUTIVE_ERRORS {
                                            warn!(
                                                sensor_id = reading.sensor_id,
                                                "Consecutive errors detected but no XSHUT controller available"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading sensors: {}", e);
                }
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
