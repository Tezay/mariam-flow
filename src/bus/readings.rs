use crate::error::AppError;
use crate::sensor::{SensorDriver, SensorDriverFactory, SensorRangeStatus, SensorStatus};
use crate::state::{AppState, ReadingStatus, SensorReading};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use tracing::{debug, warn};

const MIN_DISTANCE_MM: u16 = 40;
const MAX_DISTANCE_MM: u16 = 4000;

/// Read distances for ready sensors using a dedicated sync cycle and persist into shared state.
pub fn read_and_store_distances<F>(
    factory: &mut F,
    state: &Arc<RwLock<AppState>>,
) -> Result<Vec<SensorReading>, AppError>
where
    F: SensorDriverFactory,
{
    let sensors = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.sensors().to_vec()
    };

    let mut readings = Vec::new();
    for sensor in sensors {
        if !matches!(sensor.status, SensorStatus::Ready) {
            debug!(sensor_id = sensor.sensor_id, "Skipping sensor not ready");
            continue;
        }

        let mut driver = match factory.create_for_address(sensor.i2c_address) {
            Ok(driver) => driver,
            Err(err) => {
                warn!(
                    sensor_id = sensor.sensor_id,
                    address = format_args!("{:#04x}", sensor.i2c_address),
                    error = %err,
                    "Failed to create sensor driver for reading"
                );
                readings.push(SensorReading {
                    sensor_id: sensor.sensor_id,
                    distance_mm: 0,
                    timestamp: SystemTime::now(),
                    status: ReadingStatus::Error {
                        reason: format!("driver create failed: {err}"),
                    },
                });
                continue;
            }
        };

        let measurement = match driver.read_distance() {
            Ok(measurement) => measurement,
            Err(err) => {
                warn!(
                    sensor_id = sensor.sensor_id,
                    address = format_args!("{:#04x}", sensor.i2c_address),
                    error = %err,
                    "Failed to read distance"
                );
                readings.push(SensorReading {
                    sensor_id: sensor.sensor_id,
                    distance_mm: 0,
                    timestamp: SystemTime::now(),
                    status: ReadingStatus::Error {
                        reason: format!("read failed: {err}"),
                    },
                });
                continue;
            }
        };

        let status = validate_measurement(measurement.distance_mm, measurement.range_status);
        if let ReadingStatus::Error { ref reason } = status {
            warn!(
                sensor_id = sensor.sensor_id,
                address = format_args!("{:#04x}", sensor.i2c_address),
                distance_mm = measurement.distance_mm,
                range_status = format_args!("{:?}", measurement.range_status),
                error = reason,
                "Invalid distance reading"
            );
        }

        readings.push(SensorReading {
            sensor_id: sensor.sensor_id,
            distance_mm: measurement.distance_mm,
            timestamp: SystemTime::now(),
            status,
        });
    }

    let mut guard = state.write().map_err(|_| AppError::StateLock)?;
    guard.set_readings(readings.clone())?;
    Ok(readings)
}

/// Spawn a dedicated sync thread that continuously refreshes distance readings.
pub fn spawn_reading_thread<F>(
    mut factory: F,
    state: Arc<RwLock<AppState>>,
    interval: Duration,
    stop: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()>
where
    F: SensorDriverFactory + Send + 'static,
{
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            if let Err(err) = read_and_store_distances(&mut factory, &state) {
                warn!(error = %err, "Distance read cycle failed");
            }
            std::thread::sleep(interval);
        }
    })
}

fn validate_measurement(distance_mm: u16, range_status: SensorRangeStatus) -> ReadingStatus {
    if !range_status.is_valid() {
        return ReadingStatus::Error {
            reason: format!("range status not valid: {range_status:?}"),
        };
    }

    if !(MIN_DISTANCE_MM..=MAX_DISTANCE_MM).contains(&distance_mm) {
        return ReadingStatus::Error {
            reason: format!(
                "distance out of range: {distance_mm}mm (expected {MIN_DISTANCE_MM}-{MAX_DISTANCE_MM})"
            ),
        };
    }

    ReadingStatus::Ok { range_status }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::mock::{MockSensorBehavior, MockSensorFactory};
    use crate::sensor::{SensorInfo, SensorStatus};
    use std::time::UNIX_EPOCH;

    #[test]
    fn read_cycle_records_valid_and_invalid_readings() -> Result<(), AppError> {
        let behaviors = vec![
            MockSensorBehavior::with_reading(120, SensorRangeStatus::Valid),
            MockSensorBehavior::with_reading(20, SensorRangeStatus::Valid),
            MockSensorBehavior::with_reading(200, SensorRangeStatus::SignalFailure),
        ];
        let mut factory = MockSensorFactory::new(behaviors);

        let state = Arc::new(RwLock::new(AppState::new()));
        let _sensor_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_sensors()
        };
        let _reading_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_readings()
        };
        {
            let mut guard = state.write().map_err(|_| AppError::StateLock)?;
            guard.set_sensors(vec![
                SensorInfo {
                    sensor_id: 1,
                    xshut_pin: 17,
                    i2c_address: 0x30,
                    status: SensorStatus::Ready,
                },
                SensorInfo {
                    sensor_id: 2,
                    xshut_pin: 27,
                    i2c_address: 0x31,
                    status: SensorStatus::Ready,
                },
                SensorInfo {
                    sensor_id: 3,
                    xshut_pin: 22,
                    i2c_address: 0x32,
                    status: SensorStatus::Ready,
                },
            ])?;
        }

        let readings = read_and_store_distances(&mut factory, &state)?;

        assert_eq!(readings.len(), 3);
        assert_eq!(readings[0].sensor_id, 1);
        assert!(matches!(readings[0].status, ReadingStatus::Ok { .. }));
        assert_eq!(readings[0].distance_mm, 120);

        assert_eq!(readings[1].sensor_id, 2);
        match &readings[1].status {
            ReadingStatus::Error { reason } => {
                assert!(reason.contains("distance out of range"));
            }
            _ => panic!("expected error status"),
        }

        assert_eq!(readings[2].sensor_id, 3);
        match &readings[2].status {
            ReadingStatus::Error { reason } => {
                assert!(reason.contains("range status not valid"));
            }
            _ => panic!("expected error status"),
        }

        let guard = state.read().map_err(|_| AppError::StateLock)?;
        assert_eq!(guard.readings().len(), 3);
        assert!(guard
            .readings()
            .iter()
            .all(|reading| reading.timestamp >= UNIX_EPOCH));

        Ok(())
    }

    #[test]
    fn read_cycle_continues_on_driver_errors() -> Result<(), AppError> {
        let behaviors = vec![
            MockSensorBehavior::fail_create(),
            MockSensorBehavior::with_reading(250, SensorRangeStatus::Valid),
            MockSensorBehavior::fail_read_distance(),
        ];
        let mut factory = MockSensorFactory::new(behaviors);

        let state = Arc::new(RwLock::new(AppState::new()));
        let _sensor_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_sensors()
        };
        let _reading_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_readings()
        };
        {
            let mut guard = state.write().map_err(|_| AppError::StateLock)?;
            guard.set_sensors(vec![
                SensorInfo {
                    sensor_id: 1,
                    xshut_pin: 17,
                    i2c_address: 0x30,
                    status: SensorStatus::Ready,
                },
                SensorInfo {
                    sensor_id: 2,
                    xshut_pin: 27,
                    i2c_address: 0x31,
                    status: SensorStatus::Ready,
                },
                SensorInfo {
                    sensor_id: 3,
                    xshut_pin: 22,
                    i2c_address: 0x32,
                    status: SensorStatus::Ready,
                },
            ])?;
        }

        let readings = read_and_store_distances(&mut factory, &state)?;

        assert_eq!(readings.len(), 3);
        match &readings[0].status {
            ReadingStatus::Error { reason } => {
                assert!(reason.contains("driver create failed"));
            }
            _ => panic!("expected error status"),
        }
        assert!(matches!(readings[1].status, ReadingStatus::Ok { .. }));
        match &readings[2].status {
            ReadingStatus::Error { reason } => {
                assert!(reason.contains("read failed"));
            }
            _ => panic!("expected error status"),
        }

        Ok(())
    }
}
