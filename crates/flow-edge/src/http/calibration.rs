//! Recording a labelled capture, and what becomes of it afterwards.

use std::net::SocketAddr;

use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::{Rename, error_response, refusal};
use crate::calibration::{
    RecordedSession, SessionRequest, recorded_sessions, session_id, session_meta, write_archive,
};
use crate::edge_state::EdgeState;
use crate::journal::{Event, EventKind};
use crate::now_us;
use flow_core::{DensityClass, Label};
use flow_ingest::SessionWriter;

/// Lists the captures recorded at this site, newest first.
pub(super) async fn sessions(State(state): State<EdgeState>) -> Json<Vec<RecordedSession>> {
    let root = state.data_dir().join("sessions");
    Json(recorded_sessions(&root))
}

/// Removes a recorded capture.
///
/// Offered because a truncated or mistaken capture is dead weight on a card
/// shared with everything else the appliance stores.
pub(super) async fn delete_session(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Response {
    let root = state.data_dir().join("sessions");
    let Some(dir) = session_dir(&root, &session_id) else {
        return error_response(StatusCode::BAD_REQUEST, "not a session identifier");
    };
    if !dir.is_dir() {
        return error_response(StatusCode::NOT_FOUND, "no such session");
    }
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::CalibrationStopped)
                    .from_client(peer.ip())
                    .with_detail(format!("session deleted: {session_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not delete the session: {err}"),
        ),
    }
}

/// Renames a recorded capture.
pub(super) async fn rename_session(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(rename): Json<Rename>,
) -> Response {
    let Some(name) = rename.trimmed() else {
        return error_response(StatusCode::BAD_REQUEST, "a name is required");
    };
    let root = state.data_dir().join("sessions");
    let Some(dir) = session_dir(&root, &session_id) else {
        return error_response(StatusCode::BAD_REQUEST, "not a session identifier");
    };
    if !dir.is_dir() {
        return error_response(StatusCode::NOT_FOUND, "no such session");
    }
    match crate::calibration::rename_session(&dir, name) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail(format!("session renamed: {session_id}")),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not rename the session: {err}"),
        ),
    }
}

/// The directory a session identifier names, or nothing if it names anything
/// else.
///
/// Refused rather than sanitised: the identifier is a directory name, and a
/// value that could climb out of the sessions root is not a mistyped session,
/// it is not a session at all.
pub(super) fn session_dir(root: &std::path::Path, session_id: &str) -> Option<std::path::PathBuf> {
    if session_id.is_empty() || session_id.contains(['/', '\\']) || session_id.contains("..") {
        return None;
    }
    Some(root.join(session_id))
}

/// Sends one capture as a gzipped tar, for training elsewhere.
///
/// Written to a temporary file and streamed from it, rather than assembled in
/// memory: a capture runs to tens of megabytes, on a machine with 512 MB.
pub(super) async fn session_archive(
    State(state): State<EdgeState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> Response {
    let root = state.data_dir().join("sessions");
    let Some(session_dir) = session_dir(&root, &session_id) else {
        return error_response(StatusCode::BAD_REQUEST, "not a session identifier");
    };
    if !session_dir.is_dir() {
        return error_response(StatusCode::NOT_FOUND, "no such session");
    }

    let named = crate::calibration::archive_name(
        &session_id,
        &recorded_sessions(&root)
            .into_iter()
            .find(|session| session.session_id == session_id)
            .map(|session| session.environment)
            .unwrap_or_default(),
    );
    let archive_path = root.join(format!("{session_id}.tar.gz"));
    let built = {
        let (dir, id, path) = (
            session_dir.clone(),
            session_id.clone(),
            archive_path.clone(),
        );
        tokio::task::spawn_blocking(move || write_archive(&dir, &id, &path)).await
    };
    if !matches!(built, Ok(Ok(()))) {
        let _ = std::fs::remove_file(&archive_path);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not build the archive",
        );
    }

    let Ok(file) = tokio::fs::File::open(&archive_path).await else {
        let _ = std::fs::remove_file(&archive_path);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not read the archive",
        );
    };
    // Removed now: the open handle keeps it readable until the body is sent,
    // so no half-built archive survives a client that walks away.
    let _ = std::fs::remove_file(&archive_path);

    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    (
        [
            (header::CONTENT_TYPE, "application/gzip".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{named}\""),
            ),
        ],
        body,
    )
        .into_response()
}

/// Records what each density class means at this site.
///
/// A property of the queue rather than of one capture, so it is answered once
/// and copied into every session recorded afterwards.
pub(super) async fn set_classes(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(classes): Json<Option<flow_core::ClassMapping>>,
) -> Response {
    match state.write_config(|config| config.classes = classes) {
        Ok(()) => {
            state.record(
                Event::new(EventKind::ConfigurationChanged)
                    .from_client(peer.ip())
                    .with_detail("density classes described".to_owned()),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(rejection) => refusal(&rejection),
    }
}

/// Starts recording a labeled capture session.
///
/// The session directory is created here rather than on the intake thread, so
/// a full card or a name already taken is answered to the caller instead of
/// failing out of sight.
///
/// Estimation is not torn down: the intake skips that stage while a capture is
/// running and resumes on its own when the capture ends, which is why nothing
/// has to remember whether it was estimating beforehand.
pub(super) async fn start_calibration(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(request): Json<SessionRequest>,
) -> Response {
    let (config, data_dir) = (state.config_snapshot(), state.data_dir());
    if config.rx_node_ids().is_empty() {
        return error_response(StatusCode::CONFLICT, "no receiver is paired");
    }
    // Refused rather than started empty: a capture recorded while nothing is
    // being read produces a session with labels and no frames, and the person
    // labelling would spend the hour finding out afterwards.
    if !state.stream_health().running {
        return error_response(
            StatusCode::CONFLICT,
            "the appliance is not reading any stream",
        );
    }

    let id = session_id(now_us(), &config.identity.kit_id);
    if let Err(err) = state.begin_calibration(&id, now_us()) {
        return error_response(StatusCode::CONFLICT, &err.to_string());
    }

    let meta = session_meta(&config, &request, id.clone());
    match SessionWriter::create(&data_dir.join("sessions"), &meta) {
        Ok(writer) => {
            state.attach_recorder(writer);
            state.record(
                Event::new(EventKind::CalibrationStarted)
                    .from_client(peer.ip())
                    .with_detail(id.clone()),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(err) => {
            // The runtime was claimed a moment ago; releasing it here keeps a
            // failed start from leaving the appliance unable to try again.
            state.end_calibration();
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("could not open the session: {err}"),
            )
        }
    }
}

/// Ends the capture and seals its directory.
pub(super) async fn stop_calibration(
    State(state): State<EdgeState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Response {
    let Some(writer) = state.detach_recorder() else {
        return error_response(StatusCode::CONFLICT, "no capture is running");
    };
    state.end_calibration();

    match writer.finalize() {
        Ok(summary) => {
            // The site counts as captured from here, not from the next boot:
            // sealing is what makes the session usable, and the installation
            // waits on exactly that.
            state.mark_site_captured();
            state.record(
                Event::new(EventKind::CalibrationStopped)
                    .from_client(peer.ip())
                    .with_detail(format!(
                        "{} frames, {} labels",
                        summary.frames, summary.labels
                    )),
            );
            (StatusCode::OK, Json(state.status())).into_response()
        }
        Err(err) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not seal the session: {err}"),
        ),
    }
}

#[derive(Deserialize)]
pub(super) struct LabelBody {
    class: DensityClass,
}

/// Annotates the capture with what is being observed right now.
///
/// Stamped by the appliance clock, never by the caller's: the phone doing the
/// labeling and the appliance recording the frames are two machines, and a
/// label has to land on the same timeline as the frames it describes.
pub(super) async fn add_label(
    State(state): State<EdgeState>,
    Json(body): Json<LabelBody>,
) -> Response {
    let label = Label {
        ts_us: now_us(),
        class: body.class,
        count: None,
    };
    match state.record_label(&label) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(message) => error_response(StatusCode::CONFLICT, &message),
    }
}
