//! The edge daemon: runs the live inference pipeline on a CSI frame
//! source and serves the current estimate over a local REST API.
//!
//! The blocking intake loop (parsing → windowing → features → model →
//! wait estimation) runs on its own thread and publishes each estimate
//! into a `watch` channel; the async HTTP server only ever reads the
//! latest value. If the stream ends or fails, the server keeps running —
//! the staleness check masks the public estimate on its own.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use flow_api::{AppState, EstimateSender, estimate_channel, router};
use flow_infer::{DensityModel, LiveConfig, LivePipeline, WaitConfig};
use flow_ingest::{FrameSource, MacAddr, SenderKey, SourceConfig, parse_node_mapping};
use serde::Deserialize;

/// Per-site configuration file (JSON) — same shape as `csi-infer`.
#[derive(Debug, Deserialize)]
struct SiteConfig {
    /// Receiving node id — required for line-based inputs only.
    #[serde(default)]
    node_id: Option<String>,
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

/// Serve live density estimates from a CSI source over local REST.
#[derive(Debug, Parser)]
#[command(name = "flow-api", version)]
struct Args {
    /// Capture file, '-' for stdin, or udp://ADDR:PORT.
    #[arg(short, long)]
    input: String,

    /// Path to the exported ONNX density model.
    #[arg(short, long)]
    model: PathBuf,

    /// Path to the site configuration JSON.
    #[arg(short, long)]
    config: PathBuf,

    /// Sender mapping for UDP inputs: <node-id>=<ip[:port]>, repeatable.
    #[arg(long = "node")]
    nodes: Vec<String>,

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
    let tx_mac: Option<MacAddr> = args.tx_mac.as_deref().map(str::parse).transpose()?;

    let mut nodes: HashMap<SenderKey, String> = HashMap::new();
    for spec in &args.nodes {
        let (name, key) = parse_node_mapping(spec)?;
        nodes.insert(key, name);
    }
    let source = FrameSource::open(SourceConfig {
        input: args.input.clone(),
        node_id: site.node_id.clone(),
        nodes,
        tx_mac,
        start_ts_us: None,
    })?;

    let model = DensityModel::load(&args.model)?;
    let pipeline = LivePipeline::new(
        model,
        LiveConfig {
            window_us: site.window_us,
            hop_us: site.hop_us,
            rx_nodes: source.rx_node_ids(),
            wait: WaitConfig {
                people_per_class: site.people_per_class,
                service_rate_per_min: site.service_rate_per_min,
                smoothing_tau_s: site.smoothing_tau_s,
                hysteresis_margin: site.hysteresis_margin,
                min_confidence: site.min_confidence,
            },
        },
    )?;

    let (sender, receiver) = estimate_channel();
    std::thread::spawn(move || {
        if let Err(err) = stream_loop(source, pipeline, &sender) {
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
    mut source: FrameSource,
    mut pipeline: LivePipeline,
    sender: &EstimateSender,
) -> Result<(), Box<dyn Error>> {
    while let Some(result) = source.next_frame() {
        let frame = result?;
        if let Some(estimate) = pipeline.push(frame)? {
            // Ignore send errors: the server owning the receiver is gone,
            // so the process is shutting down anyway.
            let _ = sender.send(Some(estimate));
        }
    }
    eprintln!("stream ended — {}", source.stats_line());
    Ok(())
}
