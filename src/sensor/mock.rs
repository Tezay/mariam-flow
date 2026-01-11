use crate::error::AppError;
use crate::sensor::{SensorDriver, SensorDriverFactory};

#[derive(Debug, Clone, Copy)]
pub struct MockSensorBehavior {
    pub init_ok: bool,
    pub set_address_ok: bool,
    pub verify_ok: bool,
}

impl MockSensorBehavior {
    pub fn ok() -> Self {
        Self {
            init_ok: true,
            set_address_ok: true,
            verify_ok: true,
        }
    }

    pub fn fail_init() -> Self {
        Self {
            init_ok: false,
            set_address_ok: true,
            verify_ok: true,
        }
    }

    pub fn fail_set_address() -> Self {
        Self {
            init_ok: true,
            set_address_ok: false,
            verify_ok: true,
        }
    }

    pub fn fail_verify() -> Self {
        Self {
            init_ok: true,
            set_address_ok: true,
            verify_ok: false,
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
}

impl SensorDriverFactory for MockSensorFactory {
    type Driver = MockSensorDriver;

    fn create_default(&mut self) -> Result<Self::Driver, AppError> {
        let behavior = self
            .behaviors
            .get(self.next_index)
            .copied()
            .unwrap_or_else(MockSensorBehavior::ok);
        self.next_index += 1;
        Ok(MockSensorDriver { behavior })
    }
}
