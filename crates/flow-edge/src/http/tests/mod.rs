//! The HTTP surface, exercised through the router it is mounted on.

use std::net::{Ipv4Addr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, StatusCode, header};
use flow_core::{DensityClass, NodeRole};
use http_body_util::BodyExt;
use serde_json::json;
use tower::util::ServiceExt;

use super::journal::{MAX_EVENT_LIMIT, csv_field, utc_instant};
use super::live::PUBLIC_MAX_AGE_US;
use super::*;
use crate::config::{Addressing, ApplianceConfig, PairedNode, SiteTuning, Uplink, WifiSecurity};
use crate::credential::AdminCredential;
use crate::journal::{Event, EventKind, EventPage, Journal};
use crate::lifecycle::{Phase, Runtime, Stage};
use crate::now_us;
use crate::secret::DeviceSecret;

mod auth;
mod calibration;
mod install;
mod journal;
mod live;
mod models;

const AP_PASSPHRASE: &str = "correct-horse-battery";

const UPLINK_PASSPHRASE: &str = "campus-secret-value";

const SECRET: &str = "K7M4-9PQR-2WXY-6BTN-3HFD";

const WRONG_SECRET: &str = "K7M4-9PQR-2WXY-6BTN-3HFE";

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
            mac: Some("1a:00:00:00:00:00".into()),
            address: None,
            position: None,
        },
        PairedNode {
            node_id: "rx-1".into(),
            role: NodeRole::Rx,
            mac: Some("aa:bb:cc:00:00:01".into()),
            address: Some("192.168.4.51".parse().unwrap()),
            position: None,
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

fn state_for(config: ApplianceConfig, model_installed: bool) -> EdgeState {
    let secret = DeviceSecret::parse(SECRET).unwrap();
    let credential = AdminCredential::establish(&secret, now_us()).unwrap();
    let journal = Journal::open_in_memory().unwrap();
    // Tests that write configuration supply a real path of their own;
    // the rest never reach the disk.
    EdgeState::new(
        config,
        std::path::PathBuf::from("/nonexistent/appliance.json"),
        std::path::PathBuf::from("/nonexistent"),
        credential,
        model_installed,
        journal,
    )
}

/// Sends a request, optionally with a cookie and a JSON body, from a
/// given client address.
async fn send(
    state: &EdgeState,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: Option<String>,
    client: u8,
) -> (StatusCode, HeaderMap, serde_json::Value) {
    let mut builder = axum::http::Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let body = match body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            axum::body::Body::from(json)
        }
        None => axum::body::Body::empty(),
    };
    let mut request = builder.body(body).unwrap();
    // The real server supplies this through `into_make_service_with_
    // connect_info`; the throttle keys on it.
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((
            Ipv4Addr::new(192, 168, 4, client),
            51_000,
        ))));

    let response = router(state.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, headers, value)
}

async fn login_with(state: &EdgeState, secret: &str, client: u8) -> (StatusCode, HeaderMap) {
    let body = serde_json::json!({ "secret": secret }).to_string();
    let (status, headers, _) = send(state, "POST", "/api/session", None, Some(body), client).await;
    (status, headers)
}

/// Logs in and returns the cookie to present on later requests.
async fn session_of(state: &EdgeState) -> String {
    let (status, headers) = login_with(state, SECRET, 10).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let set = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    set.split(';').next().unwrap().to_owned()
}

/// An appliance whose configuration can actually be written, with the
/// directory kept alive for the duration of the test.
fn writable() -> (EdgeState, tempfile::TempDir) {
    writable_with(installed(), true)
}

/// A state whose intake is reading, which starting a capture requires.
fn recording_ready() -> (EdgeState, tempfile::TempDir) {
    let (state, dir) = writable();
    state.set_stream_running(true);
    (state, dir)
}

fn writable_with(config: ApplianceConfig, model_installed: bool) -> (EdgeState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("appliance.json");
    config.save(&path).unwrap();

    let secret = DeviceSecret::parse(SECRET).unwrap();
    let credential = AdminCredential::establish(&secret, now_us()).unwrap();
    let journal = Journal::open_in_memory().unwrap();
    let state = EdgeState::new(
        config,
        path,
        dir.path().to_path_buf(),
        credential,
        model_installed,
        journal,
    );
    (state, dir)
}

async fn put(
    state: &EdgeState,
    path: &str,
    cookie: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let (status, _, value) =
        send(state, "PUT", path, Some(cookie), Some(body.to_string()), 10).await;
    (status, value)
}

fn stored(state: &EdgeState) -> ApplianceConfig {
    let path = state.config_path();
    ApplianceConfig::load(&path).unwrap()
}

fn journal_details(state: &EdgeState) -> String {
    // The journal batches writes for the SD card's sake, so a reader has
    // to ask for them the way the housekeeping task does.
    state.flush_journal();
    state
        .recent_events(&EventPage {
            limit: 50,
            ..EventPage::default()
        })
        .into_iter()
        .filter_map(|event| event.detail)
        .collect::<Vec<String>>()
        .join(" | ")
}

async fn post(
    state: &EdgeState,
    path: &str,
    cookie: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let (status, _, value) = send(
        state,
        "POST",
        path,
        Some(cookie),
        Some(body.to_string()),
        10,
    )
    .await;
    (status, value)
}

/// A state serving now, with `estimate` published.
///
/// An appliance with no declared schedule serves, which is what an
/// installation that has not reached the hours step does.
fn serving(estimate: flow_infer::WaitEstimate) -> (EdgeState, tempfile::TempDir) {
    let (state, dir) = writable();
    state.publish_estimate(estimate);
    (state, dir)
}

/// A schedule that is closed at every hour of every day.
fn never_open() -> serde_json::Value {
    serde_json::json!({
        "timezone": "Europe/Paris",
        "weekly": {
            "monday": [], "tuesday": [], "wednesday": [], "thursday": [],
            "friday": [], "saturday": [], "sunday": [],
        },
        "closures": [],
    })
}

fn estimate_at(ts_us: u64, reliable: bool) -> flow_infer::WaitEstimate {
    flow_infer::WaitEstimate {
        ts_us,
        wait_minutes: 6.5,
        people: 13.0,
        level: 2.1,
        display_class: DensityClass::Medium,
        confidence: 0.81,
        reliable,
    }
}

fn always_open() -> serde_json::Value {
    let day = serde_json::json!([{ "from": "00:00", "to": "23:59" }]);
    serde_json::json!({
        "timezone": "Europe/Paris",
        "weekly": {
            "monday": day, "tuesday": day, "wednesday": day, "thursday": day,
            "friday": day, "saturday": day, "sunday": day,
        },
        "closures": [],
    })
}
