//! Runs the full live inference chain on a stream of `CSI_DATA` lines:
//! parsing → windowing → features → ONNX model → wait estimate.
//!
//! Input is a recorded capture file or stdin for a live pipe
//! (`cat /dev/ttyUSB0 | csi-infer -i - …`). Estimates are printed as
//! human-readable lines, or NDJSON with `--json`.

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use flow_infer::{DensityModel, LiveConfig, LivePipeline, WaitConfig, WaitEstimate};
use flow_ingest::{CsiReader, MacAddr, Timeline};
use serde::Deserialize;

/// Per-site configuration file (JSON).
#[derive(Debug, Deserialize)]
struct SiteConfig {
    /// Receiving node id of this capture stream.
    node_id: String,
    /// Calibrated people count per density class.
    people_per_class: [f32; 4],
    /// Service rate λ, people per minute.
    service_rate_per_min: f32,
    /// Smoothing time constant, seconds.
    smoothing_tau_s: f32,
    /// Hysteresis half-width on the 0–3 level scale.
    hysteresis_margin: f32,
    /// Confidence threshold below which estimates are unreliable.
    min_confidence: f32,
    /// Window duration in µs (must match training). Defaults to 5 s.
    #[serde(default = "default_window_us")]
    window_us: u64,
    /// Emission period in µs. Defaults to 1 s.
    #[serde(default = "default_hop_us")]
    hop_us: u64,
}

fn default_window_us() -> u64 {
    5_000_000
}

fn default_hop_us() -> u64 {
    1_000_000
}

/// Run live density inference over a CSI capture or pipe.
#[derive(Debug, Parser)]
#[command(name = "csi-infer", version)]
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

    /// Keep only frames sensed from this transmitter MAC address.
    #[arg(long)]
    tx_mac: Option<String>,

    /// Timestamp assigned to the first frame, µs since the Unix epoch
    /// (default: now).
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

fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let site: SiteConfig = serde_json::from_str(&fs::read_to_string(&args.config)?)?;
    let model = DensityModel::load(&args.model)?;
    let mut pipeline = LivePipeline::new(
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
    let base_us = match args.start_ts_us {
        Some(ts) => ts,
        None => now_us()?,
    };
    let input: Box<dyn BufRead> = if args.input == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(&args.input)?))
    };

    let mut reader = CsiReader::new(input);
    let mut timeline = Timeline::new(base_us);
    for result in reader.by_ref() {
        let raw = result?;
        if let Some(wanted) = tx_mac {
            if raw.mac != wanted {
                continue;
            }
        }
        let ts_us = timeline.assign(raw.local_timestamp);
        let frame = raw.to_frame(site.node_id.as_str(), ts_us)?;
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
    eprintln!(
        "frames: {}  estimates: {}  incomplete windows: {}",
        stats.frames, stats.estimates, stats.incomplete_windows
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
                r#"{{"wait_min":{:.2},"people":{:.2},"level":{:.3},"#,
                r#""class":{},"confidence":{:.3},"reliable":{}}}"#
            ),
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

fn now_us() -> Result<u64, Box<dyn Error>> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(u64::try_from(elapsed.as_micros())?)
}
