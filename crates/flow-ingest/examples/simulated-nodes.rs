//! Streams CSI to an appliance from several simulated receivers.
//!
//! Development aid for running the edge on a laptop with no sensors attached.
//! The signal is a fixed pattern, so nothing it produces means anything about
//! a queue — what it exercises is intake, pairing, stream health and capture.

use std::error::Error;
use std::net::{SocketAddr, UdpSocket};
use std::thread::sleep;
use std::time::Duration;

use clap::Parser;
use flow_ingest::parse_line;

/// Stream simulated CSI frames to a running appliance.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Source address of one receiver; repeat for several.
    ///
    /// The appliance tells receivers apart by the address a datagram comes
    /// from, so two receivers need two addresses this host actually holds —
    /// its loopback and its LAN address, for instance.
    #[arg(long = "from", required = true)]
    sources: Vec<String>,

    /// Port the appliance reads on.
    #[arg(long, default_value_t = 5566)]
    port: u16,

    /// Transmitter every receiver reports having heard.
    #[arg(long, default_value = "1a:2b:3c:4d:5e:6f")]
    tx_mac: String,

    /// Frames per second, per receiver.
    #[arg(long, default_value_t = 40)]
    rate: u32,
}

fn frame(tx_mac: &str, seq: u32, local_us: u32) -> String {
    format!("CSI_DATA,{seq},{tx_mac},-52,11,-92,4,12,6,{local_us},128,1,4,0,\"[4,3,0,1]\"")
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Checked against the crate's own parser rather than trusted: a format
    // this example emits but the appliance cannot read would look like a
    // network fault for as long as it took to find.
    parse_line(&frame(&args.tx_mac, 0, 0))?;

    let mut receivers = Vec::new();
    for source in &args.sources {
        let socket = UdpSocket::bind(format!("{source}:0"))?;
        let target: SocketAddr = format!("{source}:{}", args.port).parse()?;
        receivers.push((socket, target));
    }
    eprintln!(
        "streaming {} frame(s)/s from {} receiver(s) to port {}",
        args.rate,
        receivers.len(),
        args.port
    );

    let step_us = 1_000_000 / args.rate;
    let period = Duration::from_micros(u64::from(step_us));
    let (mut seq, mut local_us) = (0u32, 183_920_121u32);
    loop {
        let line = frame(&args.tx_mac, seq, local_us);
        for (socket, target) in &receivers {
            socket.send_to(line.as_bytes(), target)?;
        }
        seq = seq.wrapping_add(1);
        local_us = local_us.wrapping_add(step_us);
        sleep(period);
    }
}
