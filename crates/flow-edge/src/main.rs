//! Entry point of the edge appliance daemon.
//!
//! Three jobs, one binary:
//!
//! - `serve` — what an installed appliance runs. It consumes any pending
//!   credential recovery, loads and validates the configuration, refuses
//!   to start without a credential to authenticate against, then serves.
//! - `provision` — run once while preparing a unit: draws a device secret,
//!   prints it for the label, and stores only its hash on the card.
//! - `new-secret` — prints a secret without touching anything, for filling
//!   in a recovery file.

use std::error::Error;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use flow_edge::{
    AdminCredential, ApplianceConfig, DeviceSecret, EdgeState, Event, EventKind, Journal, Phase,
    ResetOutcome, SECRET_ENTROPY_BITS, apply_pending_reset, now_us, router,
};

/// File name of the active density model inside the data directory.
const ACTIVE_MODEL: &str = "model.onnx";

/// How often buffered journal entries are written out.
const FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Flushes between two prunes — roughly one hour.
const PRUNE_EVERY_TICKS: u64 = 720;

/// Run the Mariam Flow edge appliance.
#[derive(Debug, Parser)]
#[command(name = "flow-edge", version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the dashboard and API (what an installed appliance runs).
    Serve(ServeArgs),
    /// Establish this appliance's administrator credential.
    Provision(ProvisionArgs),
    /// Print a fresh device secret without storing anything.
    NewSecret,
}

#[derive(Debug, Parser)]
struct ServeArgs {
    /// Path to the appliance configuration file.
    #[arg(short, long, default_value = "/etc/mariam-flow/appliance.json")]
    config: PathBuf,

    /// Directory holding appliance data: the credential, the active model,
    /// capture sessions, and the state database.
    #[arg(short, long, default_value = "/var/lib/mariam-flow")]
    data_dir: PathBuf,

    /// Credential recovery file, consumed and deleted at startup.
    ///
    /// It sits on the card's boot partition because that is the partition
    /// an ordinary desktop can write to: recovering a unit means putting
    /// the card in a laptop, not having a login on the appliance.
    #[arg(long, default_value = "/boot/firmware/mariam-flow-secret-reset")]
    reset_file: PathBuf,

    /// Address to serve the dashboard and API on.
    ///
    /// Local by default. The deployed appliance binds the sensor access
    /// point interface instead, which is only reachable by devices that
    /// already hold its Wi-Fi passphrase.
    #[arg(short, long, default_value = "127.0.0.1:8080")]
    listen: String,
}

#[derive(Debug, Parser)]
struct ProvisionArgs {
    /// Directory the credential is written to.
    #[arg(short, long, default_value = "/var/lib/mariam-flow")]
    data_dir: PathBuf,

    /// Replace an existing credential.
    ///
    /// Off by default: re-provisioning a unit that is already in service
    /// invalidates the label on its case, which should never happen by
    /// accident.
    #[arg(long)]
    force: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let outcome = match &args.command {
        Command::Serve(args) => serve(args),
        Command::Provision(args) => provision(args),
        Command::NewSecret => {
            print_secret(&DeviceSecret::generate());
            Ok(())
        }
    };
    match outcome {
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

fn serve(args: &ServeArgs) -> Result<(), Box<dyn Error>> {
    // Recovery runs before anything else: an operator who has lost the
    // label must be able to get back in even if the rest is unhappy.
    let mut reset_applied = false;
    match apply_pending_reset(&args.reset_file, &args.data_dir, now_us()) {
        Ok(ResetOutcome::Applied) => {
            reset_applied = true;
            eprintln!(
                "administrator credential replaced from {}",
                args.reset_file.display()
            );
        }
        Ok(ResetOutcome::None) => {}
        // A mistyped recovery file must not take a working installation
        // offline; the file is left in place and the old credential stands.
        Err(err) => eprintln!("warning: ignoring {}: {err}", args.reset_file.display()),
    }

    let credential = AdminCredential::load(&args.data_dir)?;
    let config = ApplianceConfig::load(&args.config)?;
    let model_installed = args.data_dir.join(ACTIVE_MODEL).is_file();

    let kit_id = config.identity.kit_id.clone();
    let journal = Journal::open(&args.data_dir)?;
    let state = EdgeState::new(config, credential, model_installed, journal);

    if reset_applied {
        state.record(Event::new(EventKind::CredentialReset));
    }
    state.record(
        Event::new(EventKind::Started)
            .with_detail(format!("version {}", env!("CARGO_PKG_VERSION"))),
    );
    // One write to make "when did this unit last boot" reliable; the rest
    // of the lifecycle traffic rides the periodic flush.
    state.flush_journal();

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

        let housekeeper = tokio::spawn(housekeeping(state.clone()));

        // Connect info carries the client address the login throttle keys on.
        axum::serve(
            listener,
            router(state.clone()).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown_signal())
        .await?;

        housekeeper.abort();
        state.record(Event::new(EventKind::Stopped));
        state.flush_journal();
        eprintln!("stopped");
        Ok::<(), Box<dyn Error>>(())
    })
}

/// Writes what the journal has buffered, and prunes it now and then.
///
/// Buffered events would otherwise sit in memory until something forced
/// them out; this bounds that wait to a few seconds without paying a
/// physical write per event.
async fn housekeeping(state: EdgeState) {
    let mut ticks: u64 = 0;
    let mut interval = tokio::time::interval(FLUSH_INTERVAL);
    loop {
        interval.tick().await;
        state.flush_journal();
        ticks += 1;
        if ticks % PRUNE_EVERY_TICKS == 0 {
            state.prune_journal();
        }
    }
}

/// Resolves when the supervisor asks the daemon to stop.
///
/// Both signals matter: systemd sends SIGTERM, a console sends SIGINT.
/// Catching them is what lets the journal be flushed instead of losing
/// whatever was buffered.
async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => eprintln!("cannot listen for SIGTERM: {err}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

fn provision(args: &ProvisionArgs) -> Result<(), Box<dyn Error>> {
    if !args.force && AdminCredential::load(&args.data_dir).is_ok() {
        return Err(format!(
            "{} already holds a credential; pass --force to replace it and invalidate the printed label",
            args.data_dir.display()
        )
        .into());
    }

    let secret = DeviceSecret::generate();
    AdminCredential::establish(&secret, now_us())?.save(&args.data_dir)?;

    eprintln!("credential stored in {}", args.data_dir.display());
    print_secret(&secret);
    Ok(())
}

/// Prints the secret on stdout, alone, so it can be piped into a label or
/// a QR encoder. Everything else this binary says goes to stderr.
fn print_secret(secret: &DeviceSecret) {
    eprintln!("device secret ({SECRET_ENTROPY_BITS} bits) — print it, it is not recoverable:");
    println!("{secret}");
}
