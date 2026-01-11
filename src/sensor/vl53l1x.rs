use crate::error::AppError;
use crate::sensor::{SensorDriver, SensorDriverFactory};

#[cfg(target_os = "linux")]
use crate::sensor::DEFAULT_I2C_ADDRESS_7BIT;
#[cfg(target_os = "linux")]
use rppal::i2c::I2c;
#[cfg(target_os = "linux")]
use vl53l1x_uld::{IOVoltage, VL53L1X};

#[cfg(target_os = "linux")]
pub struct Vl53l1xFactory {
    io_voltage: IOVoltage,
}

#[cfg(target_os = "linux")]
impl Vl53l1xFactory {
    pub fn new(io_voltage: IOVoltage) -> Self {
        Self { io_voltage }
    }
}

#[cfg(target_os = "linux")]
pub struct Vl53l1xDriver {
    driver: VL53L1X<I2c>,
    io_voltage: IOVoltage,
}

#[cfg(target_os = "linux")]
impl SensorDriver for Vl53l1xDriver {
    fn init_default(&mut self) -> Result<(), AppError> {
        self.driver
            .init(self.io_voltage)
            .map_err(|err| AppError::Sensor(format!("{err:?}")))
    }

    fn set_address(&mut self, new_address: u8) -> Result<(), AppError> {
        self.driver
            .set_address(new_address)
            .map_err(|err| AppError::Sensor(format!("{err:?}")))
    }

    fn verify(&mut self) -> Result<(), AppError> {
        self.driver
            .get_sensor_id()
            .map(|_| ())
            .map_err(|err| AppError::Sensor(format!("{err:?}")))
    }
}

#[cfg(target_os = "linux")]
impl SensorDriverFactory for Vl53l1xFactory {
    type Driver = Vl53l1xDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError> {
        let i2c = I2c::new().map_err(|err| AppError::I2c(err.to_string()))?;
        let driver = VL53L1X::new(i2c, DEFAULT_I2C_ADDRESS_7BIT);
        Ok(Vl53l1xDriver {
            driver,
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
}

#[cfg(not(target_os = "linux"))]
impl SensorDriverFactory for Vl53l1xFactory {
    type Driver = Vl53l1xDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError> {
        Err(AppError::Sensor(
            "VL53L1X driver requires Linux/Raspberry Pi".to_string(),
        ))
    }
}
