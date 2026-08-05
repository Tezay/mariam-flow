//! Bringing an appliance into service, and declaring it done.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::{error_response, refusal};
use crate::config::{NetworkSurvey, Uplink, WaitTuning};
use crate::edge_state::EdgeState;
use crate::journal::{Event, EventKind};
use crate::lifecycle::Stage;
use flow_core::ClassMapping;

#[derive(Deserialize)]
pub(super) struct SiteBody {
    site_name: String,
}

/// Names the site this appliance is installed at.
pub(super) async fn set_site(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<SiteBody>,
) -> Response {
    let name = body.site_name.trim().to_owned();
    match state.write_config(|config| config.identity.site_name = Some(name.clone())) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("site named {name:?}")),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Records how the appliance reaches the site network, or that it will not.
///
/// `null` returns the question to unanswered, which is a different state from
/// a deliberate `offline` and is what the installation progress reads.
pub(super) async fn set_uplink(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(uplink): Json<Option<Uplink>>,
) -> Response {
    // Named by shape, never by secret: the journal is readable by anyone who
    // can read the appliance, and a passphrase must not travel into it.
    let described = match uplink.as_ref() {
        None => "uplink question reopened".to_owned(),
        Some(Uplink::Offline) => "uplink set offline".to_owned(),
        Some(Uplink::Wifi { ssid, .. }) => format!("uplink set to Wi-Fi {ssid:?}"),
        Some(Uplink::Ethernet { .. }) => "uplink set to wired".to_owned(),
    };
    match state.write_config(|config| config.network.uplink = uplink) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Records what the site's network was found to ask for.
///
/// Separate from the uplink because it is a statement about the site rather
/// than about the appliance: it stays true when the appliance is left offline
/// precisely because the site asks for something it cannot yet offer, and the
/// request sent to the network administrator is built from it.
pub(super) async fn set_network_survey(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(survey): Json<Option<NetworkSurvey>>,
) -> Response {
    let described = survey.map_or_else(
        || "network survey cleared".to_owned(),
        |survey| format!("network survey recorded ({:?})", survey.authentication),
    );
    match state.write_config(|config| config.network.survey = survey) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// What the installer says about the queue at this site.
#[derive(Deserialize)]
pub(super) struct QueueBody {
    /// What each density looks like, in the operator's own words.
    #[serde(default)]
    classes: Option<ClassMapping>,
    wait: WaitTuning,
}

/// Records what the queue looks like and how fast it is served.
///
/// The words and the counts are answered together and stored together: a
/// screen that saved one without the other would leave the appliance able to
/// label a queue it cannot time, or the reverse.
pub(super) async fn set_queue(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<QueueBody>,
) -> Response {
    match state.write_config(|config| {
        config.classes = body.classes;
        config.wait = Some(body.wait);
    }) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!(
                        "queue described, {} served per minute",
                        body.wait.service_rate_per_min
                    )),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

#[derive(Deserialize)]
pub(super) struct InstallationBody {
    completed: bool,
}

/// Closes the installation, or reopens it.
///
/// Closing is refused while any step is outstanding: the flag only stops the
/// wizard reappearing, so setting it early would leave an operational screen
/// the appliance cannot honour. Reopening is always allowed.
pub(super) async fn set_installation(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<InstallationBody>,
) -> Response {
    if body.completed {
        let readiness = state.readiness();
        let stage = readiness.stage();
        if stage != Stage::Complete {
            return error_response(
                StatusCode::CONFLICT,
                &format!("{stage} is not finished yet"),
            );
        }
    }

    let described = if body.completed {
        "installation closed"
    } else {
        "installation reopened"
    };
    match state.write_config(|config| config.onboarding_completed = body.completed) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described.to_owned()),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}
