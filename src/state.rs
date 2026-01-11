use crate::error::AppError;
use crate::sensor::SensorInfo;
use tokio::sync::watch;

#[derive(Debug)]
pub struct AppState {
    sensors: Vec<SensorInfo>,
    sensors_tx: watch::Sender<Vec<SensorInfo>>,
}

impl AppState {
    pub fn new() -> Self {
        let (sensors_tx, _sensors_rx) = watch::channel(Vec::new());
        Self {
            sensors: Vec::new(),
            sensors_tx,
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
}
