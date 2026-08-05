//! Bringing an appliance into service: the site, its sensors, its network.

use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::{error_response, refusal};
use crate::config::{NetworkSurvey, PairedNode, Uplink};
use crate::discovery::Discovery;
use crate::edge_state::EdgeState;
use crate::journal::{Event, EventKind};
use crate::lifecycle::Stage;
use crate::schedule::ServiceWindow;
use crate::system::SystemReport;
use flow_core::NodeRole;

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

/// Replaces the paired nodes with the set the installer confirmed.
///
/// The whole list at once: identifiers, addresses and MACs must be unique
/// across it and only one node may transmit, so checking an addition against
/// a stored remainder would be checking half a thing.
pub(super) async fn set_nodes(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(nodes): Json<Vec<PairedNode>>,
) -> Response {
    let described = format!("{} node(s) paired", nodes.len());
    match state.write_config(|config| config.nodes = nodes) {
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

/// Where a node physically sits.
#[derive(Debug, Deserialize)]
pub(super) struct Placement {
    /// The operator's own words, or nothing to say it is unknown again.
    position: String,
}

/// Describes where one node sits.
pub(super) async fn describe_node(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    Json(placement): Json<Placement>,
) -> Response {
    let position = placement.position.trim();
    // A blank field is a statement — "I do not know where this one is" — and
    // not the same thing as a refused name.
    let position = (!position.is_empty()).then(|| position.to_owned());

    let mut known = false;
    let outcome = state.write_config(|config| {
        for node in &mut config.nodes {
            if node.node_id == node_id {
                node.position.clone_from(&position);
                known = true;
            }
        }
    });
    if !known {
        return error_response(StatusCode::NOT_FOUND, "no such node");
    }
    match outcome {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("node {node_id} placed")),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// The hardware now answering for a node.
#[derive(Debug, Deserialize)]
pub(super) struct Hardware {
    /// Source address of the replacement, for a receiver.
    #[serde(default)]
    address: Option<IpAddr>,
    /// Hardware address of the replacement, for a transmitter.
    #[serde(default)]
    mac: Option<String>,
}

/// Points an existing node at the hardware that replaced it.
///
/// The identifier is kept rather than the node re-paired from scratch: it is
/// what capture sessions are written against, and what a trained model was
/// validated for. A receiver renamed on replacement would leave the site with
/// a model that no longer fits it.
pub(super) async fn adopt_hardware(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    Json(hardware): Json<Hardware>,
) -> Response {
    let role = {
        let config = state.config_snapshot();
        config
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(|node| node.role)
    };
    let Some(role) = role else {
        return error_response(StatusCode::NOT_FOUND, "no such node");
    };

    // Each role is known by exactly one thing, so offering the other is a
    // mistake worth naming rather than a field to ignore.
    let described = match (role, &hardware.address, &hardware.mac) {
        (NodeRole::Rx, Some(address), None) => format!("{node_id} now at {address}"),
        (NodeRole::Tx, None, Some(mac)) => format!("{node_id} now {mac}"),
        (NodeRole::Rx, _, _) => {
            return error_response(StatusCode::BAD_REQUEST, "a receiver is adopted by address");
        }
        (NodeRole::Tx, _, _) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "a transmitter is adopted by MAC address",
            );
        }
    };

    match state.write_config(|config| {
        for node in &mut config.nodes {
            if node.node_id == node_id {
                match role {
                    NodeRole::Rx => node.address = hardware.address,
                    NodeRole::Tx => node.mac.clone_from(&hardware.mac),
                }
            }
        }
    }) {
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

/// Returns the stored service schedule, or `null` when none is declared.
///
/// The schedule is read here rather than from `/api/status` because only the
/// screen that edits it needs it: every other caller wants the resulting
/// state — open or closed — which the status already carries.
pub(super) async fn service_window(State(state): State<EdgeState>) -> Json<Option<ServiceWindow>> {
    Json(state.config_snapshot().service)
}

/// Replaces the service schedule, or clears it.
///
/// The whole schedule is sent at once rather than patched field by field:
/// its parts constrain one another — intervals must not overlap, a closure
/// must not end before it begins — so validating a fragment against a stored
/// remainder would be checking half a thing.
///
/// A `null` body clears the schedule, which returns the appliance to
/// estimating around the clock.
pub(super) async fn set_service_window(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(window): Json<Option<ServiceWindow>>,
) -> Response {
    let described = window.as_ref().map_or_else(
        || "service schedule cleared".to_owned(),
        |window| format!("service schedule set ({})", window.timezone),
    );

    match state.write_config(|config| config.service = window) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(described),
            );
            (StatusCode::OK, Json(state.service_state())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Reports what is streaming that no node mapping claims.
///
/// Always available, never a mode: replacing a node on a running
/// installation must not mean stopping the estimation to find it again.
pub(super) async fn discovery(State(state): State<EdgeState>) -> Json<Discovery> {
    Json(state.discovery())
}

/// Reports what the machine says about itself.
///
/// Read on each request rather than cached: uptime, load and temperature are
/// the point, and a stale temperature is worse than none.
pub(super) async fn system() -> Json<SystemReport> {
    Json(SystemReport::read())
}
