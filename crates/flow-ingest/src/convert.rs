//! Conversion of raw parsed frames into canonical CSI frames.

use flow_core::{CsiFrame, FrameError, TimestampUs};

use crate::esp_csi::RawCsiFrame;

impl RawCsiFrame {
    /// Converts this raw frame into a canonical [`CsiFrame`].
    ///
    /// This is where the edge-side rules apply:
    ///
    /// - `ts_us` is the edge-assigned reception timestamp; the node's
    ///   `local_timestamp` is never used;
    /// - `node_id` comes from the edge configuration (MAC-to-node mapping);
    /// - interleaved I/Q values (imaginary part first) become
    ///   per-sub-carrier amplitude and phase: `amp = √(re² + im²)`,
    ///   `phase = atan2(im, re)` in radians. The mapping is bijective, so
    ///   no information is lost relative to the raw values.
    ///
    /// The resulting frame's `mcs` field carries the classic layout's MCS
    /// index when present, and the raw `rate` field otherwise (C6 family
    /// lines do not report a separate MCS) — the per-frame PHY indicator
    /// available on each chip family.
    ///
    /// `data` is expected to hold complete I/Q pairs, as guaranteed by
    /// [`parse_line`](crate::parse_line); a trailing unpaired value on a
    /// hand-built frame is ignored.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::Empty`] for a frame without sub-carrier data.
    pub fn to_frame(
        &self,
        node_id: impl Into<String>,
        ts_us: TimestampUs,
    ) -> Result<CsiFrame, FrameError> {
        let pairs = self.data.len() / 2;
        let mut amp = Vec::with_capacity(pairs);
        let mut phase = Vec::with_capacity(pairs);
        for pair in self.data.chunks_exact(2) {
            let im = f32::from(pair[0]);
            let re = f32::from(pair[1]);
            amp.push(re.hypot(im));
            phase.push(im.atan2(re));
        }
        CsiFrame::new(
            ts_us,
            node_id,
            self.rssi,
            self.mcs.unwrap_or(self.rate),
            amp,
            phase,
        )
    }
}

#[cfg(test)]
mod tests {
    use core::f32::consts::{FRAC_PI_2, PI};

    use flow_core::FrameError;

    use crate::esp_csi::{LineFormat, MacAddr, RawCsiFrame};

    const EPS: f32 = 1e-6;

    fn raw(data: Vec<i16>) -> RawCsiFrame {
        RawCsiFrame {
            format: LineFormat::Esp32C6Family,
            seq: 1,
            mac: MacAddr([0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f]),
            rssi: -52,
            rate: 11,
            mcs: None,
            noise_floor: -92,
            channel: 6,
            local_timestamp: 1_000,
            sig_len: 128,
            first_word_invalid: false,
            data,
        }
    }

    fn assert_close(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < EPS,
            "{what}: expected {expected}, got {actual}"
        );
    }

    #[test]
    fn computes_amplitude_and_phase_from_iq_pairs() {
        // Pairs are (imaginary, real): (4,3) → amp 5, phase atan2(4,3);
        // (0,1) → amp 1, phase 0; (1,0) → amp 1, phase π/2;
        // (0,-1) → amp 1, phase π.
        let frame = raw(vec![4, 3, 0, 1, 1, 0, 0, -1])
            .to_frame("rx-1", 1_720_000_000_000_000)
            .unwrap();
        assert_eq!(frame.len, 4);
        assert_close(frame.amp[0], 5.0, "amp[0]");
        assert_close(frame.phase[0], 4.0_f32.atan2(3.0), "phase[0]");
        assert_close(frame.amp[1], 1.0, "amp[1]");
        assert_close(frame.phase[1], 0.0, "phase[1]");
        assert_close(frame.phase[2], FRAC_PI_2, "phase[2]");
        assert_close(frame.phase[3], PI, "phase[3]");
    }

    #[test]
    fn assigns_edge_timestamp_and_node_id() {
        let frame = raw(vec![0, 1]).to_frame("rx-2", 42).unwrap();
        assert_eq!(frame.ts_us, 42);
        assert_eq!(frame.node_id, "rx-2");
        assert_eq!(frame.rssi, -52);
        assert_eq!(frame.validate(), Ok(()));
    }

    #[test]
    fn mcs_falls_back_to_rate_on_c6() {
        let frame = raw(vec![0, 1]).to_frame("rx-1", 0).unwrap();
        assert_eq!(frame.mcs, 11);
    }

    #[test]
    fn mcs_uses_classic_mcs_when_present() {
        let mut classic = raw(vec![0, 1]);
        classic.format = LineFormat::Esp32Classic;
        classic.mcs = Some(7);
        let frame = classic.to_frame("rx-1", 0).unwrap();
        assert_eq!(frame.mcs, 7);
    }

    #[test]
    fn empty_data_is_rejected() {
        assert_eq!(
            raw(Vec::new()).to_frame("rx-1", 0).unwrap_err(),
            FrameError::Empty
        );
    }

    #[test]
    fn parsed_line_converts_and_validates_end_to_end() {
        let line = concat!(
            "CSI_DATA,312,1a:2b:3c:4d:5e:6f,-52,11,-92,4,12,6,183920121,128,1,",
            "6,0,\"[4,3,0,1,1,0]\""
        );
        let frame = crate::parse_line(line)
            .unwrap()
            .to_frame("rx-1", 1_720_000_000_000_000)
            .unwrap();
        assert_eq!(frame.validate(), Ok(()));
        assert_eq!(frame.len, 3);
        assert_eq!(frame.mcs, 11);
    }
}
