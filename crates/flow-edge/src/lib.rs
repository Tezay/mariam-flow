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
//! Planned: authenticated sessions over that surface, the embedded
//! dashboard, node pairing, network configuration through NetworkManager,
//! calibration control, model import and the outbound push of aggregated
//! estimates. See `docs/architecture.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod api;
mod config;
mod credential;
mod error;
mod secret;
mod state;
mod store;

pub use api::{EdgeState, router};
pub use config::{
    Addressing, ApplianceConfig, DEFAULT_HOP_US, DEFAULT_SENSOR_CHANNEL, DEFAULT_WINDOW_US,
    Identity, NetworkConfig, PairedNode, SensorAp, SiteTuning, Uplink, WifiSecurity,
};
pub use credential::{AdminCredential, CREDENTIAL_FILE, ResetOutcome, apply_pending_reset};
pub use error::{ConfigError, CredentialError, SecretError, StoreError, TransitionError};
pub use secret::{DeviceSecret, SECRET_ENTROPY_BITS};
pub use state::{Phase, Readiness, Runtime, RuntimeMode, Stage};
