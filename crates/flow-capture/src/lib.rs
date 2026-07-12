//! Labeled capture: session recording plus the live labeling web page.
//!
//! During a supervised calibration session, the edge records the CSI
//! stream into a canonical session while an installer labels the live
//! density class from a phone. Both must share one session and one clock:
//! label timestamps are assigned by the edge at HTTP reception — the
//! phone's clock is never trusted, exactly like the sensing nodes'.
//!
//! The web page is a single embedded vanilla-HTML file (no framework, no
//! build step); the API is three routes:
//!
//! - `GET /` — the labeling page (four large buttons plus a status bar);
//! - `GET /status` — session state, polled by the page;
//! - `POST /label {"class": 0..3}` — appends a label to the running
//!   session, timestamped by the edge.
//!
//! The tool binds to the LAN (the phone connects over Wi-Fi) and is only
//! run during calibration; it serves no CSI data.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use flow_core::{CsiFrame, DensityClass, Label, SessionMeta};
use flow_ingest::session::{SessionError, SessionSummary, SessionWriter};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const UI_HTML: &str = include_str!("ui.html");

/// Failure while labeling.
#[derive(Debug, Error)]
pub enum LabelError {
    /// The class value is outside `0..=3`.
    #[error("invalid class value {0}")]
    InvalidClass(u8),
    /// The capture has ended; the session is sealed.
    #[error("capture has ended")]
    Ended,
    /// The underlying session write failed.
    #[error(transparent)]
    Session(#[from] SessionError),
}

struct Inner {
    writer: Option<SessionWriter>,
    session_id: String,
    class_mapping: flow_core::ClassMapping,
    started_us: u64,
    frames: u64,
    labels: u64,
    stream_alive: bool,
    current_class: Option<DensityClass>,
    current_since_us: Option<u64>,
}

/// State shared between the capture thread and the HTTP handlers.
#[derive(Clone)]
pub struct CaptureState {
    inner: Arc<Mutex<Inner>>,
}

impl CaptureState {
    /// Wraps a freshly created session writer.
    #[must_use]
    pub fn new(writer: SessionWriter, meta: &SessionMeta, started_us: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                writer: Some(writer),
                session_id: meta.session_id.clone(),
                class_mapping: meta.class_mapping.clone(),
                started_us,
                frames: 0,
                labels: 0,
                stream_alive: true,
                current_class: None,
                current_since_us: None,
            })),
        }
    }

    /// Appends one frame to the session (called by the capture thread).
    ///
    /// # Errors
    ///
    /// [`LabelError::Ended`] once the session is sealed, or the underlying
    /// session error.
    pub fn record_frame(&self, frame: &CsiFrame) -> Result<(), LabelError> {
        let mut inner = self.lock();
        let writer = inner.writer.as_mut().ok_or(LabelError::Ended)?;
        writer.write_frame(frame)?;
        inner.frames += 1;
        Ok(())
    }

    /// Marks the input stream as ended (frames will no longer arrive).
    pub fn mark_stream_ended(&self) {
        self.lock().stream_alive = false;
    }

    /// Seals the session; further labels are refused. Returns `None` if
    /// already finalized.
    ///
    /// # Errors
    ///
    /// The underlying finalization error.
    pub fn finalize(&self) -> Result<Option<SessionSummary>, SessionError> {
        let writer = {
            let mut inner = self.lock();
            inner.stream_alive = false;
            inner.writer.take()
        };
        match writer {
            None => Ok(None),
            Some(writer) => writer.finalize().map(Some),
        }
    }

    fn label(&self, class_value: u8) -> Result<(), LabelError> {
        let class = DensityClass::try_from(class_value)
            .map_err(|_| LabelError::InvalidClass(class_value))?;
        let ts_us = now_us();
        let mut inner = self.lock();
        let writer = inner.writer.as_mut().ok_or(LabelError::Ended)?;
        writer.write_label(&Label {
            ts_us,
            class,
            count: None,
        })?;
        inner.labels += 1;
        inner.current_class = Some(class);
        inner.current_since_us = Some(ts_us);
        Ok(())
    }

    fn status(&self) -> StatusResponse {
        let inner = self.lock();
        StatusResponse {
            now_us: now_us(),
            session_id: inner.session_id.clone(),
            recording: inner.writer.is_some(),
            stream_alive: inner.stream_alive,
            frames: inner.frames,
            labels: inner.labels,
            duration_s: now_us().saturating_sub(inner.started_us) / 1_000_000,
            current_class: inner.current_class.map(DensityClass::as_u8),
            current_since_us: inner.current_since_us,
            class_mapping: ClassMappingDto {
                empty: inner.class_mapping.empty.clone(),
                low: inner.class_mapping.low.clone(),
                medium: inner.class_mapping.medium.clone(),
                saturated: inner.class_mapping.saturated.clone(),
            },
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // A poisoned mutex means a writer panicked mid-operation; the
        // session data is what matters and is already on disk, so
        // continuing with the recovered guard is the right call here.
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[derive(Serialize)]
struct StatusResponse {
    /// Server clock at response time — lets the page estimate the
    /// phone-vs-edge clock skew for its live elapsed counter.
    now_us: u64,
    session_id: String,
    recording: bool,
    stream_alive: bool,
    frames: u64,
    labels: u64,
    duration_s: u64,
    current_class: Option<u8>,
    current_since_us: Option<u64>,
    class_mapping: ClassMappingDto,
}

#[derive(Serialize)]
struct ClassMappingDto {
    empty: String,
    low: String,
    medium: String,
    saturated: String,
}

#[derive(Deserialize)]
struct LabelRequest {
    class: u8,
}

#[derive(Serialize)]
struct LabelResponse {
    ok: bool,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Builds the labeler router.
pub fn router(state: CaptureState) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/status", get(status))
        .route("/label", post(label))
        .with_state(state)
}

async fn page() -> Html<&'static str> {
    Html(UI_HTML)
}

async fn status(State(state): State<CaptureState>) -> Json<StatusResponse> {
    Json(state.status())
}

async fn label(
    State(state): State<CaptureState>,
    Json(request): Json<LabelRequest>,
) -> Result<Json<LabelResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.label(request.class) {
        Ok(()) => Ok(Json(LabelResponse { ok: true })),
        Err(err) => {
            let code = match &err {
                LabelError::InvalidClass(_) => StatusCode::BAD_REQUEST,
                LabelError::Ended => StatusCode::CONFLICT,
                LabelError::Session(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err((
                code,
                Json(ErrorResponse {
                    error: err.to_string(),
                }),
            ))
        }
    }
}

/// Current Unix time in microseconds (the edge clock labels are stamped
/// with).
#[must_use]
pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use flow_core::{ClassMapping, NodePlacement, NodeRole};
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    use super::*;

    fn meta() -> SessionMeta {
        SessionMeta {
            session_id: "s-label-001".into(),
            site: "lab-a".into(),
            environment: "test".into(),
            wifi_channel: 6,
            nodes: vec![NodePlacement {
                node_id: "rx-1".into(),
                role: NodeRole::Rx,
                position: "desk".into(),
            }],
            firmware_version: "0.1.0".into(),
            software_version: "0.1.0".into(),
            class_mapping: ClassMapping {
                empty: "0 people".into(),
                low: "1 person".into(),
                medium: "2 people".into(),
                saturated: "3+ people".into(),
            },
        }
    }

    fn state(root: &std::path::Path) -> CaptureState {
        let meta = meta();
        let writer = SessionWriter::create(root, &meta).unwrap();
        CaptureState::new(writer, &meta, now_us())
    }

    async fn request(
        router: Router,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let mut builder = axum::http::Request::builder().method(method).uri(path);
        let body = match body {
            Some(json) => {
                builder = builder.header("content-type", "application/json");
                axum::body::Body::from(json.to_owned())
            }
            None => axum::body::Body::empty(),
        };
        let response = router.oneshot(builder.body(body).unwrap()).await.unwrap();
        let code = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (code, value)
    }

    #[tokio::test]
    async fn page_is_served() {
        let root = tempfile::tempdir().unwrap();
        let response = router(state(root.path()))
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("csi-capture"));
        assert!(html.contains("saturated"));
    }

    #[tokio::test]
    async fn status_reflects_a_fresh_session() {
        let root = tempfile::tempdir().unwrap();
        let (code, body) = request(router(state(root.path())), "GET", "/status", None).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["recording"], true);
        assert_eq!(body["frames"], 0);
        assert_eq!(body["labels"], 0);
        assert_eq!(body["current_class"], serde_json::Value::Null);
        assert_eq!(body["class_mapping"]["medium"], "2 people");
        assert!(body["now_us"].as_u64().unwrap() > 1_700_000_000_000_000);
    }

    #[tokio::test]
    async fn labels_are_written_with_edge_timestamps() {
        let root = tempfile::tempdir().unwrap();
        let state = state(root.path());

        let (code, body) = request(
            router(state.clone()),
            "POST",
            "/label",
            Some(r#"{"class":2}"#),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(body["ok"], true);
        let (_, status) = request(router(state.clone()), "GET", "/status", None).await;
        assert_eq!(status["labels"], 1);
        assert_eq!(status["current_class"], 2);

        let summary = state.finalize().unwrap().unwrap();
        let labels = std::fs::read_to_string(summary.path.join("labels.ndjson")).unwrap();
        let label: Label = serde_json::from_str(labels.lines().next().unwrap()).unwrap();
        assert_eq!(label.class, DensityClass::Medium);
        assert!(label.ts_us > 1_700_000_000_000_000, "edge-stamped");
    }

    #[tokio::test]
    async fn invalid_class_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let (code, body) = request(
            router(state(root.path())),
            "POST",
            "/label",
            Some(r#"{"class":9}"#),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        assert!(body["error"].as_str().unwrap().contains("invalid class"));
    }

    #[tokio::test]
    async fn labeling_after_finalize_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let state = state(root.path());
        state.finalize().unwrap().unwrap();
        assert!(state.finalize().unwrap().is_none(), "idempotent");

        let (code, body) = request(router(state), "POST", "/label", Some(r#"{"class":1}"#)).await;
        assert_eq!(code, StatusCode::CONFLICT);
        assert!(body["error"].as_str().unwrap().contains("ended"));
    }

    #[tokio::test]
    async fn frames_flow_through_the_shared_state() {
        let root = tempfile::tempdir().unwrap();
        let state = state(root.path());
        let frame =
            CsiFrame::new(now_us(), "rx-1", -52, 7, vec![1.0, 2.0], vec![0.0, 0.1]).unwrap();
        state.record_frame(&frame).unwrap();
        state.mark_stream_ended();

        let (_, status) = request(router(state.clone()), "GET", "/status", None).await;
        assert_eq!(status["frames"], 1);
        assert_eq!(status["stream_alive"], false);
        assert_eq!(status["recording"], true);

        let summary = state.finalize().unwrap().unwrap();
        assert_eq!(summary.frames, 1);
    }
}
