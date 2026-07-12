//! `csi-capture`: records a labeled capture session.
//!
//! Combines the capture loop (CSI frame source → canonical session on
//! disk) with the labeling web page served on the LAN, so that frames and
//! labels land in the same session, stamped by the same edge clock.
//! Input is a capture file, stdin (live serial pipe), or the production
//! UDP intake — the latter records both RX nodes into one session.
//! `Ctrl-C` — or the end of the input stream — flushes, syncs, and seals
//! the session.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use flow_capture::{CaptureState, now_us, router};
use flow_core::SessionMeta;
use flow_ingest::session::SessionWriter;
use flow_ingest::{FrameSource, MacAddr, SenderKey, SourceConfig, parse_node_mapping};

/// Record a labeled capture session (CSI source + phone labeling page).
#[derive(Debug, Parser)]
#[command(name = "csi-capture", version)]
struct Args {
    /// Capture file, '-' for stdin, or udp://ADDR:PORT.
    #[arg(short, long)]
    input: String,

    /// Path to a SessionMeta JSON file (session_id, site, nodes,
    /// class mapping — the class descriptions shown on the buttons).
    #[arg(short, long)]
    meta: PathBuf,

    /// Root directory in which the session directory is created.
    #[arg(short, long, default_value = "data/sessions")]
    out: PathBuf,

    /// Receiving node id — required for line-based inputs only.
    #[arg(short, long)]
    node_id: Option<String>,

    /// Sender mapping for UDP inputs: <node-id>=<ip[:port]>, repeatable.
    #[arg(long = "node")]
    nodes: Vec<String>,

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
    let tx_mac: Option<MacAddr> = args.tx_mac.as_deref().map(str::parse).transpose()?;

    let mut nodes: HashMap<SenderKey, String> = HashMap::new();
    for spec in &args.nodes {
        let (name, key) = parse_node_mapping(spec)?;
        nodes.insert(key, name);
    }
    let source = FrameSource::open(SourceConfig {
        input: args.input.clone(),
        node_id: args.node_id.clone(),
        nodes,
        tx_mac,
        start_ts_us: None,
    })?;

    for node_id in source.rx_node_ids() {
        if !meta.nodes.iter().any(|n| n.node_id == node_id) {
            return Err(
                format!("node id {node_id:?} is not declared in the session metadata").into(),
            );
        }
    }

    let writer = SessionWriter::create(&args.out, &meta)?;
    let state = CaptureState::new(writer, &meta, now_us());

    let capture_state = state.clone();
    std::thread::spawn(move || {
        if let Err(err) = capture_loop(source, &capture_state) {
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

fn capture_loop(mut source: FrameSource, state: &CaptureState) -> Result<(), Box<dyn Error>> {
    while let Some(result) = source.next_frame() {
        let frame = result?;
        state.record_frame(&frame)?;
    }
    eprintln!("stream ended — {}", source.stats_line());
    Ok(())
}
