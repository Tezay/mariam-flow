//! The Mariam Flow edge appliance daemon.
//!
//! Where the other crates are libraries and lab tools, this one is the
//! product: the single long-running process an installed appliance boots
//! into. It owns the appliance's configuration, its installation
//! lifecycle, and — as they land — the dashboard it serves, the sensing
//! nodes it pairs, the calibration it drives and the network it
//! configures.
//!
//! One process, not several, because the deployment target is a
//! 512 MB single-board computer and because the parts genuinely share
//! state: the guided installation, calibration and live inference all
//! contend for the same CSI stream, and arbitrating that across process
//! boundaries would buy nothing.
//!
//! Implemented so far:
//!
//! - [`ApplianceConfig`] — the validated, atomically persisted
//!   configuration: identity, sensor access point, uplink, paired nodes,
//!   site tuning.
//! - [`Readiness`] / [`Phase`] / [`Stage`] — installation progress,
//!   derived from facts rather than stored as a cursor, so it survives a
//!   reboot mid-installation.
//! - [`Runtime`] — exclusive access to the CSI stream, so calibration and
//!   live inference can never both claim it.
//! - [`DeviceSecret`] / [`AdminCredential`] — the credential printed on an
//!   appliance's label, stored only as an Argon2id hash, with a recovery
//!   path that requires physical possession of the card.
//! - [`router`] — the read-only status surface the dashboard builds on.
//!
//! - [`SessionStore`] / [`Throttle`] — sessions opened by a successful
//!   login, and the doubling delay that makes guessing the secret
//!   uneconomic without ever locking the installer out.
//!
//! Planned: the embedded dashboard, node pairing, network configuration through NetworkManager,
//! calibration control, model import and the outbound push of aggregated
//! estimates. See `docs/architecture.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod assets;
mod calibration;
mod config;
mod credential;
mod discovery;
mod edge_state;
mod error;
mod history;
mod http;
mod journal;
mod lifecycle;
mod model;
mod pipeline;
mod schedule;
mod secret;
mod session;
mod store;
mod system;
mod throttle;
mod views;

pub use config::{
    Addressing, ApplianceConfig, DEFAULT_HOP_US, DEFAULT_SENSOR_CHANNEL, DEFAULT_WINDOW_US,
    Identity, NetworkConfig, PairedNode, SensorAp, Uplink, WaitTuning, WifiSecurity,
};
pub use credential::{AdminCredential, CREDENTIAL_FILE, ResetOutcome, apply_pending_reset};
pub use edge_state::EdgeState;
pub use error::{
    ConfigError, CredentialError, JournalError, PipelineError, ScheduleError, SecretError,
    StoreError, TransitionError,
};
pub use history::{MINUTE_US, MinuteAggregator, MinuteSummary};
pub use http::router;
pub use journal::{
    Event, EventCategory, EventKind, JOURNAL_FILE, Journal, MAX_EVENTS, RETENTION_US, RecordedEvent,
};
pub use lifecycle::{Phase, Readiness, Runtime, RuntimeMode, Stage};
pub use pipeline::{LiveOptions, NodeHealth, StreamHealth, spawn_pipeline};
pub use schedule::{Closure, Interval, LocalTime, ServiceState, ServiceWindow, WeeklyHours};
pub use secret::{DeviceSecret, SECRET_ENTROPY_BITS};
pub use session::{ABSOLUTE_LIFETIME_US, IDLE_TIMEOUT_US, SessionStore};
pub use throttle::Throttle;

/// File name of the active density model inside the data directory.
pub const ACTIVE_MODEL: &str = "model.onnx";

/// Analysis geometry of the model in service, beside it.
pub const ACTIVE_ANALYSIS: &str = "analysis.json";

/// Current Unix time in microseconds — the appliance clock.
///
/// Every timestamp the daemon assigns comes from here, exactly as frame and
/// label timestamps do elsewhere in the system: the edge clock is the only
/// one trusted.
#[must_use]
pub fn now_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
        })
}
