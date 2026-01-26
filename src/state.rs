use crate::error::AppError;
use crate::estimation::linear_v1::LinearV1Model;
use crate::estimation::model::EstimationModel;
use crate::sensor::{SensorId, SensorInfo, SensorRangeStatus};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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
pub struct SensorObstruction {
    pub sensor_id: SensorId,
    pub obstructed: Option<bool>,
    pub timestamp: SystemTime,
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
    obstructions: Vec<SensorObstruction>,
    obstructions_tx: watch::Sender<Vec<SensorObstruction>>,
    wait_time: Option<WaitTimeEstimate>,
    wait_time_tx: watch::Sender<Option<WaitTimeEstimate>>,
    calibration: Option<CalibrationParams>,
    model: Arc<dyn EstimationModel>,
}

impl AppState {
    pub fn new() -> Self {
        let (sensors_tx, _sensors_rx) = watch::channel(Vec::new());
        let (readings_tx, _readings_rx) = watch::channel(Vec::new());
        let (obstructions_tx, _obstructions_rx) = watch::channel(Vec::new());
        let (wait_time_tx, _wait_time_rx) = watch::channel(None);
        let model = Arc::new(LinearV1Model::with_defaults());
        Self {
            sensors: Vec::new(),
            sensors_tx,
            readings: Vec::new(),
            readings_tx,
            obstructions: Vec::new(),
            obstructions_tx,
            wait_time: None,
            wait_time_tx,
            calibration: None,
            model,
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
        // Send is best-effort - no subscribers is OK, local state is still updated
        let _ = self.sensors_tx.send(sensors);
        Ok(())
    }

    pub fn readings(&self) -> &[SensorReading] {
        &self.readings
    }

    pub fn subscribe_readings(&self) -> watch::Receiver<Vec<SensorReading>> {
        self.readings_tx.subscribe()
    }

    pub fn set_readings(&mut self, readings: Vec<SensorReading>) -> Result<(), AppError> {
        self.readings = readings.clone();
        // Send is best-effort - no subscribers is OK, local state is still updated
        let _ = self.readings_tx.send(readings);
        Ok(())
    }

    pub fn obstructions(&self) -> &[SensorObstruction] {
        &self.obstructions
    }

    pub fn subscribe_obstructions(&self) -> watch::Receiver<Vec<SensorObstruction>> {
        self.obstructions_tx.subscribe()
    }

    pub fn set_obstructions(
        &mut self,
        obstructions: Vec<SensorObstruction>,
    ) -> Result<(), AppError> {
        self.obstructions = obstructions.clone();
        // Send is best-effort - no subscribers is OK, local state is still updated
        let _ = self.obstructions_tx.send(obstructions);
        Ok(())
    }

    pub fn wait_time(&self) -> Option<&WaitTimeEstimate> {
        self.wait_time.as_ref()
    }

    pub fn subscribe_wait_time(&self) -> watch::Receiver<Option<WaitTimeEstimate>> {
        self.wait_time_tx.subscribe()
    }

    pub fn set_wait_time(&mut self, wait_time: WaitTimeEstimate) -> Result<(), AppError> {
        self.wait_time = Some(wait_time.clone());
        // Send is best-effort - no subscribers is OK, local state is still updated
        let _ = self.wait_time_tx.send(Some(wait_time));
        Ok(())
    }

    pub fn calibration(&self) -> Option<&CalibrationParams> {
        self.calibration.as_ref()
    }

    pub fn set_calibration(&mut self, calibration: Option<CalibrationParams>) {
        self.calibration = calibration;
    }

    pub fn set_model(&mut self, model: Arc<dyn EstimationModel>) {
        self.model = model;
    }

    pub fn model(&self) -> &Arc<dyn EstimationModel> {
        &self.model
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
