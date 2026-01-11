use mariam_flow::bus::readings::read_and_store_distances;
use mariam_flow::sensor::mock::{MockSensorBehavior, MockSensorFactory};
use mariam_flow::sensor::{SensorInfo, SensorRangeStatus, SensorStatus};
use mariam_flow::state::{AppState, ReadingStatus};
use std::sync::{Arc, RwLock};

#[test]
fn pipeline_mock_updates_state_for_all_sensors() -> Result<(), mariam_flow::error::AppError> {
    let behaviors = vec![
        MockSensorBehavior::with_reading(250, SensorRangeStatus::Valid),
        MockSensorBehavior::with_reading(20, SensorRangeStatus::Valid),
        MockSensorBehavior::fail_read_distance(),
    ];
    let mut factory = MockSensorFactory::new(behaviors);

    let state = Arc::new(RwLock::new(AppState::new()));
    let _sensor_rx = {
        let guard = state
            .read()
            .map_err(|_| mariam_flow::error::AppError::StateLock)?;
        guard.subscribe_sensors()
    };
    let _reading_rx = {
        let guard = state
            .read()
            .map_err(|_| mariam_flow::error::AppError::StateLock)?;
        guard.subscribe_readings()
    };
    {
        let mut guard = state
            .write()
            .map_err(|_| mariam_flow::error::AppError::StateLock)?;
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
    assert!(matches!(readings[0].status, ReadingStatus::Ok { .. }));
    assert!(matches!(readings[1].status, ReadingStatus::Error { .. }));
    assert!(matches!(readings[2].status, ReadingStatus::Error { .. }));

    let guard = state
        .read()
        .map_err(|_| mariam_flow::error::AppError::StateLock)?;
    assert_eq!(guard.readings().len(), 3);
    Ok(())
}
