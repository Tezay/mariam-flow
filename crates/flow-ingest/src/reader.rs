//! Streaming reader turning any line-based byte source into CSI frames.
//!
//! [`CsiReader`] wraps a [`BufRead`] source — a recorded capture file, a
//! serial port, standard input — and yields parsed [`RawCsiFrame`]s while
//! accumulating [`StreamStats`].
//!
//! Robustness policy: a capture must survive dirty input. Non-frame lines
//! (boot logs, prompts) and malformed `CSI_DATA` lines are counted and
//! skipped, never fatal; invalid UTF-8 (serial noise) is replaced lossily
//! before parsing. Only I/O errors from the underlying source are yielded
//! to the caller.

use std::collections::HashMap;
use std::io::BufRead;

use crate::esp_csi::{MacAddr, ParseError, RawCsiFrame, parse_line};

/// Statistics accumulated while reading a CSI line stream.
///
/// `lost_frames` is inferred from gaps in the per-transmitter sequence
/// numbers and is the basis of the session-quality metric (frame-loss
/// rate). A sequence number lower than or equal to its predecessor is
/// counted as a reset (node reboot or reordering), not as a loss.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Frames parsed successfully.
    pub frames: u64,
    /// Lines without the `CSI_DATA` marker (logs, blank lines, noise).
    pub skipped_lines: u64,
    /// `CSI_DATA` lines that failed to parse.
    pub parse_errors: u64,
    /// Frames inferred as lost from per-transmitter sequence gaps.
    pub lost_frames: u64,
    /// Sequence resets observed (node reboot or reordering).
    pub seq_resets: u64,
}

/// Iterator over the CSI frames of a line-based byte source.
///
/// Yields `Ok(RawCsiFrame)` for every parsed frame and `Err` only for I/O
/// errors on the underlying reader; see the module documentation for the
/// robustness policy. Accumulated statistics are available at any point
/// through [`CsiReader::stats`].
#[derive(Debug)]
pub struct CsiReader<R> {
    inner: R,
    buf: Vec<u8>,
    stats: StreamStats,
    last_seq: HashMap<MacAddr, u32>,
}

impl<R: BufRead> CsiReader<R> {
    /// Wraps a line-based byte source.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            stats: StreamStats::default(),
            last_seq: HashMap::new(),
        }
    }

    /// Statistics accumulated so far.
    #[must_use]
    pub fn stats(&self) -> StreamStats {
        self.stats
    }

    fn note_seq(&mut self, mac: MacAddr, seq: u32) {
        if let Some(prev) = self.last_seq.insert(mac, seq) {
            if seq > prev {
                self.stats.lost_frames += u64::from(seq - prev - 1);
            } else {
                self.stats.seq_resets += 1;
            }
        }
    }
}

impl<R: BufRead> Iterator for CsiReader<R> {
    type Item = std::io::Result<RawCsiFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.buf.clear();
            match self.inner.read_until(b'\n', &mut self.buf) {
                Ok(0) => return None,
                Err(err) => return Some(Err(err)),
                Ok(_) => {}
            }
            // Serial noise may not be valid UTF-8; replace rather than fail.
            let line = String::from_utf8_lossy(&self.buf);
            match parse_line(&line) {
                Ok(raw) => {
                    self.stats.frames += 1;
                    self.note_seq(raw.mac, raw.seq);
                    return Some(Ok(raw));
                }
                Err(ParseError::NotCsiData) => self.stats.skipped_lines += 1,
                Err(_) => self.stats.parse_errors += 1,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn c6_line(seq: u32, mac: &str) -> String {
        format!("CSI_DATA,{seq},{mac},-52,11,-92,4,12,6,183920121,128,1,4,0,\"[4,3,0,1]\"")
    }

    const MAC_A: &str = "1a:2b:3c:4d:5e:6f";
    const MAC_B: &str = "aa:bb:cc:dd:ee:ff";

    fn read_all(input: impl AsRef<[u8]>) -> (Vec<RawCsiFrame>, StreamStats) {
        let mut reader = CsiReader::new(Cursor::new(input.as_ref().to_vec()));
        let frames: Vec<RawCsiFrame> = reader.by_ref().map(|r| r.unwrap()).collect();
        (frames, reader.stats())
    }

    #[test]
    fn yields_frames_and_skips_log_lines() {
        let input = format!(
            "I (1234) wifi: connected\n{}\nESP-ROM:esp32c6-20220919\n{}\n",
            c6_line(1, MAC_A),
            c6_line(2, MAC_A),
        );
        let (frames, stats) = read_all(input);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].seq, 1);
        assert_eq!(frames[1].seq, 2);
        assert_eq!(stats.frames, 2);
        assert_eq!(stats.skipped_lines, 2);
        assert_eq!(stats.parse_errors, 0);
        assert_eq!(stats.lost_frames, 0);
    }

    #[test]
    fn counts_malformed_csi_lines_without_dying() {
        let input = format!(
            "{}\nCSI_DATA,oops,truncated\n{}\n",
            c6_line(1, MAC_A),
            c6_line(2, MAC_A),
        );
        let (frames, stats) = read_all(input);
        assert_eq!(frames.len(), 2);
        assert_eq!(stats.parse_errors, 1);
    }

    #[test]
    fn detects_sequence_gaps_as_lost_frames() {
        let input = format!("{}\n{}\n", c6_line(1, MAC_A), c6_line(4, MAC_A));
        let (frames, stats) = read_all(input);
        assert_eq!(frames.len(), 2);
        assert_eq!(stats.lost_frames, 2);
        assert_eq!(stats.seq_resets, 0);
    }

    #[test]
    fn tracks_sequences_per_transmitter() {
        let input = format!(
            "{}\n{}\n{}\n{}\n",
            c6_line(1, MAC_A),
            c6_line(10, MAC_B),
            c6_line(2, MAC_A),
            c6_line(11, MAC_B),
        );
        let (_, stats) = read_all(input);
        assert_eq!(stats.lost_frames, 0);
        assert_eq!(stats.seq_resets, 0);
    }

    #[test]
    fn counts_sequence_reset_not_loss() {
        let input = format!("{}\n{}\n", c6_line(10, MAC_A), c6_line(3, MAC_A));
        let (_, stats) = read_all(input);
        assert_eq!(stats.lost_frames, 0);
        assert_eq!(stats.seq_resets, 1);
    }

    #[test]
    fn survives_invalid_utf8_noise() {
        let mut input: Vec<u8> = Vec::new();
        input.extend_from_slice(&[0xff, 0xfe, 0x80, b'\n']);
        input.extend_from_slice(c6_line(1, MAC_A).as_bytes());
        input.push(b'\n');
        let (frames, stats) = read_all(input);
        assert_eq!(frames.len(), 1);
        assert_eq!(stats.skipped_lines, 1);
    }

    #[test]
    fn empty_input_yields_nothing() {
        let (frames, stats) = read_all(b"");
        assert!(frames.is_empty());
        assert_eq!(stats, StreamStats::default());
    }

    #[test]
    fn last_line_without_newline_is_read() {
        let (frames, _) = read_all(c6_line(1, MAC_A));
        assert_eq!(frames.len(), 1);
    }
}
