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
    WaitTuning(String),
    /// The service schedule is unusable.
    #[error("service schedule: {0}")]
    Service(#[from] ScheduleError),
    /// A window or hop duration is zero.
    #[error("{field} must be greater than zero")]
    NotPositive {
        /// Name of the offending field.
        field: &'static str,
    },
}

/// A string that is not a well-formed device secret.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SecretError {
    /// The secret does not have the expected number of characters.
    #[error("a device secret has {expected} characters, got {got}")]
    InvalidLength {
        /// Characters a device secret must carry.
        expected: usize,
        /// Characters actually found, separators excluded.
        got: usize,
    },
    /// A character is neither in the alphabet nor foldable onto it.
    #[error("{0:?} is not a device-secret character")]
    InvalidCharacter(char),
}

/// Failure while establishing or checking the administrator credential.
#[derive(Debug, Error)]
pub enum CredentialError {
    /// The supplied secret is not well formed.
    #[error("malformed device secret")]
    Secret(#[from] SecretError),
    /// The credential file could not be read or written.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The password hasher rejected the operation.
    ///
    /// Carries the hasher's own message: its error type is not
    /// comparable, and the detail only ever reaches an operator's console.
    #[error("password hashing failed: {0}")]
    Hashing(String),
    /// The appliance has no administrator credential yet.
    #[error("no administrator credential at {path}; run `flow-edge provision` first")]
    Missing {
        /// Where the credential was expected.
        path: PathBuf,
    },
}

/// Failure while reading or writing the appliance journal.
#[derive(Debug, Error)]
#[error("appliance journal")]
pub struct JournalError(#[from] rusqlite::Error);

/// Why the live pipeline could not start.
///
/// Most variants describe an installation that has not reached calibration
/// yet, which is a normal stage rather than a fault: the daemon reports the
/// reason and keeps serving.
#[derive(Debug, Error)]
pub enum PipelineError {
    /// The frame source could not be opened.
    ///
    /// The cause is carried as text rather than as a nested error: it comes
    /// from another crate's error type, and only ever reaches an operator's
    /// console.
    #[error("opening {input}: {detail}")]
    Source {
        /// The input specification that failed.
        input: String,
        /// The underlying message.
        detail: String,
    },
    /// The model could not be loaded.
    #[error("loading {path}: {detail}")]
    Model {
        /// The model that failed to load.
        path: PathBuf,
        /// The underlying message.
        detail: String,
    },
    /// The model and the configuration disagree.
    #[error("pipeline: {0}")]
    Pipeline(String),
}

/// A service schedule the appliance refuses to run with.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    /// The time zone is not one the system knows.
    #[error("unknown time zone {0:?}")]
    TimeZone(String),
    /// A time of day is not `HH:MM` within a real day.
    #[error("{0:?} is not a time of day")]
    Time(String),
    /// A closure date is not `YYYY-MM-DD`.
    #[error("{0:?} is not a date")]
    Date(String),
    /// An interval ends before, or when, it starts.
    ///
    /// The day is named because a week holds seven of them, and the person
    /// reading this is looking at a form with seven rows.
    #[error("{day}: service from {from} to {to} ends before it starts")]
    IntervalOrder {
        /// Day of the week the offending interval belongs to.
        day: &'static str,
        /// Start of the offending interval.
        from: String,
        /// End of the offending interval.
        to: String,
    },
    /// Two intervals of the same day overlap.
    ///
    /// Two overlapping services are a mistake rather than a schedule: the
    /// operator meant one longer interval.
    #[error("{day}: service {first} overlaps {second}")]
    Overlap {
        /// Day of the week the offending intervals belong to.
        day: &'static str,
        /// The earlier interval.
        first: String,
        /// The one that starts before it ends.
        second: String,
    },
    /// A closure ends before it starts.
    #[error("closure from {from} to {to} ends before it starts")]
    ClosureOrder {
        /// First day of the offending closure.
        from: String,
        /// Last day of the offending closure.
        to: String,
    },
    /// A timestamp outside the range the calendar can express.
    #[error("{0} is not a representable instant")]
    Instant(u64),
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

/// A density model bundle that cannot be taken into service.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    /// The archive could not be read as a gzipped tar.
    #[error("the file is not a model bundle: {0}")]
    Unreadable(String),
    /// It carries something a bundle does not hold.
    #[error("unexpected file in the bundle: {0}")]
    UnexpectedMember(String),
    /// A member is larger than a bundle should ever be.
    #[error("{member} is too large for a model bundle")]
    TooLarge {
        /// Name of the offending member.
        member: String,
    },
    /// A member the bundle must hold is absent.
    #[error("the bundle has no {0}")]
    MissingMember(&'static str),
    /// The tuning could not be read.
    #[error("the bundle site settings are unusable: {0}")]
    Tuning(String),
    /// The model does not fit this appliance.
    #[error("{0}")]
    Unusable(String),
    /// The files could not be put in place.
    #[error("{0}")]
    Staging(String),
    /// No model is held under that handle.
    #[error("no model called {0}")]
    Unknown(String),
}
