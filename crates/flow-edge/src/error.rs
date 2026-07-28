use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Failure while reading or writing appliance state on disk.
///
/// Every variant carries the offending path: on an appliance nobody logs
/// into daily, "which file" is the first thing an operator needs.
#[derive(Debug, Error)]
pub enum StoreError {
    /// The file could not be read.
    #[error("reading {path}")]
    Read {
        /// File the appliance tried to read.
        path: PathBuf,
        /// Underlying I/O failure.
        source: io::Error,
    },
    /// The file could not be written.
    #[error("writing {path}")]
    Write {
        /// File the appliance tried to write.
        path: PathBuf,
        /// Underlying I/O failure.
        source: io::Error,
    },
    /// The file exists but does not hold valid JSON for its type.
    #[error("parsing {path}")]
    Parse {
        /// File whose contents could not be understood.
        path: PathBuf,
        /// Underlying deserialization failure.
        source: serde_json::Error,
    },
    /// The file was read and parsed, but its contents are not a usable
    /// configuration.
    #[error("invalid configuration in {path}")]
    Invalid {
        /// File holding the rejected configuration.
        path: PathBuf,
        /// The validation failure.
        source: ConfigError,
    },
}

/// A configuration value the appliance refuses to run with.
///
/// Validation happens once, when the configuration is loaded or saved, so
/// that the rest of the daemon can treat an [`ApplianceConfig`] as known
/// good. Wi-Fi limits (SSID and passphrase lengths, channel range) come
/// from IEEE 802.11, not from arbitrary choices.
///
/// [`ApplianceConfig`]: crate::ApplianceConfig
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    /// A required text field is empty.
    #[error("{field} must not be empty")]
    Empty {
        /// Name of the offending field.
        field: &'static str,
    },
    /// An SSID exceeds the 32-byte limit of IEEE 802.11.
    #[error("SSID must be 1..=32 bytes, got {0}")]
    SsidLength(usize),
    /// A WPA passphrase is outside the 8..=63 character range.
    #[error("Wi-Fi passphrase must be 8..=63 characters, got {0}")]
    PassphraseLength(usize),
    /// The sensor access point channel is outside the 2.4 GHz band.
    #[error("sensor AP channel must be in 1..=13, got {0}")]
    Channel(u8),
    /// A node MAC address could not be parsed.
    #[error("node {node_id} has an invalid MAC address {mac:?}")]
    NodeMac {
        /// Logical id of the offending node.
        node_id: String,
        /// The unparsable value.
        mac: String,
    },
    /// Two nodes share a logical id.
    #[error("duplicate node id {0}")]
    DuplicateNodeId(String),
    /// Two nodes share a MAC address.
    #[error("duplicate node MAC address {0}")]
    DuplicateNodeMac(String),
    /// Two nodes claim the same reserved address.
    #[error("duplicate node address {0}")]
    DuplicateNodeAddress(String),
    /// More than one transmitter is declared.
    ///
    /// The geometry is one transmitter framed by receivers; a second
    /// transmitter on the same channel would corrupt every measurement.
    #[error("exactly one transmitter is allowed, got {0}")]
    MultipleTransmitters(usize),
    /// A site-tuning value is unusable (delegated to the wait estimator,
    /// the single source of truth for what a valid tuning is).
    #[error("site tuning: {0}")]
    SiteTuning(String),
    /// A window or hop duration is zero.
    #[error("{field} must be greater than zero")]
    NotPositive {
        /// Name of the offending field.
        field: &'static str,
    },
}

/// A refused appliance lifecycle or runtime transition.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TransitionError {
    /// The CSI stream already has an exclusive consumer.
    ///
    /// Calibration and live inference both consume the single UDP stream;
    /// only one may run at a time. Callers that want to switch stop the
    /// current activity first.
    #[error("the CSI stream is already in use by {current}")]
    StreamBusy {
        /// What currently holds the stream (`calibration` or `live`).
        current: &'static str,
    },
    /// Live inference was requested without an active density model.
    #[error("no active model: calibrate the site or import a model first")]
    NoModel,
    /// Onboarding cannot be closed while a step is still outstanding.
    #[error("installation is not complete: still waiting on {stage}")]
    Incomplete {
        /// The first step that is not satisfied yet.
        stage: crate::Stage,
    },
}
