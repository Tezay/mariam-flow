//! The appliance HTTP surface: the router, and what every route shares.
//!
//! Access is **denied by default**. Three routes are open — the liveness
//! probe, the login endpoint, and the public estimate — and every other one
//! sits behind the session guard, so a route added later is protected unless
//! someone deliberately places it outside.
//!
//! Three rules bound what may cross this boundary:
//!
//! - **No credentials, ever.** The status surface reports network *shape* —
//!   which SSID, which mode — and never a passphrase, though the daemon holds
//!   them.
//! - **No raw CSI.** The privacy invariant of the whole system.
//! - **No unbounded verification.** Checking a secret costs an Argon2id hash,
//!   so attempts are throttled per client and serialised process-wide.

use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::{self};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::edge_state::{EdgeState, WriteRejection};
use crate::views::StatusResponse;

#[cfg(test)]
mod tests;

mod auth;
mod calibration;
mod install;
mod journal;
mod live;
mod models;

use self::auth::{login, logout, require_session, security_headers};
use self::calibration::{
    add_label, delete_session, rename_session, session_archive, sessions, set_classes,
    start_calibration, stop_calibration,
};
use self::install::{
    adopt_hardware, describe_node, discovery, service_window, set_installation, set_network_survey,
    set_nodes, set_service_window, set_site, set_uplink, system,
};
use self::journal::{events, events_csv};
use self::live::{estimates, live, public_estimate};
use self::models::{MAX_BUNDLE_BYTES, forget_model, import_model, models, rename_model, use_model};

/// Builds the appliance router.
///
/// Protected routes are grouped behind the session guard, so adding a route
/// to that group is enough to protect it.
pub fn router(state: EdgeState) -> Router {
    let protected = Router::new()
        .route("/api/status", get(status))
        .route("/api/events", get(events))
        .route("/api/events.csv", get(events_csv))
        .route("/api/live", get(live))
        .route("/api/estimates", get(estimates))
        .route("/api/discovery", get(discovery))
        .route("/api/system", get(system))
        .route("/api/site", put(set_site))
        .route("/api/nodes", put(set_nodes))
        .route("/api/nodes/{node_id}", axum::routing::patch(describe_node))
        .route("/api/nodes/{node_id}/hardware", post(adopt_hardware))
        .route("/api/uplink", put(set_uplink))
        .route("/api/network-survey", put(set_network_survey))
        .route(
            "/api/model",
            post(import_model).layer(axum::extract::DefaultBodyLimit::max(MAX_BUNDLE_BYTES)),
        )
        .route("/api/models", get(models))
        .route(
            "/api/models/{model_id}",
            axum::routing::delete(forget_model)
                .post(use_model)
                .patch(rename_model),
        )
        .route("/api/classes", put(set_classes))
        .route(
            "/api/calibration",
            post(start_calibration).delete(stop_calibration),
        )
        .route("/api/calibration/label", post(add_label))
        .route("/api/sessions", get(sessions))
        .route(
            "/api/sessions/{session_id}",
            axum::routing::delete(delete_session).patch(rename_session),
        )
        .route("/api/sessions/{session_id}/archive", get(session_archive))
        .route("/api/installation", put(set_installation))
        .route(
            "/api/service-window",
            get(service_window).put(set_service_window),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_session,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/estimate", get(public_estimate))
        .route("/api/session", post(login).delete(logout))
        .merge(protected)
        // Anything the API does not claim is the dashboard: its assets, or
        // one of its client-side routes.
        .fallback(crate::assets::serve)
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

/// Answers a refused write.
///
/// A storage failure is not the caller's to fix by sending something else, so
/// it is reported as the appliance's fault rather than theirs.
pub(super) fn refusal(rejection: &WriteRejection) -> Response {
    match rejection {
        WriteRejection::Invalid(reason) => error_response(StatusCode::BAD_REQUEST, reason),
        WriteRejection::Storage => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "the appliance could not store the change",
        ),
    }
}

#[derive(Serialize)]
pub(super) struct ErrorResponse {
    error: String,
}

pub(super) fn error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

#[derive(Serialize)]
pub(super) struct Health {
    status: &'static str,
}

pub(super) async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

pub(super) async fn status(State(state): State<EdgeState>) -> Json<StatusResponse> {
    Json(state.status())
}

/// A new name for something the appliance holds.
#[derive(Debug, Deserialize)]
pub(super) struct Rename {
    /// What it should be called from now on.
    name: String,
}

impl Rename {
    /// The trimmed name, or nothing if it says nothing.
    ///
    /// A blank name is refused rather than stored: a list of recordings where
    /// one row is empty is a list nobody can act on.
    fn trimmed(&self) -> Option<&str> {
        let name = self.name.trim();
        (!name.is_empty()).then_some(name)
    }
}
