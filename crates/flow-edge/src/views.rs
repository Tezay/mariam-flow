//! The shapes the appliance serialises, and nothing else.
//!
//! Separate from the state they read, so that adding a field to the wire
//! format cannot reach into the state's internals.

use serde::Serialize;

use crate::config::{NetworkSurvey, Uplink};
use crate::edge_state::JournalFailure;
use crate::lifecycle::{Phase, Readiness, RuntimeMode};
use crate::pipeline::StreamHealth;
use crate::schedule::ServiceState;

#[derive(Serialize)]
pub(crate) struct StatusResponse {
    pub(crate) kit_id: String,
    pub(crate) site_name: Option<String>,
    pub(crate) phase: Phase,
    pub(crate) readiness: Readiness,
    pub(crate) runtime: RuntimeMode,
    pub(crate) model_installed: bool,
    pub(crate) stream: StreamHealth,
    pub(crate) service: ServiceState,
    pub(crate) sensor_ap: SensorApView,
    pub(crate) uplink: UplinkView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) survey: Option<NetworkSurvey>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) classes: Option<flow_core::ClassMapping>,
    /// Which stored model is estimating, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) active_model: Option<String>,
    /// Since when the journal has been unable to write, if it cannot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) journal_failure: Option<JournalFailure>,
    pub(crate) nodes: Vec<NodeView>,
}

#[derive(Serialize)]
pub(crate) struct SensorApView {
    pub(crate) ssid: String,
    pub(crate) channel: u8,
}

/// The uplink as the dashboard sees it: its shape, never its secrets.
#[derive(Serialize)]
pub(crate) struct UplinkView {
    pub(crate) mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ssid: Option<String>,
}

impl UplinkView {
    pub(crate) fn of(uplink: Option<&Uplink>) -> Self {
        match uplink {
            None => Self {
                mode: "undecided",
                ssid: None,
            },
            Some(Uplink::Offline) => Self {
                mode: "offline",
                ssid: None,
            },
            Some(Uplink::Wifi { ssid, .. }) => Self {
                mode: "wifi",
                ssid: Some(ssid.clone()),
            },
            Some(Uplink::Ethernet { .. }) => Self {
                mode: "ethernet",
                ssid: None,
            },
        }
    }
}

#[derive(Serialize)]
pub(crate) struct NodeView {
    pub(crate) node_id: String,
    pub(crate) role: flow_core::NodeRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mac: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) position: Option<String>,
}

/// What the live stream sends on every tick.
///
/// Estimate and stream health travel together because the screen needs
/// both to say anything useful: a missing estimate means one thing when the
/// nodes are streaming and quite another when they have gone silent.
#[derive(Serialize)]
pub(crate) struct LiveSnapshot {
    /// The current estimate, absent until the first window fills.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) estimate: Option<EstimateView>,
    /// How the frames are arriving.
    pub(crate) stream: StreamHealth,
    /// Whether the site is serving, and when that next changes.
    pub(crate) service: ServiceState,
    /// Appliance clock, so a browser can judge staleness without trusting
    /// its own — the same reasoning as the labeling page.
    pub(crate) now_us: u64,
    /// Newest row of the journal, or nothing if it is empty.
    ///
    /// Carried here rather than polled for: this stream already ticks every
    /// second for the live view, so a reader of the journal learns that
    /// something happened within a second and at the cost of one integer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) journal_id: Option<i64>,
}

/// An estimate as the dashboard sees it.
///
/// `reliable` is carried rather than used to hide the value: an operator
/// looking at an administration screen needs to see what the model produced
/// *and* that it is not trustworthy. The public estimate surface is where
/// masking belongs, and it already does it.
#[derive(Serialize)]
pub(crate) struct EstimateView {
    pub(crate) ts_us: u64,
    pub(crate) wait_minutes: f32,
    pub(crate) people: f32,
    pub(crate) level: f32,
    pub(crate) class: String,
    pub(crate) confidence: f32,
    pub(crate) reliable: bool,
}

impl From<flow_infer::WaitEstimate> for EstimateView {
    fn from(estimate: flow_infer::WaitEstimate) -> Self {
        Self {
            ts_us: estimate.ts_us,
            wait_minutes: estimate.wait_minutes,
            people: estimate.people,
            level: estimate.level,
            class: estimate.display_class.to_string(),
            confidence: estimate.confidence,
            reliable: estimate.reliable,
        }
    }
}
