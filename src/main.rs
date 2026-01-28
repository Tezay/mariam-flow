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
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::time::Duration;

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

    // Load calibration/model
    let model = match config.calibration_path() {
        Some(path) => match estimation::load_calibration_from_path(path) {
            Ok(model) => {
                tracing::info!(path = %path.display(), "Estimation model loaded");
                model
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load calibration, using default");
                Box::new(estimation::linear_v1::LinearV1Model::with_defaults())
            }
        },
        None => {
            tracing::info!("No calibration path configured, using default model");
            Box::new(estimation::linear_v1::LinearV1Model::with_defaults())
        }
    };

    if let Ok(mut guard) = state.write() {
        guard.set_model(Arc::from(model));
    } else {
        tracing::warn!("State lock poisoned while applying model");
    }

    // Discover sensors at startup
    let sensor_configs = config.sensor_configs();
    let xshut_controller = if sensor_configs.is_empty() {
        tracing::warn!("No sensors configured in [sensors].xshut_pins");
        None
    } else {
        tracing::info!(
            count = sensor_configs.len(),
            pins = ?config.xshut_pins(),
            "Starting sensor discovery"
        );
        run_sensor_discovery(&config, &state)
    };

    let has_sensors = xshut_controller.is_some();

    // Start periodic refresh thread (readings → obstructions → wait time)
    let stop_flag = Arc::new(AtomicBool::new(false));
    let refresh_interval = config.refresh_interval();
    let _refresh_handle = if has_sensors {
        Some(spawn_refresh_thread(
            xshut_controller,
            &state,
            Arc::clone(&stop_flag),
            refresh_interval,
        ))
    } else {
        tracing::warn!("Refresh thread not started - no sensors available");
        None
    };

    let app = api::router(Arc::clone(&state));
    let port = config.server_port();
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "API server listening");
    axum::serve(listener, app).await?;

    // Signal refresh thread to stop
    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);

    Ok(())
}

/// Run sensor discovery and return controller if successful
fn run_sensor_discovery(
    config: &config::Config,
    state: &Arc<RwLock<state::AppState>>,
) -> Option<Box<dyn bus::xshut::XshutController + Send>> {
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
                return None;
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

                if ready > 0 {
                    Some(Box::new(xshut))
                } else {
                    None
                }
            }
            Err(err) => {
                tracing::error!(error = %err, "Sensor discovery failed");
                None
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (config, state);
        tracing::warn!("Sensor discovery requires Linux/Raspberry Pi - skipping");
        None
    }
}

/// Spawn the periodic refresh thread for the estimation pipeline
fn spawn_refresh_thread(
    xshut_controller: Option<Box<dyn bus::xshut::XshutController + Send>>,
    state: &Arc<RwLock<state::AppState>>,
    stop: Arc<AtomicBool>,
    interval: Duration,
) -> std::thread::JoinHandle<()> {
    // Get model from state to pass to thread
    let model = {
        let guard = state.read().expect("state lock poisoned");
        guard.model().clone()
    };
    #[cfg(target_os = "linux")]
    {
        use estimation::spawn_refresh_thread as spawn_thread;
        use sensor::vl53l1x::Vl53l1xFactory;

        let factory = Vl53l1xFactory::default();
        tracing::info!(
            interval_ms = interval.as_millis(),
            "Starting estimation refresh thread"
        );
        spawn_thread(
            factory,
            xshut_controller,
            Arc::clone(state),
            interval,
            stop,
            model,
        )
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (state, stop, interval, model, xshut_controller);
        tracing::warn!("Refresh thread requires Linux/Raspberry Pi - starting dummy thread");
        std::thread::spawn(|| {})
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
