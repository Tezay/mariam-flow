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
    ACTIVE_MODEL, AdminCredential, ApplianceConfig, DeviceSecret, EdgeState, Event, EventKind,
    Journal, LiveOptions, Phase, ResetOutcome, SECRET_ENTROPY_BITS, apply_pending_reset, now_us,
    router, spawn_pipeline,
};

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

    /// Where the daemon reads CSI frames from.
    ///
    /// The receivers stream to this socket in production. A capture file or
    /// `-` replays a recorded session instead, which is how the whole chain
    /// is exercised on a machine with no sensors attached.
    #[arg(long, default_value = "udp://0.0.0.0:5566")]
    input: String,

    /// Receiving node id, required only when replaying a line-based input.
    #[arg(long)]
    node_id: Option<String>,

    /// Keep only frames sensed from this transmitter MAC address.
    #[arg(long)]
    tx_mac: Option<String>,

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

    /// Also write this unit's factory configuration here.
    #[arg(short, long, requires_all = ["kit_id", "ap_ssid", "ap_passphrase"])]
    config: Option<PathBuf>,

    /// Kit identifier.
    #[arg(long, requires = "config")]
    kit_id: Option<String>,

    /// Network name the sensor access point announces.
    #[arg(long, requires = "config")]
    ap_ssid: Option<String>,

    /// WPA2 passphrase of the sensor access point, 8..=63 characters.
    ///
    /// Supplied rather than generated: the nodes are flashed with it before
    /// the appliance is provisioned, and one invented here would leave them
    /// unable to join.
    #[arg(long, requires = "config")]
    ap_passphrase: Option<String>,

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
    let config_for_pipeline = config.clone();
    let journal = Journal::open(&args.data_dir)?;
    let state = EdgeState::new(
        config,
        args.config.clone(),
        args.data_dir.clone(),
        credential,
        model_installed,
        journal,
    );

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

    // Only an unreadable stream is reported here. Having nothing to estimate
    // with is a stage of an installation, and the readiness the dashboard
    // already shows says which step is outstanding.
    let tx_mac = args.tx_mac.as_deref().map(str::parse).transpose()?;
    let options = LiveOptions {
        input: args.input.clone(),
        node_id: args.node_id.clone(),
        tx_mac,
    };
    match spawn_pipeline(state.clone(), &config_for_pipeline, &args.data_dir, options) {
        Ok(()) => eprintln!("reading {}", args.input),
        Err(err) => eprintln!("stream unavailable: {err}"),
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
        .with_graceful_shutdown(shutdown_signal(state.clone()))
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
    let mut clock = ClockWatch::new();
    loop {
        interval.tick().await;
        if let Some(step) = clock.stepped() {
            state.record(Event::new(EventKind::ClockStepped).with_detail(step));
        }
        state.flush_journal();
        ticks += 1;
        if ticks % PRUNE_EVERY_TICKS == 0 {
            state.prune_journal();
        }
    }
}

/// Watches the wall clock against a monotonic one.
///
/// An appliance installed offline boots with whatever time it kept, and the
/// clock jumps the moment an uplink lets it be corrected. Rows are ordered by
/// their identifier so the journal still reads in order, but their times
/// contradict each other across the jump — which looks like corruption unless
/// the jump itself is recorded.
struct ClockWatch {
    wall_us: u64,
    monotonic: std::time::Instant,
}

/// A wall-clock drift larger than this, over one tick, is a step rather than
/// the ordinary imprecision of a timer.
const CLOCK_STEP_US: i128 = 5 * 1_000_000;

impl ClockWatch {
    fn new() -> Self {
        Self {
            wall_us: now_us(),
            monotonic: std::time::Instant::now(),
        }
    }

    /// How far the clock jumped since the last call, phrased, if it did.
    fn stepped(&mut self) -> Option<String> {
        let wall = now_us();
        let monotonic = std::time::Instant::now();
        let elapsed = i128::try_from(monotonic.duration_since(self.monotonic).as_micros()).ok()?;
        let moved = i128::from(wall) - i128::from(self.wall_us);
        self.wall_us = wall;
        self.monotonic = monotonic;

        let step = moved - elapsed;
        if step.abs() < CLOCK_STEP_US {
            return None;
        }
        let seconds = step / 1_000_000;
        Some(format!(
            "{}{} s",
            if seconds > 0 { "+" } else { "" },
            seconds
        ))
    }
}

/// Resolves when the supervisor asks the daemon to stop.
///
/// Both signals matter: systemd sends SIGTERM, a console sends SIGINT.
/// Catching them is what lets the journal be flushed instead of losing
/// whatever was buffered.
async fn shutdown_signal(state: EdgeState) {
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

    // Before returning, so that responses which never end on their own are
    // already finishing when axum starts waiting for connections to close.
    state.begin_shutdown();
}

fn provision(args: &ProvisionArgs) -> Result<(), Box<dyn Error>> {
    if !args.force && AdminCredential::load(&args.data_dir).is_ok() {
        return Err(format!(
            "{} already holds a credential; pass --force to replace it and invalidate the printed label",
            args.data_dir.display()
        )
        .into());
    }
    if let Some(path) = &args.config
        && !args.force
        && path.exists()
    {
        return Err(format!(
            "{} already exists; pass --force to replace it",
            path.display()
        )
        .into());
    }

    // Written before the credential: a refused configuration must not leave a
    // unit holding a secret that has already been printed and cannot be read
    // back.
    if let Some(path) = &args.config {
        let (Some(kit_id), Some(ssid), Some(passphrase)) =
            (&args.kit_id, &args.ap_ssid, &args.ap_passphrase)
        else {
            return Err("--config needs --kit-id, --ap-ssid and --ap-passphrase".into());
        };
        ApplianceConfig::factory(kit_id, ssid, passphrase).save(path)?;
        eprintln!("factory configuration written to {}", path.display());
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
