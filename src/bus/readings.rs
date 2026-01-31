use crate::error::AppError;
use crate::estimation::model::EstimationModel;
use crate::sensor::{SensorDriver, SensorDriverFactory, SensorRangeStatus, SensorStatus};
use crate::state::{AppState, ReadingStatus, SensorReading};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tracing::{debug, warn};

/// Read distances for ready sensors using a dedicated sync cycle and persist into shared state.
pub fn read_and_store_distances<F>(
    factory: &mut F,
    sensors: &mut [crate::sensor::SensorInfo],
    state: &Arc<RwLock<AppState>>,
    model: &dyn EstimationModel,
) -> Result<Vec<SensorReading>, AppError>
where
    F: SensorDriverFactory,
{
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

        let status = validate_measurement(measurement.distance_mm, measurement.range_status, model);
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

fn validate_measurement(
    distance_mm: u16,
    range_status: SensorRangeStatus,
    model: &dyn EstimationModel,
) -> ReadingStatus {
    if !range_status.is_valid() {
        return ReadingStatus::Error {
            reason: format!("range status not valid: {range_status:?}"),
        };
    }

    let config = model.occupancy_config();
    if !(config.sensor_min_mm..=config.sensor_max_mm).contains(&distance_mm) {
        return ReadingStatus::Error {
            reason: format!(
                "distance out of range: {distance_mm}mm (expected {}-{})",
                config.sensor_min_mm, config.sensor_max_mm
            ),
        };
    }

    ReadingStatus::Ok { range_status }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estimation::model::{EstimationModel, OccupancyConfig};
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
        let model = TestModel::new(OccupancyConfig {
            threshold_mm: 1000,
            sensor_min_mm: 40,
            sensor_max_mm: 4000,
        });

        let state = Arc::new(RwLock::new(AppState::new()));
        let _sensor_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_sensors()
        };
        let _reading_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_readings()
        };

        let mut sensors = vec![
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
        ];

        {
            let mut guard = state.write().map_err(|_| AppError::StateLock)?;
            guard.set_sensors(sensors.clone())?;
        }

        let readings = read_and_store_distances(&mut factory, &mut sensors, &state, &model)?;

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
        assert!(
            guard
                .readings()
                .iter()
                .all(|reading| reading.timestamp >= UNIX_EPOCH)
        );

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
        let model = TestModel::new(OccupancyConfig::default());

        let state = Arc::new(RwLock::new(AppState::new()));
        let _sensor_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_sensors()
        };
        let _reading_rx = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_readings()
        };

        let mut sensors = vec![
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
        ];

        {
            let mut guard = state.write().map_err(|_| AppError::StateLock)?;
            guard.set_sensors(sensors.clone())?;
        }

        let readings = read_and_store_distances(&mut factory, &mut sensors, &state, &model)?;

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

    #[derive(Debug)]
    struct TestModel {
        occupancy_config: OccupancyConfig,
    }

    impl TestModel {
        fn new(occupancy_config: OccupancyConfig) -> Self {
            Self { occupancy_config }
        }
    }

    impl EstimationModel for TestModel {
        fn compute_wait_time(
            &self,
            _obstructions: &[crate::state::SensorObstruction],
            timestamp: std::time::SystemTime,
        ) -> crate::state::WaitTimeEstimate {
            crate::state::WaitTimeEstimate {
                wait_time_minutes: None,
                timestamp,
                status: crate::state::WaitTimeStatus::Degraded,
                error_code: Some(crate::state::WaitTimeErrorCode::NoData),
            }
        }

        fn occupancy_config(&self) -> &OccupancyConfig {
            &self.occupancy_config
        }
    }
}
