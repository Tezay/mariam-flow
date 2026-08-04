//! Receiving a density model and choosing which one is in service.

use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::{Rename, error_response, refusal};
use crate::edge_state::EdgeState;
use crate::journal::{Event, EventKind};
use crate::model;
use crate::now_us;

/// Largest bundle the appliance will read into memory.
///
/// A classical model is kilobytes. The cap is not about real models but about
/// what an authenticated caller could otherwise ask a 512 MB machine to hold.
pub(super) const MAX_BUNDLE_BYTES: usize = 32 * 1024 * 1024;

/// Takes a trained model into service.
///
/// Validated, then activated in one move: an import that left the appliance
/// with a checked model it was not using would need a second visit to finish,
/// and the person who did the import has already left.
pub(super) async fn import_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    body: axum::body::Bytes,
) -> Response {
    let (data_dir, rx_nodes) = (state.data_dir(), state.config_snapshot().rx_node_ids());
    if rx_nodes.is_empty() {
        return error_response(StatusCode::CONFLICT, "no receiver is paired");
    }

    let staging = data_dir.join("model.staging");
    let staged = match model::stage(&body, &staging) {
        Ok(staged) => staged,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, &err.to_string()),
    };
    if let Err(err) = model::check(&staged, rx_nodes) {
        let _ = std::fs::remove_dir_all(&staging);
        return error_response(StatusCode::BAD_REQUEST, &err.to_string());
    }
    let stored = match model::store(&staged, &data_dir, now_us()) {
        Ok(id) => id,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&staging);
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string());
        }
    };
    let _ = std::fs::remove_dir_all(&staging);

    let named = staged
        .manifest
        .as_ref()
        .map_or_else(|| stored.clone(), |manifest| manifest.name.clone());
    put_in_service(&state, peer.ip(), &data_dir, &stored, &named)
}

/// Puts a stored model into service and lets the intake know.
///
/// Writing the configuration bumps its generation, which is what makes the
/// intake rebuild and pick the model up — the appliance starts estimating
/// without being restarted.
pub(super) fn put_in_service(
    state: &EdgeState,
    peer: IpAddr,
    data_dir: &std::path::Path,
    id: &str,
    named: &str,
) -> Response {
    let tuning = match model::activate(data_dir, id) {
        Ok(tuning) => tuning,
        Err(err) => return error_response(StatusCode::CONFLICT, &err.to_string()),
    };
    let handle = id.to_owned();
    if let Err(rejection) = state.write_config(|config| {
        config.site = Some(tuning);
        config.active_model = Some(handle);
    }) {
        return refusal(&rejection);
    }
    state.set_model_installed(true);
    state.record(
        Event::new(EventKind::ModelActivated)
            .from_client(peer)
            .with_detail(named.to_owned()),
    );
    (StatusCode::OK, Json(state.status())).into_response()
}

/// Lists every model the appliance holds, newest first.
pub(super) async fn models(State(state): State<EdgeState>) -> Json<Vec<model::StoredModel>> {
    let (data_dir, active) = (state.data_dir(), state.config_snapshot().active_model);
    Json(model::library(&data_dir, active.as_deref()))
}

/// Puts one of the stored models back into service.
pub(super) async fn use_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    let data_dir = state.data_dir();
    put_in_service(&state, peer.ip(), &data_dir, &model_id, &model_id)
}

/// Removes a stored model.
pub(super) async fn forget_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
) -> Response {
    let (data_dir, active) = (state.data_dir(), state.config_snapshot().active_model);
    // Refused rather than allowed to leave the appliance estimating from a
    // model nobody can name any more.
    if active.as_deref() == Some(model_id.as_str()) {
        return error_response(StatusCode::CONFLICT, "that model is in service");
    }
    match model::remove(&data_dir, &model_id) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("model removed: {model_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(StatusCode::NOT_FOUND, &err.to_string()),
    }
}

/// Renames a stored model.
pub(super) async fn rename_model(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(model_id): axum::extract::Path<String>,
    Json(rename): Json<Rename>,
) -> Response {
    let Some(name) = rename.trimmed() else {
        return error_response(StatusCode::BAD_REQUEST, "a name is required");
    };
    let data_dir = state.data_dir();
    match model::rename(&data_dir, &model_id, name) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("model renamed: {model_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(StatusCode::NOT_FOUND, &err.to_string()),
    }
}
