use crate::bus::readings::read_and_store_distances;
use crate::error::AppError;
use crate::sensor::SensorDriverFactory;
use crate::state::{
    AppState, OccupancyReading, OccupancyStatus, WaitTimeErrorCode, WaitTimeEstimate, WaitTimeStatus,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};
use tracing::warn;

pub mod linear;

pub const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

pub fn update_occupancy_from_readings(
    state: &Arc<RwLock<AppState>>,
) -> Result<OccupancyReading, AppError> {
    update_occupancy_from_readings_at(state, SystemTime::now())
}

fn update_occupancy_from_readings_at(
    state: &Arc<RwLock<AppState>>,
    timestamp: SystemTime,
) -> Result<OccupancyReading, AppError> {
    let readings = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.readings().to_vec()
    };

    let occupancy = linear::compute_occupancy(&readings, timestamp);
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
) -> Result<WaitTimeEstimate, AppError> {
    update_wait_time_from_occupancy_at(state, SystemTime::now())
}

fn update_wait_time_from_occupancy_at(
    state: &Arc<RwLock<AppState>>,
    timestamp: SystemTime,
) -> Result<WaitTimeEstimate, AppError> {
    let occupancy = {
        let guard = state.read().map_err(|_| AppError::StateLock)?;
        guard.occupancy().cloned()
    };

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

pub fn run_refresh_cycle<F>(
    factory: &mut F,
    state: &Arc<RwLock<AppState>>,
) -> Result<(OccupancyReading, WaitTimeEstimate), AppError>
where
    F: SensorDriverFactory,
{
    read_and_store_distances(factory, state)?;
    let cycle_timestamp = SystemTime::now();
    let occupancy = update_occupancy_from_readings_at(state, cycle_timestamp)?;
    let wait_time = update_wait_time_from_occupancy_at(state, cycle_timestamp)?;

    Ok((occupancy, wait_time))
}

pub fn spawn_refresh_thread<F>(
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
            let cycle_start = Instant::now();
            if let Err(err) = run_refresh_cycle(&mut factory, &state) {
                warn!(error = %err, "Refresh cycle failed");
            }
            let elapsed = cycle_start.elapsed();
            if elapsed > interval {
                warn!(
                    elapsed_ms = elapsed.as_millis(),
                    interval_ms = interval.as_millis(),
                    "Refresh cycle exceeded interval"
                );
                continue;
            }
            sleep_with_stop(&stop, interval - elapsed);
        }
    })
}

fn sleep_with_stop(stop: &AtomicBool, duration: Duration) {
    let deadline = Instant::now() + duration;
    let max_chunk = Duration::from_millis(100);

    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }

        let remaining = deadline - now;
        let sleep_for = remaining.min(max_chunk);
        std::thread::sleep(sleep_for);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::mock::{MockSensorBehavior, MockSensorFactory};
    use crate::sensor::{SensorInfo, SensorRangeStatus, SensorStatus};
    use std::sync::{Arc, RwLock};
    use std::time::UNIX_EPOCH;

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

    #[test]
    fn refresh_cycle_updates_pipeline_state() -> Result<(), AppError> {
        let behaviors = vec![
            MockSensorBehavior::with_reading(800, SensorRangeStatus::Valid),
            MockSensorBehavior::with_reading(1800, SensorRangeStatus::Valid),
        ];
        let mut factory = MockSensorFactory::new(behaviors);

        let state = Arc::new(RwLock::new(AppState::new()));
        let (_sensor_rx, _reading_rx, _occupancy_rx, _wait_time_rx) = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            (
                guard.subscribe_sensors(),
                guard.subscribe_readings(),
                guard.subscribe_occupancy(),
                guard.subscribe_wait_time(),
            )
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
            ])?;
        }

        let (occupancy, wait_time) = run_refresh_cycle(&mut factory, &state)?;

        assert_eq!(occupancy.timestamp, wait_time.timestamp);
        assert!(occupancy.timestamp >= UNIX_EPOCH);

        let guard = state.read().map_err(|_| AppError::StateLock)?;
        let max_reading_ts = guard
            .readings()
            .iter()
            .map(|reading| reading.timestamp)
            .max()
            .expect("expected readings in state");
        assert_eq!(guard.readings().len(), 2);
        assert!(max_reading_ts <= occupancy.timestamp);
        assert_eq!(guard.occupancy(), Some(&occupancy));
        assert_eq!(guard.wait_time(), Some(&wait_time));

        Ok(())
    }

    #[test]
    fn default_refresh_interval_is_five_seconds() {
        assert_eq!(DEFAULT_REFRESH_INTERVAL, Duration::from_secs(5));
    }
}
