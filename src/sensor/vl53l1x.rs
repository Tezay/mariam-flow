use crate::error::AppError;
use crate::sensor::{DistanceMeasurement, SensorDriver, SensorDriverFactory};

#[cfg(target_os = "linux")]
use crate::sensor::DEFAULT_I2C_ADDRESS_7BIT;
#[cfg(target_os = "linux")]
use crate::sensor::SensorRangeStatus;
#[cfg(target_os = "linux")]
use rppal::i2c::I2c;
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "linux")]
use vl53l1x_uld::{IOVoltage, RangeStatus as Vl53l1xRangeStatus, VL53L1X};

#[cfg(target_os = "linux")]
pub struct Vl53l1xFactory {
    io_voltage: IOVoltage,
    cache: HashMap<u8, Arc<Mutex<VL53L1X<I2c>>>>,
}

#[cfg(target_os = "linux")]
impl Vl53l1xFactory {
    pub fn new(io_voltage: IOVoltage) -> Self {
        Self {
            io_voltage,
            cache: HashMap::new(),
        }
    }
}

#[cfg(target_os = "linux")]
enum Vl53l1xInner {
    Owned(VL53L1X<I2c>),
    Shared(Arc<Mutex<VL53L1X<I2c>>>),
}

#[cfg(target_os = "linux")]
pub struct Vl53l1xDriver {
    inner: Vl53l1xInner,
    io_voltage: IOVoltage,
}

#[cfg(target_os = "linux")]
impl SensorDriver for Vl53l1xDriver {
    fn init_default(&mut self) -> Result<(), AppError> {
        match &mut self.inner {
            Vl53l1xInner::Owned(driver) => driver
                .init(self.io_voltage)
                .map_err(|err| AppError::Sensor(format!("{err:?}"))),
            Vl53l1xInner::Shared(driver) => {
                let mut guard = driver
                    .lock()
                    .map_err(|_| AppError::Sensor("sensor driver lock poisoned".to_string()))?;
                guard
                    .init(self.io_voltage)
                    .map_err(|err| AppError::Sensor(format!("{err:?}")))
            }
        }
    }

    fn set_address(&mut self, new_address: u8) -> Result<(), AppError> {
        match &mut self.inner {
            Vl53l1xInner::Owned(driver) => driver
                .set_address(new_address)
                .map_err(|err| AppError::Sensor(format!("{err:?}"))),
            Vl53l1xInner::Shared(driver) => {
                let mut guard = driver
                    .lock()
                    .map_err(|_| AppError::Sensor("sensor driver lock poisoned".to_string()))?;
                guard
                    .set_address(new_address)
                    .map_err(|err| AppError::Sensor(format!("{err:?}")))
            }
        }
    }

    fn verify(&mut self) -> Result<(), AppError> {
        match &mut self.inner {
            Vl53l1xInner::Owned(driver) => driver
                .get_sensor_id()
                .map(|_| ())
                .map_err(|err| AppError::Sensor(format!("{err:?}"))),
            Vl53l1xInner::Shared(driver) => {
                let mut guard = driver
                    .lock()
                    .map_err(|_| AppError::Sensor("sensor driver lock poisoned".to_string()))?;
                guard
                    .get_sensor_id()
                    .map(|_| ())
                    .map_err(|err| AppError::Sensor(format!("{err:?}")))
            }
        }
    }

    fn start_ranging(&mut self) -> Result<(), AppError> {
        match &mut self.inner {
            Vl53l1xInner::Owned(driver) => driver
                .start_ranging()
                .map_err(|err| AppError::Sensor(format!("{err:?}"))),
            Vl53l1xInner::Shared(driver) => {
                let mut guard = driver
                    .lock()
                    .map_err(|_| AppError::Sensor("sensor driver lock poisoned".to_string()))?;
                guard
                    .start_ranging()
                    .map_err(|err| AppError::Sensor(format!("{err:?}")))
            }
        }
    }

    fn read_distance(&mut self) -> Result<DistanceMeasurement, AppError> {
        let result = match &mut self.inner {
            Vl53l1xInner::Owned(driver) => {
                let result = driver
                    .get_result()
                    .map_err(|err| AppError::Sensor(format!("{err:?}")))?;
                // Clear interrupt to trigger next measurement
                driver
                    .clear_interrupt()
                    .map_err(|err| AppError::Sensor(format!("clear_interrupt: {err:?}")))?;
                result
            }
            Vl53l1xInner::Shared(driver) => {
                let mut guard = driver
                    .lock()
                    .map_err(|_| AppError::Sensor("sensor driver lock poisoned".to_string()))?;
                let result = guard
                    .get_result()
                    .map_err(|err| AppError::Sensor(format!("{err:?}")))?;
                // Clear interrupt to trigger next measurement
                guard
                    .clear_interrupt()
                    .map_err(|err| AppError::Sensor(format!("clear_interrupt: {err:?}")))?;
                result
            }
        };
        Ok(DistanceMeasurement {
            distance_mm: result.distance_mm,
            range_status: SensorRangeStatus::from(result.status),
        })
    }
}

#[cfg(target_os = "linux")]
impl SensorDriverFactory for Vl53l1xFactory {
    type Driver = Vl53l1xDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError> {
        let i2c = I2c::new().map_err(|err| AppError::I2c(err.to_string()))?;
        let driver = VL53L1X::new(i2c, DEFAULT_I2C_ADDRESS_7BIT);
        Ok(Vl53l1xDriver {
            inner: Vl53l1xInner::Owned(driver),
            io_voltage: self.io_voltage,
        })
    }

    fn create_for_address(&mut self, address: u8) -> Result<Self::Driver, AppError> {
        let shared = if let Some(shared) = self.cache.get(&address) {
            shared.clone()
        } else {
            let i2c = I2c::new().map_err(|err| AppError::I2c(err.to_string()))?;
            let driver = VL53L1X::new(i2c, address);
            let shared = Arc::new(Mutex::new(driver));
            self.cache.insert(address, shared.clone());
            shared
        };
        Ok(Vl53l1xDriver {
            inner: Vl53l1xInner::Shared(shared),
            io_voltage: self.io_voltage,
        })
    }
}

#[cfg(not(target_os = "linux"))]
pub struct Vl53l1xFactory;

#[cfg(not(target_os = "linux"))]
impl Vl53l1xFactory {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl Default for Vl53l1xFactory {
    fn default() -> Self {
        Self::new(IOVoltage::Volt2_8) // Default to 2.8V IO
    }
}

#[cfg(not(target_os = "linux"))]
impl Default for Vl53l1xFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_os = "linux"))]
pub struct Vl53l1xDriver;

#[cfg(not(target_os = "linux"))]
impl SensorDriver for Vl53l1xDriver {
    fn init_default(&mut self) -> Result<(), AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }

    fn set_address(&mut self, _new_address: u8) -> Result<(), AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }

    fn verify(&mut self) -> Result<(), AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }

    fn start_ranging(&mut self) -> Result<(), AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }

    fn read_distance(&mut self) -> Result<DistanceMeasurement, AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }
}

#[cfg(not(target_os = "linux"))]
impl SensorDriverFactory for Vl53l1xFactory {
    type Driver = Vl53l1xDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }

    fn create_for_address(&mut self, _address: u8) -> Result<Self::Driver, AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }
}

#[cfg(target_os = "linux")]
impl From<Vl53l1xRangeStatus> for SensorRangeStatus {
    fn from(status: Vl53l1xRangeStatus) -> Self {
        match status {
            Vl53l1xRangeStatus::Valid => Self::Valid,
            Vl53l1xRangeStatus::SigmaFailure => Self::SigmaFailure,
            Vl53l1xRangeStatus::SignalFailure => Self::SignalFailure,
            Vl53l1xRangeStatus::MinRangeClipped => Self::MinRangeClipped,
            Vl53l1xRangeStatus::OutOfBounds => Self::OutOfBounds,
            Vl53l1xRangeStatus::HardwareFailure => Self::HardwareFailure,
            Vl53l1xRangeStatus::WrapCheckFail => Self::WrapCheckFail,
            Vl53l1xRangeStatus::Wraparound => Self::Wraparound,
            Vl53l1xRangeStatus::ProcessingFailure => Self::ProcessingFailure,
            Vl53l1xRangeStatus::CrosstalkSignal => Self::CrosstalkSignal,
            Vl53l1xRangeStatus::Synchronisation => Self::Synchronisation,
            Vl53l1xRangeStatus::MergedPulse => Self::MergedPulse,
            Vl53l1xRangeStatus::LackOfSignal => Self::LackOfSignal,
            Vl53l1xRangeStatus::MinRangeFail => Self::MinRangeFail,
            Vl53l1xRangeStatus::InvalidRange => Self::InvalidRange,
            Vl53l1xRangeStatus::None => Self::None,
        }
    }
}
