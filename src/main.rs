mod admin;
mod api;
mod bus;
mod config;
mod display;
mod error;
mod estimation;
mod sensor;
mod state;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

fn init_tracing() {
    let subscriber = tracing_subscriber::fmt().with_target(false).finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    tracing::info!(
        config_path = config::DEFAULT_CONFIG_PATH,
        "mariam-flow starting"
    );
    let config = config::load_default()?;
    let state = Arc::new(RwLock::new(state::AppState::new()));

    // Load calibration
    let calibration_load = match config.calibration_path() {
        Some(path) => estimation::load_calibration_from_path(path),
        None => estimation::CalibrationLoad::inactive("calibration path not configured"),
    };
    if let Ok(mut guard) = state.write() {
        guard.set_calibration(calibration_load.calibration.clone());
    } else {
        tracing::warn!("State lock poisoned while applying calibration");
    }
    if let Some(calibration) = calibration_load.calibration {
        if let Some(path) = config.calibration_path() {
            tracing::info!(
                path = %path.display(),
                slope = calibration.slope,
                intercept = calibration.intercept,
                "Calibration active"
            );
        } else {
            tracing::info!(
                slope = calibration.slope,
                intercept = calibration.intercept,
                "Calibration active"
            );
        }
    } else if let Some(reason) = calibration_load.inactive_reason {
        if let Some(path) = config.calibration_path() {
            tracing::warn!(
                path = %path.display(),
                reason = reason,
                "Calibration inactive"
            );
        } else {
            tracing::warn!(reason = reason, "Calibration inactive");
        }
    }

    // Discover sensors at startup
    let sensor_configs = config.sensor_configs();
    if sensor_configs.is_empty() {
        tracing::warn!("No sensors configured in [sensors].xshut_pins");
    } else {
        tracing::info!(
            count = sensor_configs.len(),
            pins = ?config.xshut_pins(),
            "Starting sensor discovery"
        );
        run_sensor_discovery(&config, &state);
    }

    let app = api::router(Arc::clone(&state));
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "API server listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Run sensor discovery
fn run_sensor_discovery(config: &config::Config, state: &Arc<RwLock<state::AppState>>) {
    #[cfg(target_os = "linux")]
    {
        use bus::xshut::{RppalXshutController, discover_and_store_sensors};
        use sensor::vl53l1x::Vl53l1xFactory;

        let xshut_pins = config.xshut_pins();
        let sensor_configs = config.sensor_configs();

        let mut xshut = match RppalXshutController::new(xshut_pins) {
            Ok(xshut) => xshut,
            Err(err) => {
                tracing::error!(error = %err, "Failed to initialize GPIO for XSHUT");
                return;
            }
        };

        let mut factory = Vl53l1xFactory::default();

        match discover_and_store_sensors(&mut xshut, &mut factory, &sensor_configs, state) {
            Ok(results) => {
                let ready = results
                    .iter()
                    .filter(|s| matches!(s.status, sensor::SensorStatus::Ready))
                    .count();
                let errors = results.len() - ready;
                tracing::info!(
                    total = results.len(),
                    ready = ready,
                    errors = errors,
                    "Sensor discovery complete"
                );
            }
            Err(err) => {
                tracing::error!(error = %err, "Sensor discovery failed");
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (config, state);
        tracing::warn!("Sensor discovery requires Linux/Raspberry Pi - skipping");
    }
}

#[cfg(test)]
mod tests {
    use super::config;

    #[test]
    fn default_config_is_valid_toml() -> Result<(), Box<dyn std::error::Error>> {
        let _config = config::load_default()?;
        Ok(())
    }
}
