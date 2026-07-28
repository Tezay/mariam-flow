//! The appliance status surface.
//!
//! This is the read-only foundation the dashboard is built on: one
//! endpoint that answers "what is this appliance, where is its
//! installation up to, and what is it doing right now". The wizard, the
//! node pairing and the calibration controls all hang off this same shared
//! state as they land.
//!
//! Two rules govern what may appear here:
//!
//! - **No credentials, ever.** The status surface reports network *shape*
//!   (which SSID, which mode) and never a passphrase, even though the
//!   daemon holds them. A test pins this.
//! - **No raw CSI.** The privacy invariant of the whole system: raw
//!   measurements never leave the site, and they never leave this process
//!   either.

use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::config::{ApplianceConfig, Uplink};
use crate::credential::AdminCredential;
use crate::state::{Phase, Readiness, Runtime, RuntimeMode};

struct Inner {
    config: ApplianceConfig,
    credential: AdminCredential,
    model_installed: bool,
    runtime: Runtime,
}

/// State shared by every handler.
#[derive(Clone)]
pub struct EdgeState {
    inner: Arc<Mutex<Inner>>,
}

impl EdgeState {
    /// Wraps a loaded configuration and the appliance credential.
    ///
    /// `model_installed` reports whether an active density model artifact
    /// is present on disk.
    #[must_use]
    pub fn new(
        config: ApplianceConfig,
        credential: AdminCredential,
        model_installed: bool,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                credential,
                model_installed,
                runtime: Runtime::new(),
            })),
        }
    }

    /// Checks a secret presented by a client against the appliance
    /// credential.
    ///
    /// Verification is deliberately expensive (Argon2id), so callers must
    /// throttle it rather than expose it to unlimited attempts.
    #[must_use]
    pub fn verify_secret(&self, presented: &str) -> bool {
        self.lock().credential.verify(presented)
    }

    /// The installation facts, as they stand.
    #[must_use]
    pub fn readiness(&self) -> Readiness {
        let inner = self.lock();
        Readiness::evaluate(&inner.config, inner.model_installed)
    }

    /// What the appliance is doing at the product level.
    #[must_use]
    pub fn phase(&self) -> Phase {
        let inner = self.lock();
        Phase::of(
            Readiness::evaluate(&inner.config, inner.model_installed),
            inner.config.onboarding_completed,
        )
    }

    /// Runs `f` against the stream guard, so callers can start or stop an
    /// activity without the guard escaping the shared lock.
    pub fn with_runtime<T>(&self, f: impl FnOnce(&mut Runtime) -> T) -> T {
        f(&mut self.lock().runtime)
    }

    fn status(&self) -> StatusResponse {
        let inner = self.lock();
        let readiness = Readiness::evaluate(&inner.config, inner.model_installed);
        let config = &inner.config;
        StatusResponse {
            kit_id: config.identity.kit_id.clone(),
            site_name: config.identity.site_name.clone(),
            phase: Phase::of(readiness, config.onboarding_completed),
            readiness,
            runtime: inner.runtime.mode().clone(),
            model_installed: inner.model_installed,
            sensor_ap: SensorApView {
                ssid: config.network.sensor_ap.ssid.clone(),
                channel: config.network.sensor_ap.channel,
            },
            uplink: UplinkView::of(config.network.uplink.as_ref()),
            nodes: config
                .nodes
                .iter()
                .map(|node| NodeView {
                    node_id: node.node_id.clone(),
                    role: node.role,
                    mac: node.mac.clone(),
                    address: node.address.map(|address| address.to_string()),
                })
                .collect(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A poisoned lock means a handler panicked mid-read; the state it
        // guards is plain data that stays coherent, so recovering the
        // guard keeps the appliance serving rather than failing every
        // subsequent request.
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Builds the status router.
pub fn router(state: EdgeState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn status(State(state): State<EdgeState>) -> Json<StatusResponse> {
    Json(state.status())
}

#[derive(Serialize)]
struct StatusResponse {
    kit_id: String,
    site_name: Option<String>,
    phase: Phase,
    readiness: Readiness,
    runtime: RuntimeMode,
    model_installed: bool,
    sensor_ap: SensorApView,
    uplink: UplinkView,
    nodes: Vec<NodeView>,
}

#[derive(Serialize)]
struct SensorApView {
    ssid: String,
    channel: u8,
}

/// The uplink as the dashboard sees it: its shape, never its secrets.
#[derive(Serialize)]
struct UplinkView {
    mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ssid: Option<String>,
}

impl UplinkView {
    fn of(uplink: Option<&Uplink>) -> Self {
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
struct NodeView {
    node_id: String,
    role: flow_core::NodeRole,
    mac: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<String>,
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use flow_core::NodeRole;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    use super::*;
    use crate::config::{Addressing, PairedNode, SiteTuning, Uplink, WifiSecurity};
    use crate::state::Stage;

    const AP_PASSPHRASE: &str = "correct-horse-battery";
    const UPLINK_PASSPHRASE: &str = "campus-secret-value";

    fn factory() -> ApplianceConfig {
        ApplianceConfig::factory("KIT-0001", "mariam-flow-0001", AP_PASSPHRASE)
    }

    fn installed() -> ApplianceConfig {
        let mut config = factory();
        config.identity.site_name = Some("RU EFREI".into());
        config.nodes = vec![
            PairedNode {
                node_id: "tx-1".into(),
                role: NodeRole::Tx,
                mac: "1a:00:00:00:00:00".into(),
                address: None,
            },
            PairedNode {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                mac: "aa:bb:cc:00:00:01".into(),
                address: Some("192.168.4.51".parse().unwrap()),
            },
        ];
        config.network.uplink = Some(Uplink::Wifi {
            ssid: "campus".into(),
            security: WifiSecurity::WpaPersonal {
                passphrase: UPLINK_PASSPHRASE.into(),
            },
            addressing: Addressing::Dhcp,
        });
        config.site = Some(SiteTuning {
            people_per_class: [0.0, 4.0, 12.0, 25.0],
            service_rate_per_min: 6.0,
            smoothing_tau_s: 30.0,
            hysteresis_margin: 0.15,
            min_confidence: 0.5,
            window_us: 5_000_000,
            hop_us: 1_000_000,
        });
        config.onboarding_completed = true;
        config
    }

    const TEST_SECRET: &str = "K7M4-9PQR-2WXY-6BTN-3HFD";

    fn state_for(config: ApplianceConfig, model_installed: bool) -> EdgeState {
        let secret = crate::secret::DeviceSecret::parse(TEST_SECRET).unwrap();
        let credential = AdminCredential::establish(&secret, 1_800_000_000_000_000).unwrap();
        EdgeState::new(config, credential, model_installed)
    }

    async fn get(state: EdgeState, path: &str) -> (StatusCode, String) {
        let request = axum::http::Request::builder()
            .uri(path)
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router(state).oneshot(request).await.unwrap();
        let code = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (code, String::from_utf8(bytes.to_vec()).unwrap())
    }

    async fn get_json(state: EdgeState, path: &str) -> serde_json::Value {
        let (code, text) = get(state, path).await;
        assert_eq!(code, StatusCode::OK);
        serde_json::from_str(&text).unwrap()
    }

    #[tokio::test]
    async fn health_answers_ok() {
        let body = get_json(state_for(factory(), false), "/health").await;
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn a_factory_appliance_reports_the_first_onboarding_step() {
        let body = get_json(state_for(factory(), false), "/api/status").await;
        assert_eq!(body["kit_id"], "KIT-0001");
        assert_eq!(body["phase"]["phase"], "onboarding");
        assert_eq!(body["phase"]["stage"], "site");
        assert_eq!(body["uplink"]["mode"], "undecided");
        assert_eq!(body["runtime"]["mode"], "idle");
        assert_eq!(body["model_installed"], false);
        assert_eq!(body["sensor_ap"]["channel"], 6);
        assert!(body["nodes"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_installed_appliance_reports_operational_state() {
        let body = get_json(state_for(installed(), true), "/api/status").await;
        assert_eq!(body["phase"]["phase"], "operational");
        assert_eq!(body["site_name"], "RU EFREI");
        assert_eq!(body["uplink"]["mode"], "wifi");
        assert_eq!(body["uplink"]["ssid"], "campus");
        assert_eq!(body["readiness"]["model_ready"], true);
        assert_eq!(body["nodes"][0]["node_id"], "tx-1");
        assert_eq!(body["nodes"][1]["address"], "192.168.4.51");
    }

    #[tokio::test]
    async fn the_status_surface_never_leaks_credentials() {
        let (_, text) = get(state_for(installed(), true), "/api/status").await;
        assert!(
            !text.contains(AP_PASSPHRASE),
            "sensor AP passphrase leaked into the status surface"
        );
        assert!(
            !text.contains(UPLINK_PASSPHRASE),
            "uplink passphrase leaked into the status surface"
        );
        assert!(!text.contains("passphrase"), "no passphrase field at all");
    }

    #[tokio::test]
    async fn the_reported_runtime_mode_follows_the_stream_guard() {
        let state = state_for(installed(), true);
        state
            .with_runtime(|runtime| runtime.start_calibration("s-001"))
            .unwrap();

        let body = get_json(state.clone(), "/api/status").await;
        assert_eq!(body["runtime"]["mode"], "calibrating");
        assert_eq!(body["runtime"]["session_id"], "s-001");

        state.with_runtime(Runtime::stop);
        let body = get_json(state, "/api/status").await;
        assert_eq!(body["runtime"]["mode"], "idle");
    }

    #[tokio::test]
    async fn readiness_is_reported_even_when_the_installation_is_closed() {
        let mut config = installed();
        config.nodes.clear();
        let state = state_for(config, true);

        assert_eq!(state.phase(), Phase::Operational);
        assert_eq!(state.readiness().stage(), Stage::Nodes);

        let body = get_json(state, "/api/status").await;
        assert_eq!(body["phase"]["phase"], "operational");
        assert_eq!(body["readiness"]["nodes_paired"], false);
    }
}
