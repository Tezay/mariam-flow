use crate::error::AppError;
use crate::sensor::{SensorId, SensorInfo, SensorRangeStatus};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadingStatus {
    Ok { range_status: SensorRangeStatus },
    Error { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensorReading {
    pub sensor_id: SensorId,
    pub distance_mm: u16,
    pub timestamp: SystemTime,
    pub status: ReadingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OccupancyStatus {
    Ok,
    NoData,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaitTimeStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WaitTimeErrorCode {
    NoData,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationParams {
    pub slope: f64,
    pub intercept: f64,
    pub min_wait_minutes: Option<u32>,
    pub max_wait_minutes: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OccupancyReading {
    pub occupancy_percent: Option<f64>,
    pub timestamp: SystemTime,
    pub status: OccupancyStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitTimeEstimate {
    pub wait_time_minutes: Option<f64>,
    pub timestamp: SystemTime,
    pub status: WaitTimeStatus,
    pub error_code: Option<WaitTimeErrorCode>,
}

#[derive(Debug)]
pub struct AppState {
    sensors: Vec<SensorInfo>,
    sensors_tx: watch::Sender<Vec<SensorInfo>>,
    readings: Vec<SensorReading>,
    readings_tx: watch::Sender<Vec<SensorReading>>,
    occupancy: Option<OccupancyReading>,
    occupancy_tx: watch::Sender<Option<OccupancyReading>>,
    wait_time: Option<WaitTimeEstimate>,
    wait_time_tx: watch::Sender<Option<WaitTimeEstimate>>,
    calibration: Option<CalibrationParams>,
}

impl AppState {
    pub fn new() -> Self {
        let (sensors_tx, _sensors_rx) = watch::channel(Vec::new());
        let (readings_tx, _readings_rx) = watch::channel(Vec::new());
        let (occupancy_tx, _occupancy_rx) = watch::channel(None);
        let (wait_time_tx, _wait_time_rx) = watch::channel(None);
        Self {
            sensors: Vec::new(),
            sensors_tx,
            readings: Vec::new(),
            readings_tx,
            occupancy: None,
            occupancy_tx,
            wait_time: None,
            wait_time_tx,
            calibration: None,
        }
    }

    pub fn sensors(&self) -> &[SensorInfo] {
        &self.sensors
    }

    pub fn subscribe_sensors(&self) -> watch::Receiver<Vec<SensorInfo>> {
        self.sensors_tx.subscribe()
    }

    pub fn set_sensors(&mut self, sensors: Vec<SensorInfo>) -> Result<(), AppError> {
        self.sensors = sensors.clone();
        self.sensors_tx
            .send(sensors)
            .map_err(|_| AppError::WatchSend)
    }

    pub fn readings(&self) -> &[SensorReading] {
        &self.readings
    }

    pub fn subscribe_readings(&self) -> watch::Receiver<Vec<SensorReading>> {
        self.readings_tx.subscribe()
    }

    pub fn set_readings(&mut self, readings: Vec<SensorReading>) -> Result<(), AppError> {
        self.readings = readings.clone();
        self.readings_tx
            .send(readings)
            .map_err(|_| AppError::WatchSend)
    }

    pub fn occupancy(&self) -> Option<&OccupancyReading> {
        self.occupancy.as_ref()
    }

    pub fn subscribe_occupancy(&self) -> watch::Receiver<Option<OccupancyReading>> {
        self.occupancy_tx.subscribe()
    }

    pub fn set_occupancy(&mut self, occupancy: OccupancyReading) -> Result<(), AppError> {
        self.occupancy = Some(occupancy.clone());
        self.occupancy_tx
            .send(Some(occupancy))
            .map_err(|_| AppError::WatchSend)
    }

    pub fn wait_time(&self) -> Option<&WaitTimeEstimate> {
        self.wait_time.as_ref()
    }

    pub fn subscribe_wait_time(&self) -> watch::Receiver<Option<WaitTimeEstimate>> {
        self.wait_time_tx.subscribe()
    }

    pub fn set_wait_time(&mut self, wait_time: WaitTimeEstimate) -> Result<(), AppError> {
        self.wait_time = Some(wait_time.clone());
        self.wait_time_tx
            .send(Some(wait_time))
            .map_err(|_| AppError::WatchSend)
    }

    pub fn calibration(&self) -> Option<&CalibrationParams> {
        self.calibration.as_ref()
    }

    pub fn set_calibration(&mut self, calibration: Option<CalibrationParams>) {
        self.calibration = calibration;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn set_readings_updates_state_and_watch() {
        let mut state = AppState::new();
        let receiver = state.subscribe_readings();
        let reading = SensorReading {
            sensor_id: 1,
            distance_mm: 420,
            timestamp: UNIX_EPOCH + Duration::from_secs(1),
            status: ReadingStatus::Ok {
                range_status: SensorRangeStatus::Valid,
            },
        };

        assert!(state.set_readings(vec![reading.clone()]).is_ok());

        assert_eq!(state.readings(), &[reading.clone()]);
        assert_eq!(receiver.borrow().as_slice(), &[reading]);
    }

    #[test]
    fn set_readings_accepts_error_status() {
        let mut state = AppState::new();
        let _receiver = state.subscribe_readings();
        let reading = SensorReading {
            sensor_id: 2,
            distance_mm: 0,
            timestamp: UNIX_EPOCH,
            status: ReadingStatus::Error {
                reason: "range out of bounds".to_string(),
            },
        };

        assert!(state.set_readings(vec![reading.clone()]).is_ok());

        assert_eq!(state.readings(), &[reading]);
    }

    #[test]
    fn set_wait_time_updates_state_and_watch() {
        let mut state = AppState::new();
        let receiver = state.subscribe_wait_time();
        let estimate = WaitTimeEstimate {
            wait_time_minutes: Some(12.0),
            timestamp: UNIX_EPOCH + Duration::from_secs(10),
            status: WaitTimeStatus::Ok,
            error_code: None,
        };

        assert!(state.set_wait_time(estimate.clone()).is_ok());

        assert_eq!(state.wait_time(), Some(&estimate));
        assert_eq!(*receiver.borrow(), Some(estimate));
    }
}
