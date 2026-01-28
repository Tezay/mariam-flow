use crate::error::AppError;
use crate::sensor::{
    ADDRESS_BASE_7BIT, DEFAULT_I2C_ADDRESS_7BIT, I2C_7BIT_MAX, SensorConfig, SensorDriver,
    SensorDriverFactory, SensorId, SensorInfo, SensorStatus,
};
use crate::state::AppState;
use std::sync::{Arc, RwLock};
use tracing::{debug, info, warn};

pub trait XshutController {
    fn set_all_low(&mut self) -> Result<(), AppError>;
    fn set_high(&mut self, pin: u8) -> Result<(), AppError>;
    fn power_cycle_sensor(&mut self, pin: u8) -> Result<(), AppError>;
}

impl XshutController for Box<dyn XshutController + Send> {
    fn set_all_low(&mut self) -> Result<(), AppError> {
        (**self).set_all_low()
    }
    fn set_high(&mut self, pin: u8) -> Result<(), AppError> {
        (**self).set_high(pin)
    }
    fn power_cycle_sensor(&mut self, pin: u8) -> Result<(), AppError> {
        (**self).power_cycle_sensor(pin)
    }
}

#[derive(Debug, Clone)]
pub struct AddressedSensor {
    pub sensor_id: SensorId,
    pub xshut_pin: u8,
    pub i2c_address: u8,
}

/// Allocate deterministic 7-bit I2C addresses using a base + offset strategy.
pub fn allocate_addresses(
    base_address: u8,
    sensors: &[SensorConfig],
) -> Result<Vec<AddressedSensor>, AppError> {
    if base_address > I2C_7BIT_MAX {
        return Err(AppError::InvalidAddress(base_address));
    }

    let max_offset = sensors.len().saturating_sub(1) as u8;
    if base_address
        .checked_add(max_offset)
        .map(|addr| addr > I2C_7BIT_MAX)
        .unwrap_or(true)
    {
        return Err(AppError::AddressAllocationOverflow);
    }

    let mut addressed = Vec::with_capacity(sensors.len());
    for (index, sensor) in sensors.iter().enumerate() {
        let address = base_address
            .checked_add(index as u8)
            .ok_or(AppError::AddressAllocationOverflow)?;
        addressed.push(AddressedSensor {
            sensor_id: sensor.sensor_id,
            xshut_pin: sensor.xshut_pin,
            i2c_address: address,
        });
    }

    Ok(addressed)
}

/// Discover sensors by toggling XSHUT and assigning unique 7-bit I2C addresses.
pub fn discover_and_address_sensors<X, F>(
    xshut: &mut X,
    factory: &mut F,
    sensors: &[SensorConfig],
) -> Result<Vec<SensorInfo>, AppError>
where
    X: XshutController,
    F: SensorDriverFactory,
{
    let addressed = allocate_addresses(ADDRESS_BASE_7BIT, sensors)?;
    xshut.set_all_low()?;
    info!(
        count = addressed.len(),
        default_address = format_args!("{DEFAULT_I2C_ADDRESS_7BIT:#04x}"),
        base_address = format_args!("{ADDRESS_BASE_7BIT:#04x}"),
        "Starting XSHUT sequencing"
    );

    let mut results = Vec::with_capacity(addressed.len());
    for sensor in addressed {
        xshut.set_high(sensor.xshut_pin)?;
        // Allow sensor boot time after XSHUT release (2ms per VL53L1X datasheet)
        std::thread::sleep(std::time::Duration::from_millis(2));
        debug!(
            sensor_id = sensor.sensor_id,
            xshut_pin = sensor.xshut_pin,
            "Sensor XSHUT enabled"
        );

        let mut driver = match factory.create_default() {
            Ok(driver) => driver,
            Err(err) => {
                warn!(
                    sensor_id = sensor.sensor_id,
                    error = %err,
                    "Failed to create sensor driver"
                );
                results.push(error_info(&sensor, err));
                continue;
            }
        };

        if let Err(err) = driver.init_default() {
            warn!(
                sensor_id = sensor.sensor_id,
                error = %err,
                "Failed to initialize sensor on default address"
            );
            results.push(error_info(&sensor, err));
            continue;
        }

        if let Err(err) = driver.set_address(sensor.i2c_address) {
            warn!(
                sensor_id = sensor.sensor_id,
                new_address = format_args!("{:#04x}", sensor.i2c_address),
                error = %err,
                "Failed to assign sensor address"
            );
            results.push(error_info(&sensor, err));
            continue;
        }

        if let Err(err) = driver.verify() {
            warn!(
                sensor_id = sensor.sensor_id,
                address = format_args!("{:#04x}", sensor.i2c_address),
                error = %err,
                "Failed to verify sensor after address assignment"
            );
            results.push(error_info(&sensor, err));
            continue;
        }

        // Start continuous ranging mode
        if let Err(err) = driver.start_ranging() {
            warn!(
                sensor_id = sensor.sensor_id,
                address = format_args!("{:#04x}", sensor.i2c_address),
                error = %err,
                "Failed to start ranging on sensor"
            );
            results.push(error_info(&sensor, err));
            continue;
        }

        results.push(SensorInfo {
            sensor_id: sensor.sensor_id,
            xshut_pin: sensor.xshut_pin,
            i2c_address: sensor.i2c_address,
            status: SensorStatus::Ready,
        });
    }

    Ok(results)
}

/// Reinitialize a specific sensor (Power cycle -> Re-address).
pub fn reinitialize_sensor<X, F>(
    xshut: &mut X,
    factory: &mut F,
    sensor: &SensorInfo,
) -> Result<(), AppError>
where
    X: XshutController + ?Sized,
    F: SensorDriverFactory,
{
    // 1. Power cycle (Hard Reset)
    info!(sensor_id = sensor.sensor_id, "Resetting sensor hardware");
    xshut.power_cycle_sensor(sensor.xshut_pin)?;

    // 2. Initialize driver on default address
    let mut driver = factory.create_default()?;
    if let Err(e) = driver.init_default() {
        warn!(sensor_id = sensor.sensor_id, error = %e, "Failed to init default during reset");
        return Err(e);
    }

    // 3. Re-assign address
    if let Err(e) = driver.set_address(sensor.i2c_address) {
        warn!(sensor_id = sensor.sensor_id, error = %e, "Failed to set address during reset");
        return Err(e);
    }

    // 4. Verify & Start
    driver.verify()?;
    driver.start_ranging()?;

    info!(
        sensor_id = sensor.sensor_id,
        address = format_args!("{:#04x}", sensor.i2c_address),
        "Sensor re-initialized successfully"
    );
    Ok(())
}

/// Discover sensors and persist results in shared state for the rest of the pipeline.
pub fn discover_and_store_sensors<X, F>(
    xshut: &mut X,
    factory: &mut F,
    sensors: &[SensorConfig],
    state: &Arc<RwLock<AppState>>,
) -> Result<Vec<SensorInfo>, AppError>
where
    X: XshutController,
    F: SensorDriverFactory,
{
    let results = discover_and_address_sensors(xshut, factory, sensors)?;
    let mut guard = state.write().map_err(|_| AppError::StateLock)?;
    guard.set_sensors(results.clone())?;
    Ok(results)
}

/// Spawn discovery in a dedicated sync thread to avoid blocking async tasks.
pub fn spawn_discovery_thread<X, F>(
    mut xshut: X,
    mut factory: F,
    sensors: Vec<SensorConfig>,
    state: Arc<RwLock<AppState>>,
) -> std::thread::JoinHandle<Result<Vec<SensorInfo>, AppError>>
where
    X: XshutController + Send + 'static,
    F: SensorDriverFactory + Send + 'static,
{
    std::thread::spawn(move || {
        discover_and_store_sensors(&mut xshut, &mut factory, &sensors, &state)
    })
}

fn error_info(sensor: &AddressedSensor, err: AppError) -> SensorInfo {
    SensorInfo {
        sensor_id: sensor.sensor_id,
        xshut_pin: sensor.xshut_pin,
        i2c_address: sensor.i2c_address,
        status: SensorStatus::Error {
            message: err.to_string(),
        },
    }
}

#[cfg(target_os = "linux")]
pub struct RppalXshutController {
    pins: std::collections::HashMap<u8, rppal::gpio::OutputPin>,
}

#[cfg(target_os = "linux")]
impl RppalXshutController {
    pub fn new(pins: &[u8]) -> Result<Self, AppError> {
        let gpio = rppal::gpio::Gpio::new().map_err(|err| AppError::Gpio(err.to_string()))?;
        let mut map = std::collections::HashMap::new();
        for pin in pins {
            let output = gpio
                .get(*pin)
                .map_err(|err| AppError::Gpio(err.to_string()))?
                .into_output();
            map.insert(*pin, output);
        }
        Ok(Self { pins: map })
    }
}

#[cfg(target_os = "linux")]
impl XshutController for RppalXshutController {
    fn set_all_low(&mut self) -> Result<(), AppError> {
        for pin in self.pins.values_mut() {
            pin.set_low();
        }
        Ok(())
    }

    fn set_high(&mut self, pin: u8) -> Result<(), AppError> {
        let output = self
            .pins
            .get_mut(&pin)
            .ok_or_else(|| AppError::Xshut(format!("missing XSHUT pin {pin}")))?;
        output.set_high();
        Ok(())
    }

    fn power_cycle_sensor(&mut self, pin: u8) -> Result<(), AppError> {
        let output = self
            .pins
            .get_mut(&pin)
            .ok_or_else(|| AppError::Xshut(format!("missing XSHUT pin {pin}")))?;

        // Cycle: Low (OFF) -> Wait -> High (ON)
        output.set_low();
        std::thread::sleep(std::time::Duration::from_millis(10));
        output.set_high();
        std::thread::sleep(std::time::Duration::from_millis(10)); // Boot time

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensor::mock::{MockSensorBehavior, MockSensorFactory};

    #[derive(Default)]
    struct MockXshut {
        actions: Vec<String>,
    }

    impl XshutController for MockXshut {
        fn set_all_low(&mut self) -> Result<(), AppError> {
            self.actions.push("all_low".to_string());
            Ok(())
        }

        fn set_high(&mut self, pin: u8) -> Result<(), AppError> {
            self.actions.push(format!("high:{pin}"));
            Ok(())
        }

        fn power_cycle_sensor(&mut self, pin: u8) -> Result<(), AppError> {
            self.actions.push(format!("cycle:{pin}"));
            Ok(())
        }
    }

    #[test]
    fn address_allocation_is_unique() -> Result<(), AppError> {
        let sensors = vec![
            SensorConfig {
                sensor_id: 1,
                xshut_pin: 17,
            },
            SensorConfig {
                sensor_id: 2,
                xshut_pin: 27,
            },
            SensorConfig {
                sensor_id: 3,
                xshut_pin: 22,
            },
        ];

        let addressed = allocate_addresses(ADDRESS_BASE_7BIT, &sensors)?;
        let mut addresses: Vec<u8> = addressed.iter().map(|sensor| sensor.i2c_address).collect();
        addresses.sort_unstable();
        addresses.dedup();
        assert_eq!(addresses.len(), sensors.len());
        Ok(())
    }

    #[test]
    fn address_allocation_rejects_overflow() {
        let sensors = vec![
            SensorConfig {
                sensor_id: 1,
                xshut_pin: 17,
            },
            SensorConfig {
                sensor_id: 2,
                xshut_pin: 27,
            },
        ];
        let result = allocate_addresses(I2C_7BIT_MAX, &sensors);
        assert!(matches!(result, Err(AppError::AddressAllocationOverflow)));
    }

    #[test]
    fn sequencing_continues_on_sensor_error() -> Result<(), AppError> {
        let sensors = vec![
            SensorConfig {
                sensor_id: 1,
                xshut_pin: 17,
            },
            SensorConfig {
                sensor_id: 2,
                xshut_pin: 27,
            },
            SensorConfig {
                sensor_id: 3,
                xshut_pin: 22,
            },
        ];

        let behaviors = vec![
            MockSensorBehavior::ok(),
            MockSensorBehavior::fail_init(),
            MockSensorBehavior::ok(),
        ];
        let mut factory = MockSensorFactory::new(behaviors);
        let mut xshut = MockXshut::default();

        let results = discover_and_address_sensors(&mut xshut, &mut factory, &sensors)?;
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0].status, SensorStatus::Ready));
        assert!(matches!(results[1].status, SensorStatus::Error { .. }));
        assert!(matches!(results[2].status, SensorStatus::Ready));
        assert_eq!(
            xshut.actions,
            vec!["all_low", "high:17", "high:27", "high:22"]
        );
        Ok(())
    }

    #[test]
    fn sequencing_records_set_address_error() -> Result<(), AppError> {
        let sensors = vec![
            SensorConfig {
                sensor_id: 1,
                xshut_pin: 17,
            },
            SensorConfig {
                sensor_id: 2,
                xshut_pin: 27,
            },
        ];

        let behaviors = vec![
            MockSensorBehavior::ok(),
            MockSensorBehavior::fail_set_address(),
        ];
        let mut factory = MockSensorFactory::new(behaviors);
        let mut xshut = MockXshut::default();

        let results = discover_and_address_sensors(&mut xshut, &mut factory, &sensors)?;
        assert!(matches!(results[0].status, SensorStatus::Ready));
        assert!(matches!(results[1].status, SensorStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn sequencing_records_verify_error() -> Result<(), AppError> {
        let sensors = vec![
            SensorConfig {
                sensor_id: 1,
                xshut_pin: 17,
            },
            SensorConfig {
                sensor_id: 2,
                xshut_pin: 27,
            },
        ];

        let behaviors = vec![MockSensorBehavior::ok(), MockSensorBehavior::fail_verify()];
        let mut factory = MockSensorFactory::new(behaviors);
        let mut xshut = MockXshut::default();

        let results = discover_and_address_sensors(&mut xshut, &mut factory, &sensors)?;
        assert!(matches!(results[0].status, SensorStatus::Ready));
        assert!(matches!(results[1].status, SensorStatus::Error { .. }));
        Ok(())
    }

    #[test]
    fn discovery_updates_shared_state() -> Result<(), AppError> {
        let sensors = vec![
            SensorConfig {
                sensor_id: 1,
                xshut_pin: 17,
            },
            SensorConfig {
                sensor_id: 2,
                xshut_pin: 27,
            },
        ];

        let behaviors = vec![MockSensorBehavior::ok(), MockSensorBehavior::ok()];
        let mut factory = MockSensorFactory::new(behaviors);
        let mut xshut = MockXshut::default();

        let state = Arc::new(RwLock::new(AppState::new()));
        let mut receiver = {
            let guard = state.read().map_err(|_| AppError::StateLock)?;
            guard.subscribe_sensors()
        };

        let results = discover_and_store_sensors(&mut xshut, &mut factory, &sensors, &state)?;
        let updated = receiver.borrow_and_update().clone();

        assert_eq!(updated.len(), results.len());
        Ok(())
    }
}
