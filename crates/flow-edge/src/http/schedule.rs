//! When the site serves.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::refusal;
use crate::edge_state::EdgeState;
use crate::journal::{Event, EventKind};
use crate::schedule::ServiceWindow;

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
