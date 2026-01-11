mod admin;
mod api;
mod bus;
mod config;
mod display;
mod error;
mod estimation;
mod sensor;
mod state;
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
    Ok(())
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
