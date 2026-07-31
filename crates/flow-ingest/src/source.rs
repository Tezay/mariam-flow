//! Unified frame source for the capture and inference tools.
//!
//! One `--input` specification covers every transport:
//!
//! - `-` — lines from stdin (live serial pipe);
//! - a path — lines from a recorded capture file;
//! - `udp://ADDR:PORT` — datagrams from the sensing nodes ([`UdpSource`]).
//!
//! Line-based sources carry a single receiving node (one serial stream =
//! one RX) and reconstruct timestamps from the node clock; the UDP source
//! carries all mapped nodes and stamps frames at reception.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::time::Duration;

use flow_core::{CsiFrame, FrameError};
use thiserror::Error;

use crate::clock::now_us;
use crate::esp_csi::MacAddr;
use crate::reader::CsiReader;
use crate::timeline::Timeline;
use crate::udp::{SenderKey, SenderObservation, UdpSource};

/// Prefix selecting the UDP transport in an input specification.
pub const UDP_SCHEME: &str = "udp://";

/// Configuration of a [`FrameSource`].
#[derive(Debug, Default)]
pub struct SourceConfig {
    /// Input specification: `-`, a file path, or `udp://ADDR:PORT`.
    pub input: String,
    /// Receiving node id — required for line-based inputs, ignored for UDP.
    pub node_id: Option<String>,
    /// Sender mapping — required for UDP inputs, ignored otherwise.
    pub nodes: HashMap<SenderKey, String>,
    /// Keep only frames sensed from this transmitter MAC.
    pub tx_mac: Option<MacAddr>,
    /// Timestamp of the first frame for line-based inputs (default: now).
    pub start_ts_us: Option<u64>,
    /// How long a UDP read waits before reporting that nothing arrived.
    ///
    /// `None` blocks until a datagram comes, which is what a tool reading a
    /// stream wants. A long-running service sets it so its own periodic work
    /// does not depend on traffic arriving.
    pub read_timeout: Option<Duration>,
}

/// Failure while opening or reading a frame source.
#[derive(Debug, Error)]
pub enum SourceError {
    /// Invalid configuration for the selected transport.
    #[error("{0}")]
    Config(String),
    /// Underlying I/O failure.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A frame failed structural conversion.
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// A stream of canonical frames from any supported transport.
pub enum FrameSource {
    /// Line-based source (file or stdin): single node, node-clock
    /// timestamp reconstruction.
    Lines {
        /// The tolerant line reader.
        reader: CsiReader<Box<dyn BufRead + Send>>,
        /// Timestamp reconstruction from the node clock.
        timeline: Timeline,
        /// Receiving node of this stream.
        node_id: String,
        /// Transmitter filter.
        tx_mac: Option<MacAddr>,
        /// Frames excluded by the transmitter filter.
        filtered: u64,
    },
    /// UDP source: multi-node, reception-time stamping.
    Udp(UdpSource),
}

impl FrameSource {
    /// Opens the source described by `config`.
    ///
    /// # Errors
    ///
    /// [`SourceError::Config`] for transport/parameter mismatches, or the
    /// underlying I/O error.
    pub fn open(config: SourceConfig) -> Result<Self, SourceError> {
        if let Some(addr) = config.input.strip_prefix(UDP_SCHEME) {
            // An empty mapping is a legitimate state, not a mistake: an
            // appliance still being installed has no nodes, and the socket has
            // to be open for the ones it is about to pair to be heard.
            let mut source = UdpSource::bind(addr, config.nodes, config.tx_mac)?;
            source.set_read_timeout(config.read_timeout)?;
            return Ok(Self::Udp(source));
        }

        let node_id = config.node_id.ok_or_else(|| {
            SourceError::Config("line-based inputs require a receiving node id".into())
        })?;
        let input: Box<dyn BufRead + Send> = if config.input == "-" {
            Box::new(BufReader::new(io::stdin()))
        } else {
            Box::new(BufReader::new(File::open(&config.input)?))
        };
        Ok(Self::Lines {
            reader: CsiReader::new(input),
            timeline: Timeline::new(config.start_ts_us.unwrap_or_else(now_us)),
            node_id,
            tx_mac: config.tx_mac,
            filtered: 0,
        })
    }

    /// Receiving nodes this source can attribute frames to, sorted.
    #[must_use]
    pub fn rx_node_ids(&self) -> Vec<String> {
        match self {
            Self::Lines { node_id, .. } => vec![node_id.clone()],
            Self::Udp(source) => source.rx_node_ids(),
        }
    }

    /// Senders streaming to this source that no node mapping claims.
    ///
    /// Only a UDP source can have any: a line-based stream is attributed to
    /// the node it was recorded from, so nothing about it is unidentified.
    #[must_use]
    pub fn observations(&self) -> Vec<SenderObservation> {
        match self {
            Self::Lines { .. } => Vec::new(),
            Self::Udp(source) => source.observations(),
        }
    }

    /// Next frame, or `None` when a line-based stream ends (a UDP source
    /// never ends by itself).
    pub fn next_frame(&mut self) -> Option<Result<CsiFrame, SourceError>> {
        match self {
            Self::Udp(source) => Some(source.next_frame().map_err(Into::into)),
            Self::Lines {
                reader,
                timeline,
                node_id,
                tx_mac,
                filtered,
            } => loop {
                let raw = match reader.next()? {
                    Ok(raw) => raw,
                    Err(err) => return Some(Err(err.into())),
                };
                if let Some(wanted) = *tx_mac {
                    if raw.mac != wanted {
                        *filtered += 1;
                        continue;
                    }
                }
                let ts_us = timeline.assign(raw.local_timestamp);
                return Some(raw.to_frame(node_id.as_str(), ts_us).map_err(Into::into));
            },
        }
    }

    /// One printable line of intake statistics.
    #[must_use]
    pub fn stats_line(&self) -> String {
        match self {
            Self::Lines {
                reader, filtered, ..
            } => {
                let stats = reader.stats();
                format!(
                    "frames: {}  filtered: {}  skipped lines: {}  parse errors: {}  \
                     lost frames: {}  seq resets: {}",
                    stats.frames,
                    filtered,
                    stats.skipped_lines,
                    stats.parse_errors,
                    stats.lost_frames,
                    stats.seq_resets
                )
            }
            Self::Udp(source) => {
                let stats = source.stats();
                format!(
                    "datagrams: {}  frames: {}  unknown senders: {}  filtered: {}  \
                     skipped: {}  parse errors: {}  lost frames: {}  seq resets: {}",
                    stats.datagrams,
                    stats.frames,
                    stats.unknown_sender,
                    stats.filtered,
                    stats.skipped,
                    stats.parse_errors,
                    stats.lost_frames,
                    stats.seq_resets
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn write_capture(lines: &[String]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    fn c6_line(seq: u32, local_ts: u32) -> String {
        format!(
            "CSI_DATA,{seq},1a:2b:3c:4d:5e:6f,-52,11,-92,4,12,6,{local_ts},128,1,4,0,\"[4,3,0,1]\""
        )
    }

    #[test]
    fn line_source_reconstructs_timestamps_and_ends() {
        let file = write_capture(&[c6_line(1, 1_000), c6_line(2, 11_000)]);
        let mut source = FrameSource::open(SourceConfig {
            input: file.path().to_string_lossy().into_owned(),
            node_id: Some("rx-1".into()),
            start_ts_us: Some(1_000_000),
            ..SourceConfig::default()
        })
        .unwrap();

        assert_eq!(source.rx_node_ids(), ["rx-1"]);
        let first = source.next_frame().unwrap().unwrap();
        let second = source.next_frame().unwrap().unwrap();
        assert_eq!(first.ts_us, 1_000_000);
        assert_eq!(second.ts_us, 1_010_000);
        assert!(source.next_frame().is_none(), "file stream must end");
        assert!(source.stats_line().contains("frames: 2"));
    }

    #[test]
    fn line_source_requires_a_node_id() {
        let result = FrameSource::open(SourceConfig {
            input: "-".into(),
            ..SourceConfig::default()
        });
        assert!(matches!(result, Err(SourceError::Config(_))));
    }

    #[test]
    fn a_udp_source_opens_before_any_node_is_paired() {
        // The socket has to be listening for the nodes an installer is about
        // to pair to be heard at all.
        let source = FrameSource::open(SourceConfig {
            input: "udp://127.0.0.1:0".into(),
            ..SourceConfig::default()
        })
        .unwrap();

        assert!(source.rx_node_ids().is_empty());
        assert!(source.observations().is_empty());
    }

    #[test]
    fn udp_source_exposes_sorted_nodes() {
        let mut nodes = HashMap::new();
        nodes.insert(
            SenderKey::Ip("10.0.0.2".parse().unwrap()),
            "rx-2".to_owned(),
        );
        nodes.insert(
            SenderKey::Ip("10.0.0.1".parse().unwrap()),
            "rx-1".to_owned(),
        );
        let source = FrameSource::open(SourceConfig {
            input: "udp://127.0.0.1:0".into(),
            nodes,
            ..SourceConfig::default()
        })
        .unwrap();
        assert_eq!(source.rx_node_ids(), ["rx-1", "rx-2"]);
    }
}
