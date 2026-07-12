//! UDP intake: the production transport from sensing nodes to the edge.
//!
//! Framing is one `CSI_DATA` text line per datagram (ADR 0005/0007): the
//! firmware sends exactly what it prints on serial. The **receiving node's
//! identity is the datagram's source address** — the MAC inside the line
//! identifies the *transmitter* of the sensed packet, never the receiver.
//! Senders are mapped to node ids explicitly; unknown senders are counted
//! and dropped.
//!
//! Frames are timestamped at reception by the edge clock. Because both RX
//! nodes land on one socket stamped by one clock, the merged stream is
//! ordered by construction — no reordering buffer. A monotonic clamp
//! guards against system-clock steps (NTP).

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use flow_core::CsiFrame;

use crate::clock::now_us;
use crate::esp_csi::{MacAddr, ParseError, parse_line};

const DATAGRAM_BUF: usize = 8192;

/// How a configured sender is matched against datagram sources.
///
/// Production nodes are identified by IP (their source port is ephemeral);
/// an explicit `ip:port` key allows advanced setups — and tests — to
/// distinguish several senders sharing one IP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SenderKey {
    /// Match on the source IP address.
    Ip(IpAddr),
    /// Match on the exact source address (IP and port).
    Sock(SocketAddr),
}

/// Parses a `name=ip` or `name=ip:port` node mapping argument.
///
/// # Errors
///
/// A human-readable message when the syntax or address is invalid.
pub fn parse_node_mapping(text: &str) -> Result<(String, SenderKey), String> {
    let (name, addr) = text
        .split_once('=')
        .ok_or_else(|| format!("expected <node-id>=<ip[:port]>, got {text:?}"))?;
    if name.is_empty() {
        return Err(format!("empty node id in {text:?}"));
    }
    if let Ok(sock) = addr.parse::<SocketAddr>() {
        return Ok((name.to_owned(), SenderKey::Sock(sock)));
    }
    match addr.parse::<IpAddr>() {
        Ok(ip) => Ok((name.to_owned(), SenderKey::Ip(ip))),
        Err(_) => Err(format!("invalid address {addr:?} in {text:?}")),
    }
}

/// Counters accumulated by a [`UdpSource`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UdpStats {
    /// Datagrams received.
    pub datagrams: u64,
    /// Frames accepted and yielded.
    pub frames: u64,
    /// Datagrams from unmapped senders.
    pub unknown_sender: u64,
    /// Datagrams without the `CSI_DATA` marker.
    pub skipped: u64,
    /// Malformed `CSI_DATA` lines.
    pub parse_errors: u64,
    /// Frames excluded by the transmitter-MAC filter.
    pub filtered: u64,
    /// Frames inferred lost from per-node sequence gaps.
    pub lost_frames: u64,
    /// Per-node sequence resets (node reboots).
    pub seq_resets: u64,
}

/// Blocking UDP frame source. See the module documentation.
#[derive(Debug)]
pub struct UdpSource {
    socket: UdpSocket,
    nodes: HashMap<SenderKey, String>,
    tx_mac: Option<MacAddr>,
    stats: UdpStats,
    last_seq: HashMap<String, u32>,
    last_ts_us: u64,
    buf: Vec<u8>,
}

impl UdpSource {
    /// Binds the intake socket and registers the sender mapping.
    ///
    /// # Errors
    ///
    /// Socket binding failures, or an empty mapping (rejected: every frame
    /// would be dropped).
    pub fn bind(
        addr: impl ToSocketAddrs,
        nodes: HashMap<SenderKey, String>,
        tx_mac: Option<MacAddr>,
    ) -> io::Result<Self> {
        if nodes.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "UDP intake needs at least one node mapping",
            ));
        }
        Ok(Self {
            socket: UdpSocket::bind(addr)?,
            nodes,
            tx_mac,
            stats: UdpStats::default(),
            last_seq: HashMap::new(),
            last_ts_us: 0,
            buf: vec![0u8; DATAGRAM_BUF],
        })
    }

    /// Address the socket is bound to.
    ///
    /// # Errors
    ///
    /// The underlying socket error.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Sets a receive timeout (mainly for tests); `None` blocks forever.
    ///
    /// # Errors
    ///
    /// The underlying socket error.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.socket.set_read_timeout(timeout)
    }

    /// Mapped receiving nodes, in stable (sorted) order.
    #[must_use]
    pub fn rx_node_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.nodes.values().cloned().collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Counters accumulated so far.
    #[must_use]
    pub fn stats(&self) -> UdpStats {
        self.stats
    }

    /// Blocks until the next valid frame from a mapped sender.
    ///
    /// Robustness policy mirrors the line reader: unknown senders,
    /// non-frame datagrams and malformed lines are counted and skipped,
    /// never fatal. The returned frame carries an edge-assigned,
    /// monotonically non-decreasing timestamp.
    ///
    /// # Errors
    ///
    /// Only socket-level I/O errors (including a configured timeout).
    pub fn next_frame(&mut self) -> io::Result<CsiFrame> {
        loop {
            let (len, from) = self.socket.recv_from(&mut self.buf)?;
            self.stats.datagrams += 1;

            let node_id = match self.identify(from) {
                Some(node_id) => node_id,
                None => {
                    self.stats.unknown_sender += 1;
                    continue;
                }
            };
            let text = String::from_utf8_lossy(&self.buf[..len]);
            let raw = match parse_line(&text) {
                Ok(raw) => raw,
                Err(ParseError::NotCsiData) => {
                    self.stats.skipped += 1;
                    continue;
                }
                Err(_) => {
                    self.stats.parse_errors += 1;
                    continue;
                }
            };
            if let Some(wanted) = self.tx_mac {
                if raw.mac != wanted {
                    self.stats.filtered += 1;
                    continue;
                }
            }
            if let Some(previous) = self.last_seq.insert(node_id.clone(), raw.seq) {
                if raw.seq > previous {
                    self.stats.lost_frames += u64::from(raw.seq - previous - 1);
                } else {
                    self.stats.seq_resets += 1;
                }
            }
            // Edge reception time, clamped monotonic against clock steps.
            self.last_ts_us = now_us().max(self.last_ts_us);
            match raw.to_frame(node_id.as_str(), self.last_ts_us) {
                Ok(frame) => {
                    self.stats.frames += 1;
                    return Ok(frame);
                }
                Err(_) => {
                    self.stats.parse_errors += 1;
                }
            }
        }
    }

    fn identify(&self, from: SocketAddr) -> Option<String> {
        self.nodes
            .get(&SenderKey::Sock(from))
            .or_else(|| self.nodes.get(&SenderKey::Ip(from.ip())))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sender() -> UdpSocket {
        UdpSocket::bind("127.0.0.1:0").unwrap()
    }

    fn c6_line(seq: u32) -> String {
        format!("CSI_DATA,{seq},1a:2b:3c:4d:5e:6f,-52,11,-92,4,12,6,1000,128,1,4,0,\"[4,3,0,1]\"")
    }

    fn source_for(senders: &[(&UdpSocket, &str)]) -> UdpSource {
        let mut nodes = HashMap::new();
        for (socket, name) in senders {
            nodes.insert(
                SenderKey::Sock(socket.local_addr().unwrap()),
                (*name).to_owned(),
            );
        }
        let source = UdpSource::bind("127.0.0.1:0", nodes, None).unwrap();
        source
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        source
    }

    #[test]
    fn frames_are_attributed_to_their_sender() {
        let a = sender();
        let b = sender();
        let mut source = source_for(&[(&a, "rx-1"), (&b, "rx-2")]);
        let target = source.local_addr().unwrap();

        a.send_to(c6_line(1).as_bytes(), target).unwrap();
        b.send_to(c6_line(10).as_bytes(), target).unwrap();
        a.send_to(c6_line(2).as_bytes(), target).unwrap();

        let first = source.next_frame().unwrap();
        let second = source.next_frame().unwrap();
        let third = source.next_frame().unwrap();
        assert_eq!(first.node_id, "rx-1");
        assert_eq!(second.node_id, "rx-2");
        assert_eq!(third.node_id, "rx-1");
        // One clock, stamped at reception: ordered by construction.
        assert!(first.ts_us <= second.ts_us && second.ts_us <= third.ts_us);
        assert_eq!(source.stats().frames, 3);
        assert_eq!(source.stats().lost_frames, 0);
    }

    #[test]
    fn unknown_senders_are_counted_and_dropped() {
        let known = sender();
        let stranger = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let target = source.local_addr().unwrap();

        stranger.send_to(c6_line(1).as_bytes(), target).unwrap();
        known.send_to(c6_line(1).as_bytes(), target).unwrap();

        let frame = source.next_frame().unwrap();
        assert_eq!(frame.node_id, "rx-1");
        assert_eq!(source.stats().unknown_sender, 1);
    }

    #[test]
    fn per_node_sequence_gaps_are_tracked_independently() {
        let a = sender();
        let b = sender();
        let mut source = source_for(&[(&a, "rx-1"), (&b, "rx-2")]);
        let target = source.local_addr().unwrap();

        a.send_to(c6_line(1).as_bytes(), target).unwrap();
        b.send_to(c6_line(100).as_bytes(), target).unwrap();
        a.send_to(c6_line(4).as_bytes(), target).unwrap(); // 2 lost
        b.send_to(c6_line(101).as_bytes(), target).unwrap();

        for _ in 0..4 {
            source.next_frame().unwrap();
        }
        assert_eq!(source.stats().lost_frames, 2);
        assert_eq!(source.stats().seq_resets, 0);
    }

    #[test]
    fn noise_and_malformed_datagrams_are_not_fatal() {
        let node = sender();
        let mut source = source_for(&[(&node, "rx-1")]);
        let target = source.local_addr().unwrap();

        node.send_to(b"boot log line", target).unwrap();
        node.send_to(b"CSI_DATA,oops", target).unwrap();
        node.send_to(&[0xff, 0xfe, 0x80], target).unwrap();
        node.send_to(c6_line(1).as_bytes(), target).unwrap();

        let frame = source.next_frame().unwrap();
        assert_eq!(frame.node_id, "rx-1");
        let stats = source.stats();
        assert_eq!(stats.skipped, 2); // boot log + binary noise
        assert_eq!(stats.parse_errors, 1);
    }

    #[test]
    fn transmitter_mac_filter_applies() {
        let node = sender();
        let mut nodes = HashMap::new();
        nodes.insert(SenderKey::Sock(node.local_addr().unwrap()), "rx-1".into());
        let wanted: MacAddr = "1a:2b:3c:4d:5e:6f".parse().unwrap();
        let mut source = UdpSource::bind("127.0.0.1:0", nodes, Some(wanted)).unwrap();
        source
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let target = source.local_addr().unwrap();

        let foreign = "CSI_DATA,9,ff:ee:dd:cc:bb:aa,-70,11,-92,4,12,6,1000,128,1,4,0,\"[1,1,1,1]\"";
        node.send_to(foreign.as_bytes(), target).unwrap();
        node.send_to(c6_line(1).as_bytes(), target).unwrap();

        source.next_frame().unwrap();
        assert_eq!(source.stats().filtered, 1);
    }

    #[test]
    fn empty_mapping_is_rejected() {
        let error = UdpSource::bind("127.0.0.1:0", HashMap::new(), None).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn mapping_parser_accepts_ip_and_socket_forms() {
        let (name, key) = parse_node_mapping("rx-1=192.168.4.11").unwrap();
        assert_eq!(name, "rx-1");
        assert_eq!(key, SenderKey::Ip("192.168.4.11".parse().unwrap()));

        let (_, key) = parse_node_mapping("rx-2=127.0.0.1:4000").unwrap();
        assert_eq!(key, SenderKey::Sock("127.0.0.1:4000".parse().unwrap()));

        assert!(parse_node_mapping("rx-1").is_err());
        assert!(parse_node_mapping("=1.2.3.4").is_err());
        assert!(parse_node_mapping("rx-1=not-an-ip").is_err());
    }
}
