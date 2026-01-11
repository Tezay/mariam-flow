use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid I2C address: {0:#04x}")]
    InvalidAddress(u8),
    #[error("address allocation exceeds 7-bit I2C range")]
    AddressAllocationOverflow,
    #[error("sensor error: {0}")]
    Sensor(String),
    #[error("xshut error: {0}")]
    Xshut(String),
    #[error("watch channel send failed")]
    WatchSend,
    #[error("gpio error: {0}")]
    Gpio(String),
    #[error("i2c error: {0}")]
    I2c(String),
    #[error("state lock poisoned")]
    StateLock,
}
