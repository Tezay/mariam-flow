//! Entry point of the edge appliance daemon.
//!
//! Boot sequence: load and validate the appliance configuration, note
//! whether an active density model is installed, then serve the status
//! surface. An appliance with an unusable configuration fails loudly at
//! start rather than serving a half-configured system.

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use flow_edge::{ApplianceConfig, EdgeState, Phase, router};

/// File name of the active density model inside the data directory.
const ACTIVE_MODEL: &str = "model.onnx";

/// Run the Mariam Flow edge appliance.
#[derive(Debug, Parser)]
#[command(name = "flow-edge", version)]
struct Args {
    /// Path to the appliance configuration file.
    #[arg(short, long, default_value = "/etc/mariam-flow/appliance.json")]
    config: PathBuf,

    /// Directory holding appliance data: the active model, capture
    /// sessions, and the state database.
    #[arg(short, long, default_value = "/var/lib/mariam-flow")]
    data_dir: PathBuf,

    /// Address to serve the dashboard and API on.
    ///
    /// Local by default. The deployed appliance binds the sensor access
    /// point interface instead, which is only reachable by devices that
    /// already hold its Wi-Fi passphrase.
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    listen: String,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            let mut source = err.source();
            while let Some(cause) = source {
                eprintln!("  caused by: {cause}");
                source = cause.source();
            }
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let config = ApplianceConfig::load(&args.config)?;
    let model_installed = args.data_dir.join(ACTIVE_MODEL).is_file();

    let kit_id = config.identity.kit_id.clone();
    let state = EdgeState::new(config, model_installed);

    match state.phase() {
        Phase::Onboarding { stage } => {
            eprintln!("{kit_id}: installation in progress — next step: {stage}");
        }
        Phase::Operational => eprintln!("{kit_id}: operational"),
    }
    if !model_installed {
        eprintln!(
            "no active model at {}",
            args.data_dir.join(ACTIVE_MODEL).display()
        );
    }

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(&args.listen).await?;
        eprintln!("serving on http://{}", args.listen);
        axum::serve(listener, router(state)).await?;
        Ok::<(), Box<dyn Error>>(())
    })
}
