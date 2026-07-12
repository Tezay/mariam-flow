//! `csi-capture`: records a labeled capture session.
//!
//! Combines the capture loop (CSI stream → canonical session on disk)
//! with the labeling web page served on the LAN, so that frames and
//! labels land in the same session, stamped by the same edge clock.
//! `Ctrl-C` — or the end of the input stream — flushes, syncs, and seals
//! the session.

use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use flow_capture::{CaptureState, now_us, router};
use flow_core::SessionMeta;
use flow_ingest::session::SessionWriter;
use flow_ingest::{CsiReader, MacAddr, Timeline};

/// Record a labeled capture session (CSI stream + phone labeling page).
#[derive(Debug, Parser)]
#[command(name = "csi-capture", version)]
struct Args {
    /// Capture source with CSI_DATA lines, or '-' for stdin (live pipe).
    #[arg(short, long)]
    input: String,

    /// Path to a SessionMeta JSON file (session_id, site, nodes,
    /// class mapping — the class descriptions shown on the buttons).
    #[arg(short, long)]
    meta: PathBuf,

    /// Root directory in which the session directory is created.
    #[arg(short, long, default_value = "data/sessions")]
    out: PathBuf,

    /// Receiving node id for every frame of this capture.
    #[arg(short, long)]
    node_id: String,

    /// Keep only frames sensed from this transmitter MAC address.
    #[arg(long)]
    tx_mac: Option<String>,

    /// Address the labeling page is served on. LAN-wide by default so a
    /// phone on the same Wi-Fi can reach it; calibration use only.
    #[arg(long, default_value = "0.0.0.0:8088")]
    listen: String,
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
    let input: Box<dyn BufRead + Send> = if args.input == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(&args.input)?))
    };

    let writer = SessionWriter::create(&args.out, &meta)?;
    let state = CaptureState::new(writer, &meta, now_us());

    let capture_state = state.clone();
    let node_id = args.node_id.clone();
    std::thread::spawn(move || {
        if let Err(err) = capture_loop(input, &node_id, tx_mac, &capture_state) {
            eprintln!("capture stream stopped: {err}");
        }
        capture_state.mark_stream_ended();
    });

    let listen = args.listen.clone();
    let server_state = state.clone();
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&listen).await?;
        eprintln!("labeling page on http://{listen} — Ctrl-C seals the session");
        axum::serve(listener, router(server_state))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        Ok::<(), Box<dyn Error>>(())
    })?;

    match state.finalize()? {
        Some(summary) => eprintln!(
            "session sealed: {} ({} frames, {} labels)",
            summary.path.display(),
            summary.frames,
            summary.labels
        ),
        None => eprintln!("session was already sealed"),
    }
    Ok(())
}

fn capture_loop(
    input: Box<dyn BufRead + Send>,
    node_id: &str,
    tx_mac: Option<MacAddr>,
    state: &CaptureState,
) -> Result<(), Box<dyn Error>> {
    let mut timeline = Timeline::new(now_us());
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
        state.record_frame(&frame)?;
    }
    let stats = reader.stats();
    eprintln!(
        "stream ended — frames: {}  skipped: {}  parse errors: {}  lost: {}",
        stats.frames, stats.skipped_lines, stats.parse_errors, stats.lost_frames
    );
    Ok(())
}
