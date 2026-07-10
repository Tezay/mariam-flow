use thiserror::Error;

/// A density class value outside the valid `0..=3` range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("invalid density class {0}, expected a value in 0..=3")]
pub struct InvalidDensityClass(pub u8);

/// A structural inconsistency in a [`CsiFrame`](crate::CsiFrame).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FrameError {
    /// The amplitude and phase vectors have different lengths.
    #[error("amplitude/phase length mismatch: {amp_len} amplitudes vs {phase_len} phases")]
    AmpPhaseMismatch {
        /// Number of amplitude values in the frame.
        amp_len: usize,
        /// Number of phase values in the frame.
        phase_len: usize,
    },
    /// The declared subcarrier count does not match the actual data length.
    #[error("declared subcarrier count {declared} does not match actual {actual}")]
    LenMismatch {
        /// Value of the frame's `len` field.
        declared: usize,
        /// Actual number of subcarrier values carried.
        actual: usize,
    },
    /// The frame carries no subcarrier data at all.
    #[error("frame carries no subcarrier data")]
    Empty,
}
