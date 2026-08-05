//! Which sensors the appliance has, and which hardware answers for each.

use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::{error_response, refusal};
use crate::config::PairedNode;
use crate::discovery::Discovery;
use crate::edge_state::EdgeState;
use crate::journal::{Event, EventKind};
use flow_core::NodeRole;

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

/// Reports what is streaming that no node mapping claims.
///
/// Always available, never a mode: replacing a node on a running
/// installation must not mean stopping the estimation to find it again.
pub(super) async fn discovery(State(state): State<EdgeState>) -> Json<Discovery> {
    Json(state.discovery())
}
