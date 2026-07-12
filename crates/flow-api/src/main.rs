//! The edge daemon: runs the live inference pipeline on a CSI stream and
//! serves the current estimate over a local REST API.
//!
//! The blocking stream loop (parsing → windowing → features → model →
//! wait estimation) runs on its own thread and publishes each estimate
//! into a `watch` channel; the async HTTP server only ever reads the
//! latest value. If the stream ends or fails, the server keeps running —
//! the staleness check masks the public estimate on its own.

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use flow_api::{AppState, EstimateSender, estimate_channel, router};
use flow_infer::{DensityModel, LiveConfig, LivePipeline, WaitConfig};
use flow_ingest::{CsiReader, MacAddr, Timeline};
use serde::Deserialize;

/// Per-site configuration file (JSON) — same shape as `csi-infer`.
#[derive(Debug, Deserialize)]
struct SiteConfig {
    node_id: String,
    people_per_class: [f32; 4],
    service_rate_per_min: f32,
    smoothing_tau_s: f32,
    hysteresis_margin: f32,
    min_confidence: f32,
    #[serde(default = "default_window_us")]
    window_us: u64,
    #[serde(default = "default_hop_us")]
    hop_us: u64,
}

fn default_window_us() -> u64 {
    5_000_000
}

fn default_hop_us() -> u64 {
    1_000_000
}

/// Serve live density estimates from a CSI stream over local REST.
#[derive(Debug, Parser)]
#[command(name = "flow-api", version)]
struct Args {
    /// Capture file containing CSI_DATA lines, or '-' for stdin.
    #[arg(short, long)]
    input: String,

    /// Path to the exported ONNX density model.
    #[arg(short, long)]
    model: PathBuf,

    /// Path to the site configuration JSON.
    #[arg(short, long)]
    config: PathBuf,

    /// Address to serve on. Local by default: the API is the on-site
    /// surface, never exposed directly to the internet.
    #[arg(long, default_value = "127.0.0.1:8080")]
    listen: String,

    /// Keep only frames sensed from this transmitter MAC address.
    #[arg(long)]
    tx_mac: Option<String>,

    /// Mask the public estimate once it is older than this many seconds.
    /// 0 disables the staleness check (replay/debugging only).
    #[arg(long, default_value_t = 15)]
    max_age_s: u64,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let site: SiteConfig = serde_json::from_str(&fs::read_to_string(&args.config)?)?;
    let model = DensityModel::load(&args.model)?;
    let pipeline = LivePipeline::new(
        model,
        LiveConfig {
            window_us: site.window_us,
            hop_us: site.hop_us,
            rx_nodes: vec![site.node_id.clone()],
            wait: WaitConfig {
                people_per_class: site.people_per_class,
                service_rate_per_min: site.service_rate_per_min,
                smoothing_tau_s: site.smoothing_tau_s,
                hysteresis_margin: site.hysteresis_margin,
                min_confidence: site.min_confidence,
            },
        },
    )?;

    let tx_mac: Option<MacAddr> = args.tx_mac.as_deref().map(str::parse).transpose()?;
    let input: Box<dyn BufRead + Send> = if args.input == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(&args.input)?))
    };

    let (sender, receiver) = estimate_channel();
    let node_id = site.node_id.clone();
    std::thread::spawn(move || {
        if let Err(err) = stream_loop(input, &node_id, tx_mac, pipeline, &sender) {
            eprintln!("stream stopped: {err}");
        }
    });

    let max_age_us = (args.max_age_s > 0).then(|| args.max_age_s * 1_000_000);
    let state = AppState::new(receiver, max_age_us);
    let listen = args.listen.clone();

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&listen).await?;
        eprintln!("serving on http://{listen}");
        axum::serve(listener, router(state)).await?;
        Ok::<(), Box<dyn Error>>(())
    })?;
    Ok(())
}

fn stream_loop(
    input: Box<dyn BufRead + Send>,
    node_id: &str,
    tx_mac: Option<MacAddr>,
    mut pipeline: LivePipeline,
    sender: &EstimateSender,
) -> Result<(), Box<dyn Error>> {
    let base_us = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros())?;
    let mut timeline = Timeline::new(base_us);
    let mut reader = CsiReader::new(input);
    for result in reader.by_ref() {
        let raw = result?;
        if let Some(wanted) = tx_mac {
            if raw.mac != wanted {
                continue;
            }
        }
        let ts_us = timeline.assign(raw.local_timestamp);
        let frame = raw.to_frame(node_id, ts_us)?;
        if let Some(estimate) = pipeline.push(frame)? {
            // Ignore send errors: the server owning the receiver is gone,
            // so the process is shutting down anyway.
            let _ = sender.send(Some(estimate));
        }
    }
    let stats = pipeline.stats();
    eprintln!(
        "stream ended — frames: {}  estimates: {}  incomplete windows: {}",
        stats.frames, stats.estimates, stats.incomplete_windows
    );
    Ok(())
}
