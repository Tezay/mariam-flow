use crate::error::AppError;

pub mod mock;
pub mod vl53l1x;

pub type SensorId = u32;

// VL53L1X default is 0x52 in 8-bit notation; use 0x29 for 7-bit addressing.
pub const DEFAULT_I2C_ADDRESS_7BIT: u8 = 0x29;
pub const ADDRESS_BASE_7BIT: u8 = 0x30;
pub const I2C_7BIT_MAX: u8 = 0x77;

#[derive(Debug, Clone)]
pub struct SensorConfig {
    pub sensor_id: SensorId,
    pub xshut_pin: u8,
}

#[derive(Debug, Clone)]
pub enum SensorStatus {
    Ready,
    Error { message: String },
}

#[derive(Debug, Clone)]
pub struct SensorInfo {
    pub sensor_id: SensorId,
    pub xshut_pin: u8,
    pub i2c_address: u8,
    pub status: SensorStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorRangeStatus {
    Valid,
    SigmaFailure,
    SignalFailure,
    MinRangeClipped,
    OutOfBounds,
    HardwareFailure,
    WrapCheckFail,
    Wraparound,
    ProcessingFailure,
    CrosstalkSignal,
    Synchronisation,
    MergedPulse,
    LackOfSignal,
    MinRangeFail,
    InvalidRange,
    None,
}

impl SensorRangeStatus {
    pub fn is_valid(self) -> bool {
        matches!(self, Self::Valid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DistanceMeasurement {
    pub distance_mm: u16,
    pub range_status: SensorRangeStatus,
}

pub trait SensorDriver {
    fn init_default(&mut self) -> Result<(), AppError>;
    fn set_address(&mut self, new_address: u8) -> Result<(), AppError>;
    fn verify(&mut self) -> Result<(), AppError>;
    /// Start continuous ranging mode. Must be called after init before reading distances.
    fn start_ranging(&mut self) -> Result<(), AppError>;
    fn read_distance(&mut self) -> Result<DistanceMeasurement, AppError>;
}

pub trait SensorDriverFactory {
    type Driver: SensorDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError>;
    fn create_for_address(&mut self, address: u8) -> Result<Self::Driver, AppError>;
}

/// Build deterministic sensor configs from an ordered list of XSHUT pins.
pub fn build_sensor_configs(xshut_pins: &[u8]) -> Vec<SensorConfig> {
    xshut_pins
        .iter()
        .enumerate()
        .map(|(index, pin)| SensorConfig {
            sensor_id: (index + 1) as SensorId,
            xshut_pin: *pin,
        })
        .collect()
}
