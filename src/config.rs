use crate::sensor::{SensorConfig, build_sensor_configs};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

pub const DEFAULT_CONFIG_PATH: &str = "config/config.toml";
pub const DEFAULT_SERVER_PORT: u16 = 8080;
pub const DEFAULT_REFRESH_INTERVAL_SECS: u64 = 5;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppSection,
    pub logging: LoggingSection,
    #[serde(default)]
    pub calibration: Option<CalibrationSettings>,
    #[serde(default)]
    pub sensors: Option<SensorsSection>,
    #[serde(default)]
    pub server: Option<ServerSection>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppSection {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingSection {
    pub level: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CalibrationSettings {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SensorsSection {
    /// GPIO pin numbers for XSHUT control, in sensor order
    pub xshut_pins: Vec<u8>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSection {
    /// Port to listen on (default: 8080)
    pub port: Option<u16>,
    /// Refresh interval in seconds for the estimation pipeline (default: 5)
    pub refresh_interval_secs: Option<u64>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Read(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

pub fn load_default() -> Result<Config, ConfigError> {
    load_from_path(DEFAULT_CONFIG_PATH)
}

pub fn load_from_path(path: impl AsRef<Path>) -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(config)
}

impl Config {
    pub fn calibration_path(&self) -> Option<&Path> {
        let path = self.calibration.as_ref()?.path.as_deref()?;
        if path.as_os_str().is_empty() {
            None
        } else {
            Some(path)
        }
    }

    /// Returns sensor configurations built from xshut_pins, or empty vec if not configured.
    pub fn sensor_configs(&self) -> Vec<SensorConfig> {
        match &self.sensors {
            Some(section) if !section.xshut_pins.is_empty() => {
                build_sensor_configs(&section.xshut_pins)
            }
            _ => Vec::new(),
        }
    }

    /// Returns the XSHUT pin numbers, or empty slice if not configured.
    pub fn xshut_pins(&self) -> &[u8] {
        self.sensors
            .as_ref()
            .map(|s| s.xshut_pins.as_slice())
            .unwrap_or(&[])
    }

    /// Returns the server port (default: 8080)
    pub fn server_port(&self) -> u16 {
        self.server
            .as_ref()
            .and_then(|s| s.port)
            .unwrap_or(DEFAULT_SERVER_PORT)
    }

    /// Returns the refresh interval as Duration (default: 5 seconds)
    pub fn refresh_interval(&self) -> Duration {
        let secs = self
            .server
            .as_ref()
            .and_then(|s| s.refresh_interval_secs)
            .unwrap_or(DEFAULT_REFRESH_INTERVAL_SECS);
        Duration::from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_includes_calibration_path() -> Result<(), Box<dyn std::error::Error>> {
        let config = load_default()?;
        assert!(config.calibration_path().is_some());
        Ok(())
    }

    #[test]
    fn empty_calibration_path_is_treated_as_missing() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = temp_dir.join(format!("mariam-config-{unique}.toml"));
        let contents = r#"
[app]
name = "mariam-flow"

[logging]
level = "info"

[calibration]
path = ""
"#;
        fs::write(&path, contents)?;

        let result = load_from_path(&path)?;
        let _ = fs::remove_file(&path);

        assert!(result.calibration_path().is_none());
        Ok(())
    }

    #[test]
    fn missing_calibration_section_is_allowed() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = temp_dir.join(format!("mariam-config-missing-cal-{unique}.toml"));
        let contents = r#"
[app]
name = "mariam-flow"

[logging]
level = "info"
"#;
        fs::write(&path, contents)?;

        let result = load_from_path(&path)?;
        let _ = fs::remove_file(&path);

        assert!(result.calibration_path().is_none());
        Ok(())
    }

    #[test]
    fn missing_config_file_returns_read_error() {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let path = temp_dir.join(format!("mariam-config-missing-{unique}.toml"));

        let result = load_from_path(&path);

        assert!(matches!(result, Err(ConfigError::Read(_))));
    }

    #[test]
    fn invalid_toml_returns_parse_error() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = std::env::temp_dir();
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = temp_dir.join(format!("mariam-config-invalid-{unique}.toml"));
        fs::write(&path, "not = [valid")?;

        let result = load_from_path(&path);
        let _ = fs::remove_file(&path);

        assert!(matches!(result, Err(ConfigError::Parse(_))));
        Ok(())
    }
}
