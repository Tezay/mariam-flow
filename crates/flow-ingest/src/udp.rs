//! UDP intake: the production transport from sensing nodes to the edge.
//!
//! Framing is one `CSI_DATA` text line per datagram (ADR 0005/0007): the
//! firmware sends exactly what it prints on serial. The **receiving node's
//! identity is the datagram's source address** — the MAC inside the line
//! identifies the *transmitter* of the sensed packet, never the receiver.
//! Senders are mapped to node ids explicitly; their frames are attributed by
//! that mapping. Unmapped senders yield no frames but are *recorded*, which
//! is how a node still to be paired is found.
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
    /// Datagrams from unmapped senders left unrecorded, the observation
    /// table being full.
    pub observations_dropped: u64,
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

/// Senders kept under observation at once.
///
/// Anyone able to reach the intake socket can create an entry, so this is a
/// bound on attacker-controlled memory, not a tuning knob. An installation
/// has a handful of nodes; a hundred distinct sources means something other
/// than sensing is happening.
const MAX_OBSERVED_SENDERS: usize = 16;

/// Distinct transmitter MACs remembered per observed sender.
const MAX_OBSERVED_MACS: usize = 4;

/// Datagrams parsed per observed sender before sampling stops.
///
/// Reading a MAC costs a parse, which an unmapped sender would otherwise
/// command without limit. Several samples are needed because a receiver
/// reports every transmitter it sensed, not only ours.
const MAC_SAMPLES: u32 = 32;

/// What an unmapped sender has been seen doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderObservation {
    /// Address the datagrams come from. Keyed by IP rather than by socket:
    /// a node that restarts draws a new ephemeral port and must not appear
    /// as a second candidate.
    pub source: IpAddr,
    /// Transmitter MACs seen in this sender's frames.
    pub tx_macs: Vec<MacAddr>,
    /// CSI datagrams counted from it.
    pub datagrams: u64,
    /// Edge time of the first datagram.
    pub first_seen_us: u64,
    /// Edge time of the most recent one.
    pub last_seen_us: u64,
    /// Datagrams parsed so far, bounding the sampling above.
    pub sampled: u32,
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
    observed: HashMap<IpAddr, SenderObservation>,
    read_timeout: Option<Duration>,
}

impl UdpSource {
    /// Binds the intake socket and registers the sender mapping.
    ///
    /// An empty mapping is allowed: an appliance still being installed has no
    /// nodes yet, and every datagram it hears is then a candidate to pair
    /// rather than something wasted.
    ///
    /// # Errors
    ///
    /// Socket binding failures.
    pub fn bind(
        addr: impl ToSocketAddrs,
        nodes: HashMap<SenderKey, String>,
        tx_mac: Option<MacAddr>,
    ) -> io::Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind(addr)?,
            nodes,
            tx_mac,
            stats: UdpStats::default(),
            last_seq: HashMap::new(),
            last_ts_us: 0,
            buf: vec![0u8; DATAGRAM_BUF],
            observed: HashMap::new(),
            read_timeout: None,
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

    /// Bounds how long a [`Self::next_frame`] call may take; `None` blocks
    /// until a frame can be yielded.
    ///
    /// The bound is on the call, not on the wait for a datagram. Datagrams
    /// that yield no frame — an unmapped sender, noise — are consumed in a
    /// loop, so a busy stream would otherwise keep a caller inside one call
    /// indefinitely and starve whatever periodic work it has.
    ///
    /// # Errors
    ///
    /// The underlying socket error.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.read_timeout = timeout;
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

    /// Senders that are streaming but are not in the mapping, oldest first.
    ///
    /// The order nodes were powered in, which is the order identifiers are
    /// then offered in.
    #[must_use]
    pub fn observations(&self) -> Vec<SenderObservation> {
        let mut observations: Vec<SenderObservation> = self.observed.values().cloned().collect();
        observations.sort_by_key(|observation| (observation.first_seen_us, observation.source));
        observations
    }

    /// Records a datagram from a sender that is not in the mapping.
    fn observe(&mut self, from: SocketAddr, len: usize, now: u64) {
        let source = from.ip();
        if !self.observed.contains_key(&source) && self.observed.len() >= MAX_OBSERVED_SENDERS {
            self.stats.observations_dropped += 1;
            return;
        }

        let observation = self
            .observed
            .entry(source)
            .or_insert_with(|| SenderObservation {
                source,
                tx_macs: Vec::new(),
                datagrams: 0,
                first_seen_us: now,
                last_seen_us: now,
                sampled: 0,
            });
        observation.datagrams += 1;
        observation.last_seen_us = now;

        if observation.sampled >= MAC_SAMPLES {
            return;
        }
        observation.sampled += 1;
        let text = String::from_utf8_lossy(&self.buf[..len]);
        if let Ok(raw) = parse_line(&text) {
            if !observation.tx_macs.contains(&raw.mac)
                && observation.tx_macs.len() < MAX_OBSERVED_MACS
            {
                observation.tx_macs.push(raw.mac);
            }
        }
    }

    /// Returns the next valid frame from a mapped sender.
    ///
    /// Robustness policy mirrors the line reader: unknown senders,
    /// non-frame datagrams and malformed lines are counted and skipped,
    /// never fatal. The returned frame carries an edge-assigned,
    /// monotonically non-decreasing timestamp.
    ///
    /// # Errors
    ///
    /// Socket-level I/O errors, and [`io::ErrorKind::WouldBlock`] once a
    /// configured read timeout has elapsed without a frame to yield.
    pub fn next_frame(&mut self) -> io::Result<CsiFrame> {
        let deadline = self.read_timeout.map(|timeout| {
            now_us().saturating_add(timeout.as_micros().try_into().unwrap_or(u64::MAX))
        });
        loop {
            if let Some(deadline) = deadline
                && now_us() >= deadline
            {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "no frame within the read timeout",
                ));
            }
            let (len, from) = self.socket.recv_from(&mut self.buf)?;
            self.stats.datagrams += 1;

            let node_id = match self.identify(from) {
                Some(node_id) => node_id,
                None => {
                    self.stats.unknown_sender += 1;
                    let now = now_us();
                    self.observe(from, len, now);
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
        let mut source = UdpSource::bind("127.0.0.1:0", nodes, None).unwrap();
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
    fn an_unmapped_sender_is_recorded_with_the_transmitter_it_reports() {
        let known = sender();
        let stranger = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let target = source.local_addr().unwrap();

        stranger.send_to(c6_line(1).as_bytes(), target).unwrap();
        known.send_to(c6_line(1).as_bytes(), target).unwrap();
        source.next_frame().unwrap();

        let observations = source.observations();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].source, stranger.local_addr().unwrap().ip());
        assert_eq!(observations[0].datagrams, 1);
        // The MAC in the line is the transmitter's: this is what lets the
        // appliance name a transmitter that never joins the network itself.
        assert_eq!(
            observations[0].tx_macs,
            vec!["1a:2b:3c:4d:5e:6f".parse::<MacAddr>().unwrap()]
        );
    }

    #[test]
    fn a_mapped_sender_is_never_offered_for_pairing() {
        let known = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let target = source.local_addr().unwrap();

        known.send_to(c6_line(1).as_bytes(), target).unwrap();
        source.next_frame().unwrap();

        assert!(source.observations().is_empty());
    }

    #[test]
    fn one_node_that_restarts_is_one_candidate_not_two() {
        // Two sockets on one address: the same node having drawn a new
        // ephemeral port. Keying observations by IP is what keeps it from
        // being offered twice.
        let known = sender();
        let before = sender();
        let after = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let target = source.local_addr().unwrap();
        assert_eq!(
            before.local_addr().unwrap().ip(),
            after.local_addr().unwrap().ip(),
            "the test needs both strangers on one address"
        );

        before.send_to(c6_line(1).as_bytes(), target).unwrap();
        after.send_to(c6_line(2).as_bytes(), target).unwrap();
        known.send_to(c6_line(1).as_bytes(), target).unwrap();
        source.next_frame().unwrap();

        let observations = source.observations();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].datagrams, 2);
    }

    #[test]
    fn counting_outlives_sampling() {
        let known = sender();
        let stranger = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let target = source.local_addr().unwrap();

        let bursts = MAC_SAMPLES + 5;
        for seq in 0..bursts {
            stranger.send_to(c6_line(seq).as_bytes(), target).unwrap();
        }
        known.send_to(c6_line(1).as_bytes(), target).unwrap();
        source.next_frame().unwrap();

        let observations = source.observations();
        // Counting outlives sampling: the rate is how a node shows it is
        // alive.
        assert_eq!(observations[0].datagrams, u64::from(bursts));
        assert_eq!(observations[0].sampled, MAC_SAMPLES);
        assert_eq!(observations[0].tx_macs.len(), 1);
    }

    #[test]
    fn the_observation_table_is_bounded_and_still_updates_what_it_holds() {
        // Driven directly: distinct source addresses are not portable on
        // loopback.
        let known = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let line = c6_line(1);
        let len = line.len();
        source.buf[..len].copy_from_slice(line.as_bytes());

        let addr_of = |n: u16| SocketAddr::from(([10, 0, 0, u8::try_from(n).unwrap()], 5000 + n));
        for n in 0..u16::try_from(MAX_OBSERVED_SENDERS).unwrap() + 4 {
            source.observe(addr_of(n), len, 1_000 + u64::from(n));
        }

        assert_eq!(source.observations().len(), MAX_OBSERVED_SENDERS);
        assert_eq!(source.stats().observations_dropped, 4);

        // A node already being observed must keep updating while the table
        // is full, or a flood would freeze the pairing screen.
        source.observe(addr_of(0), len, 9_000);
        let first = source
            .observations()
            .into_iter()
            .find(|observation| observation.source == addr_of(0).ip())
            .unwrap();
        assert_eq!(first.datagrams, 2);
        assert_eq!(first.last_seen_us, 9_000);
    }

    #[test]
    fn observations_are_offered_in_the_order_the_nodes_were_powered() {
        let known = sender();
        let mut source = source_for(&[(&known, "rx-1")]);
        let line = c6_line(1);
        let len = line.len();
        source.buf[..len].copy_from_slice(line.as_bytes());

        source.observe(SocketAddr::from(([10, 0, 0, 3], 5000)), len, 3_000);
        source.observe(SocketAddr::from(([10, 0, 0, 1], 5000)), len, 1_000);
        source.observe(SocketAddr::from(([10, 0, 0, 2], 5000)), len, 2_000);

        let sources: Vec<IpAddr> = source
            .observations()
            .into_iter()
            .map(|observation| observation.source)
            .collect();
        assert_eq!(
            sources,
            vec![
                IpAddr::from([10, 0, 0, 1]),
                IpAddr::from([10, 0, 0, 2]),
                IpAddr::from([10, 0, 0, 3]),
            ]
        );
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
    fn a_busy_stream_that_yields_nothing_still_returns_to_the_caller() {
        // The datagrams of an unmapped sender are consumed in an inner loop.
        // Without bounding the *call*, a node streaming continuously keeps the
        // caller inside one `next_frame` for good — and the periodic work that
        // publishes what has been heard never runs.
        let stranger = sender();
        let mut source = UdpSource::bind("127.0.0.1:0", HashMap::new(), None).unwrap();
        source
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let target = source.local_addr().unwrap();
        for seq in 0..400 {
            stranger.send_to(c6_line(seq).as_bytes(), target).unwrap();
        }

        let error = source.next_frame().unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(
            !source.observations().is_empty(),
            "datagrams were still read"
        );
    }

    #[test]
    fn an_appliance_with_no_nodes_yet_still_listens() {
        // An installation begins with nothing paired. Refusing to bind would
        // leave the socket closed exactly when every datagram is a candidate.
        let stranger = sender();
        let mut source = UdpSource::bind("127.0.0.1:0", HashMap::new(), None).unwrap();
        source
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let target = source.local_addr().unwrap();

        stranger.send_to(c6_line(1).as_bytes(), target).unwrap();
        assert!(source.next_frame().is_err());

        let observations = source.observations();
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].datagrams, 1);
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
