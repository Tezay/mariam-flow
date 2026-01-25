use crate::error::AppError;
use crate::sensor::{DistanceMeasurement, SensorDriver, SensorDriverFactory, SensorRangeStatus};

#[derive(Debug, Clone, Copy)]
pub struct MockSensorBehavior {
    pub create_ok: bool,
    pub init_ok: bool,
    pub set_address_ok: bool,
    pub verify_ok: bool,
    pub read_distance_ok: bool,
    pub distance_mm: u16,
    pub range_status: SensorRangeStatus,
}

impl MockSensorBehavior {
    pub fn ok() -> Self {
        Self {
            create_ok: true,
            init_ok: true,
            set_address_ok: true,
            verify_ok: true,
            read_distance_ok: true,
            distance_mm: 0,
            range_status: SensorRangeStatus::Valid,
        }
    }

    pub fn fail_init() -> Self {
        Self {
            create_ok: true,
            init_ok: false,
            set_address_ok: true,
            verify_ok: true,
            read_distance_ok: true,
            distance_mm: 0,
            range_status: SensorRangeStatus::Valid,
        }
    }

    pub fn fail_set_address() -> Self {
        Self {
            create_ok: true,
            init_ok: true,
            set_address_ok: false,
            verify_ok: true,
            read_distance_ok: true,
            distance_mm: 0,
            range_status: SensorRangeStatus::Valid,
        }
    }

    pub fn fail_verify() -> Self {
        Self {
            create_ok: true,
            init_ok: true,
            set_address_ok: true,
            verify_ok: false,
            read_distance_ok: true,
            distance_mm: 0,
            range_status: SensorRangeStatus::Valid,
        }
    }

    pub fn with_reading(distance_mm: u16, range_status: SensorRangeStatus) -> Self {
        Self {
            create_ok: true,
            init_ok: true,
            set_address_ok: true,
            verify_ok: true,
            read_distance_ok: true,
            distance_mm,
            range_status,
        }
    }

    pub fn fail_create() -> Self {
        Self {
            create_ok: false,
            init_ok: true,
            set_address_ok: true,
            verify_ok: true,
            read_distance_ok: true,
            distance_mm: 0,
            range_status: SensorRangeStatus::Valid,
        }
    }

    pub fn fail_read_distance() -> Self {
        Self {
            create_ok: true,
            init_ok: true,
            set_address_ok: true,
            verify_ok: true,
            read_distance_ok: false,
            distance_mm: 0,
            range_status: SensorRangeStatus::Valid,
        }
    }
}

pub struct MockSensorFactory {
    behaviors: Vec<MockSensorBehavior>,
    next_index: usize,
}

impl MockSensorFactory {
    pub fn new(behaviors: Vec<MockSensorBehavior>) -> Self {
        Self {
            behaviors,
            next_index: 0,
        }
    }

    fn next_behavior(&mut self) -> MockSensorBehavior {
        let behavior = self
            .behaviors
            .get(self.next_index)
            .copied()
            .unwrap_or_else(MockSensorBehavior::ok);
        self.next_index += 1;
        behavior
    }
}

pub struct MockSensorDriver {
    behavior: MockSensorBehavior,
}

impl SensorDriver for MockSensorDriver {
    fn init_default(&mut self) -> Result<(), AppError> {
        if self.behavior.init_ok {
            Ok(())
        } else {
            Err(AppError::Sensor("mock init failed".to_string()))
        }
    }

    fn set_address(&mut self, _new_address: u8) -> Result<(), AppError> {
        if self.behavior.set_address_ok {
            Ok(())
        } else {
            Err(AppError::Sensor("mock set address failed".to_string()))
        }
    }

    fn verify(&mut self) -> Result<(), AppError> {
        if self.behavior.verify_ok {
            Ok(())
        } else {
            Err(AppError::Sensor("mock verify failed".to_string()))
        }
    }

    fn start_ranging(&mut self) -> Result<(), AppError> {
        // Mock always succeeds for start_ranging
        Ok(())
    }

    fn read_distance(&mut self) -> Result<DistanceMeasurement, AppError> {
        if self.behavior.read_distance_ok {
            Ok(DistanceMeasurement {
                distance_mm: self.behavior.distance_mm,
                range_status: self.behavior.range_status,
            })
        } else {
            Err(AppError::Sensor("mock read distance failed".to_string()))
        }
    }
}

impl SensorDriverFactory for MockSensorFactory {
    type Driver = MockSensorDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError> {
        let behavior = self.next_behavior();
        if behavior.create_ok {
            Ok(MockSensorDriver { behavior })
        } else {
            Err(AppError::Sensor("mock create failed".to_string()))
        }
    }

    fn create_for_address(&mut self, _address: u8) -> Result<Self::Driver, AppError> {
        self.create_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_distance_returns_measurement() {
        let behavior = MockSensorBehavior::with_reading(123, SensorRangeStatus::Valid);
        let mut driver = MockSensorDriver { behavior };

        let measurement = driver.read_distance().expect("read distance ok");

        assert_eq!(measurement.distance_mm, 123);
        assert_eq!(measurement.range_status, SensorRangeStatus::Valid);
    }

    #[test]
    fn read_distance_can_fail() {
        let behavior = MockSensorBehavior::fail_read_distance();
        let mut driver = MockSensorDriver { behavior };

        let err = driver.read_distance().unwrap_err();

        assert_eq!(err.to_string(), "sensor error: mock read distance failed");
    }
}
