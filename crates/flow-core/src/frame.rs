use serde::{Deserialize, Serialize};

use crate::TimestampUs;
use crate::error::FrameError;

/// One CSI measurement received from a sensing node.
///
/// Field names and types match one line of `csi.ndjson` in the canonical
/// session format; they are frozen and must not change without a format
/// version bump.
///
/// # Invariants
///
/// A structurally valid frame satisfies:
///
/// - `amp.len() == phase.len()` — one amplitude and one phase per subcarrier;
/// - `len == amp.len()` — the declared subcarrier count matches the data;
/// - `len > 0` — a frame without subcarrier data is meaningless.
///
/// [`CsiFrame::new`] establishes these invariants for locally built frames.
/// Serde deserialization does **not** enforce them, so frames read from a
/// trust boundary (UDP socket, disk) must be checked with
/// [`CsiFrame::validate`] before use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CsiFrame {
    /// Reception timestamp assigned by the edge aggregator, in microseconds.
    pub ts_us: TimestampUs,
    /// Identifier of the receiving node (e.g. `"rx-1"`), as declared in the
    /// session metadata.
    pub node_id: String,
    /// Received signal strength in dBm, as reported by the node's radio.
    pub rssi: i8,
    /// Wi-Fi MCS index of the packet the CSI was extracted from.
    pub mcs: u8,
    /// Declared number of subcarriers (the length of `amp` and `phase`).
    pub len: usize,
    /// Per-subcarrier amplitude.
    pub amp: Vec<f32>,
    /// Per-subcarrier phase, in radians.
    pub phase: Vec<f32>,
}

impl CsiFrame {
    /// Builds a frame, deriving `len` from the data and checking the
    /// structural invariants.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::AmpPhaseMismatch`] if `amp` and `phase` differ
    /// in length, or [`FrameError::Empty`] if they carry no data.
    pub fn new(
        ts_us: TimestampUs,
        node_id: impl Into<String>,
        rssi: i8,
        mcs: u8,
        amp: Vec<f32>,
        phase: Vec<f32>,
    ) -> Result<Self, FrameError> {
        if amp.len() != phase.len() {
            return Err(FrameError::AmpPhaseMismatch {
                amp_len: amp.len(),
                phase_len: phase.len(),
            });
        }
        if amp.is_empty() {
            return Err(FrameError::Empty);
        }
        Ok(Self {
            ts_us,
            node_id: node_id.into(),
            rssi,
            mcs,
            len: amp.len(),
            amp,
            phase,
        })
    }

    /// Checks the structural invariants documented on [`CsiFrame`].
    ///
    /// Call this on every frame that crosses a trust boundary (deserialized
    /// from the network or from disk).
    ///
    /// # Errors
    ///
    /// Returns the first violated invariant as a [`FrameError`].
    pub fn validate(&self) -> Result<(), FrameError> {
        if self.amp.len() != self.phase.len() {
            return Err(FrameError::AmpPhaseMismatch {
                amp_len: self.amp.len(),
                phase_len: self.phase.len(),
            });
        }
        if self.len != self.amp.len() {
            return Err(FrameError::LenMismatch {
                declared: self.len,
                actual: self.amp.len(),
            });
        }
        if self.amp.is_empty() {
            return Err(FrameError::Empty);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_frame() -> CsiFrame {
        CsiFrame::new(
            1_720_000_000_000_000,
            "rx-1",
            -52,
            7,
            vec![1.0, 2.5, 0.25],
            vec![0.0, -1.5, 3.1],
        )
        .unwrap()
    }

    #[test]
    fn new_derives_len_from_data() {
        let frame = valid_frame();
        assert_eq!(frame.len, 3);
        assert_eq!(frame.validate(), Ok(()));
    }

    #[test]
    fn new_rejects_amp_phase_mismatch() {
        let err = CsiFrame::new(0, "rx-1", -50, 7, vec![1.0, 2.0], vec![0.0]).unwrap_err();
        assert_eq!(
            err,
            FrameError::AmpPhaseMismatch {
                amp_len: 2,
                phase_len: 1
            }
        );
    }

    #[test]
    fn new_rejects_empty_frame() {
        let err = CsiFrame::new(0, "rx-1", -50, 7, vec![], vec![]).unwrap_err();
        assert_eq!(err, FrameError::Empty);
    }

    #[test]
    fn json_round_trip_preserves_frame() {
        let frame = valid_frame();
        let json = serde_json::to_string(&frame).unwrap();
        let back: CsiFrame = serde_json::from_str(&json).unwrap();
        assert_eq!(back, frame);
    }

    #[test]
    fn canonical_ndjson_line_parses_and_validates() {
        let line = concat!(
            r#"{"ts_us":1720000000000000,"node_id":"rx-1","rssi":-52,"mcs":7,"#,
            r#""len":3,"amp":[1.0,2.5,0.25],"phase":[0.0,-1.5,3.1]}"#
        );
        let frame: CsiFrame = serde_json::from_str(line).unwrap();
        assert_eq!(frame.validate(), Ok(()));
        assert_eq!(frame, valid_frame());
    }

    #[test]
    fn serialized_field_names_are_frozen() {
        let json = serde_json::to_string(&valid_frame()).unwrap();
        for field in ["ts_us", "node_id", "rssi", "mcs", "len", "amp", "phase"] {
            assert!(
                json.contains(&format!("\"{field}\"")),
                "missing field {field} in {json}"
            );
        }
    }

    #[test]
    fn validate_catches_len_mismatch_after_deserialization() {
        let line = concat!(
            r#"{"ts_us":0,"node_id":"rx-1","rssi":-52,"mcs":7,"#,
            r#""len":5,"amp":[1.0],"phase":[0.0]}"#
        );
        let frame: CsiFrame = serde_json::from_str(line).unwrap();
        assert_eq!(
            frame.validate(),
            Err(FrameError::LenMismatch {
                declared: 5,
                actual: 1
            })
        );
    }
}
