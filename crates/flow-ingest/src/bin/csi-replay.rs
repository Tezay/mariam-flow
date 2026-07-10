//! Replays an esp-csi capture into a canonical session directory.
//!
//! Input is any stream of `CSI_DATA` lines: a recorded serial capture
//! file, or standard input for a live pipe
//! (`cat /dev/ttyUSB0 | csi-replay -i - …`).

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use flow_core::SessionMeta;
use flow_ingest::{CsiReader, MacAddr, SessionWriter, Timeline};

/// Replay an esp-csi capture (file or stdin) into a canonical session.
#[derive(Debug, Parser)]
#[command(name = "csi-replay", version)]
struct Args {
    /// Capture file containing CSI_DATA lines, or '-' for stdin
    /// (live pipe from a serial port).
    #[arg(short, long)]
    input: String,

    /// Path to a SessionMeta JSON file (defines session_id, site, nodes,
    /// class mapping…).
    #[arg(short, long)]
    meta: PathBuf,

    /// Root directory in which the session directory is created.
    #[arg(short, long, default_value = "data/sessions")]
    out: PathBuf,

    /// Receiving node id for every frame of this capture; must be declared
    /// in the session metadata.
    #[arg(short, long)]
    node_id: String,

    /// Keep only frames sensed from this transmitter MAC address
    /// (recommended: excludes ambient Wi-Fi traffic).
    #[arg(long)]
    tx_mac: Option<String>,

    /// Timestamp assigned to the first frame, in µs since the Unix epoch
    /// (default: now). Inter-frame timing is preserved from the node clock.
    #[arg(long)]
    start_ts_us: Option<u64>,
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
    let meta: SessionMeta = serde_json::from_str(&fs::read_to_string(&args.meta)?)?;
    if !meta.nodes.iter().any(|n| n.node_id == args.node_id) {
        return Err(format!(
            "node id {:?} is not declared in the session metadata",
            args.node_id
        )
        .into());
    }
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

    let mut writer = SessionWriter::create(&args.out, &meta)?;
    let mut reader = CsiReader::new(input);
    let mut timeline = Timeline::new(base_us);
    let mut filtered_out: u64 = 0;

    for result in reader.by_ref() {
        let raw = result?;
        if let Some(wanted) = tx_mac {
            if raw.mac != wanted {
                filtered_out += 1;
                continue;
            }
        }
        let ts_us = timeline.assign(raw.local_timestamp);
        let frame = raw.to_frame(args.node_id.as_str(), ts_us)?;
        writer.write_frame(&frame)?;
    }

    let stats = reader.stats();
    let summary = writer.finalize()?;
    println!("session sealed: {}", summary.path.display());
    println!("  frames written : {}", summary.frames);
    println!("  filtered out   : {filtered_out}");
    println!("  skipped lines  : {}", stats.skipped_lines);
    println!("  parse errors   : {}", stats.parse_errors);
    println!("  lost frames    : {}", stats.lost_frames);
    println!("  seq resets     : {}", stats.seq_resets);
    Ok(())
}

fn now_us() -> Result<u64, Box<dyn Error>> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH)?;
    Ok(u64::try_from(elapsed.as_micros())?)
}
