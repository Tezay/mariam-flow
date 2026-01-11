mod admin;
mod api;
mod bus;
mod config;
mod display;
mod error;
mod estimation;
mod sensor;
mod state;

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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::config;

    #[test]
    fn default_config_is_valid_toml() -> Result<(), Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(config::DEFAULT_CONFIG_PATH)?;
        let _: toml::Value = toml::from_str(&contents)?;
        Ok(())
    }
}
