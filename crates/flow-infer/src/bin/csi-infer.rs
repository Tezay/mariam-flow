//! Runs the full live inference chain on a CSI frame source:
//! intake → windowing → features → ONNX model → wait estimate.
//!
//! Input is a recorded capture file, stdin for a live serial pipe
//! (`cat /dev/ttyUSB0 | csi-infer -i - …`), or the production UDP intake
//! (`csi-infer -i udp://0.0.0.0:5566 --node rx-1=192.168.4.11 …`).
//! Estimates are printed as human-readable lines, or NDJSON with `--json`.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use flow_infer::{DensityModel, LiveConfig, LivePipeline, WaitConfig, WaitEstimate};
use flow_ingest::{FrameSource, MacAddr, SenderKey, SourceConfig, parse_node_mapping};
use serde::Deserialize;

/// Per-site configuration file (JSON).
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

/// Run live density inference over a CSI capture, pipe, or UDP intake.
#[derive(Debug, Parser)]
#[command(name = "csi-infer", version)]
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

    /// Keep only frames sensed from this transmitter MAC address.
    #[arg(long)]
    tx_mac: Option<String>,

    /// Timestamp assigned to the first frame of a line-based input, in µs
    /// since the Unix epoch (default: now).
    #[arg(long)]
    start_ts_us: Option<u64>,

    /// Print estimates as NDJSON instead of human-readable lines.
    #[arg(long)]
    json: bool,
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

fn parse_nodes(specs: &[String]) -> Result<HashMap<SenderKey, String>, Box<dyn Error>> {
    let mut nodes = HashMap::new();
    for spec in specs {
        let (name, key) = parse_node_mapping(spec)?;
        nodes.insert(key, name);
    }
    Ok(nodes)
}

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let site: SiteConfig = serde_json::from_str(&fs::read_to_string(&args.config)?)?;
    let tx_mac: Option<MacAddr> = args.tx_mac.as_deref().map(str::parse).transpose()?;

    let mut source = FrameSource::open(SourceConfig {
        input: args.input.clone(),
        node_id: site.node_id.clone(),
        nodes: parse_nodes(&args.nodes)?,
        tx_mac,
        start_ts_us: args.start_ts_us,
    })?;

    let model = DensityModel::load(&args.model)?;
    let mut pipeline = LivePipeline::new(
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

    while let Some(result) = source.next_frame() {
        let frame = result?;
        if let Some(estimate) = pipeline.push(frame)? {
            match print_estimate(&estimate, args.json) {
                Ok(()) => {}
                // Downstream consumer closed the pipe (e.g. `| head`):
                // stop cleanly instead of panicking on the next write.
                Err(err) if err.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
                Err(err) => return Err(err.into()),
            }
        }
    }

    let stats = pipeline.stats();
    eprintln!("{}", source.stats_line());
    eprintln!(
        "estimates: {}  incomplete windows: {}",
        stats.estimates, stats.incomplete_windows
    );
    Ok(())
}

fn print_estimate(estimate: &WaitEstimate, json: bool) -> io::Result<()> {
    use std::io::Write;

    let mut out = io::stdout().lock();
    if json {
        writeln!(
            out,
            concat!(
                r#"{{"ts_us":{},"wait_min":{:.2},"people":{:.2},"level":{:.3},"#,
                r#""class":{},"confidence":{:.3},"reliable":{}}}"#
            ),
            estimate.ts_us,
            estimate.wait_minutes,
            estimate.people,
            estimate.level,
            estimate.display_class.as_u8(),
            estimate.confidence,
            estimate.reliable,
        )
    } else {
        let reliability = if estimate.reliable {
            ""
        } else {
            "  [unreliable]"
        };
        writeln!(
            out,
            "wait ~{:.1} min  people {:.1}  class {}  confidence {:.2}{}",
            estimate.wait_minutes,
            estimate.people,
            estimate.display_class,
            estimate.confidence,
            reliability,
        )
    }
}
