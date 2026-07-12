//! Local REST API for the Mariam Flow edge.
//!
//! Implemented: the live-estimate surface. `GET /estimate` is the public
//! contract — it only ever exposes reliable, fresh estimates and answers
//! `{"status":"unavailable"}` otherwise (the product decision: never show
//! a doubtful number). `GET /internal/estimate` exposes the full internal
//! state for operators, including unreliable estimates and the masking
//! reason. `GET /health` is a liveness probe.
//!
//! The API reads the latest estimate from a `tokio::sync::watch` channel
//! fed by the inference pipeline; see the `flow-api` binary for the full
//! daemon composition. Raw CSI is never exposed by this API.
//!
//! Planned: session management and control endpoints, outbound push of
//! aggregated estimates.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use flow_infer::WaitEstimate;
use serde::Serialize;
use tokio::sync::watch;

/// Latest-estimate channel: the pipeline writes, the API reads.
pub type EstimateSender = watch::Sender<Option<WaitEstimate>>;
/// Receiving side of the latest-estimate channel.
pub type EstimateReceiver = watch::Receiver<Option<WaitEstimate>>;

/// Creates the channel pair the daemon wires between pipeline and API.
#[must_use]
pub fn estimate_channel() -> (EstimateSender, EstimateReceiver) {
    watch::channel(None)
}

/// Shared state of the HTTP handlers.
#[derive(Clone)]
pub struct AppState {
    receiver: EstimateReceiver,
    max_age_us: Option<u64>,
}

impl AppState {
    /// Builds the state from the estimate channel and an optional maximum
    /// estimate age (staleness threshold) in µs.
    #[must_use]
    pub fn new(receiver: EstimateReceiver, max_age_us: Option<u64>) -> Self {
        Self {
            receiver,
            max_age_us,
        }
    }

    fn latest(&self) -> Option<WaitEstimate> {
        *self.receiver.borrow()
    }

    fn is_stale(&self, estimate: &WaitEstimate) -> bool {
        match self.max_age_us {
            None => false,
            Some(max_age) => now_us().saturating_sub(estimate.ts_us) > max_age,
        }
    }
}

/// Builds the API router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/estimate", get(estimate))
        .route("/internal/estimate", get(internal_estimate))
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

/// Public estimate contract: only reliable, fresh values are exposed.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum PublicEstimate {
    Available {
        wait_min: f32,
        class: String,
        confidence: f32,
        ts_us: u64,
    },
    Unavailable {},
}

async fn estimate(State(state): State<AppState>) -> Json<PublicEstimate> {
    let response = match state.latest() {
        Some(latest) if latest.reliable && !state.is_stale(&latest) => PublicEstimate::Available {
            wait_min: latest.wait_minutes,
            class: latest.display_class.to_string(),
            confidence: latest.confidence,
            ts_us: latest.ts_us,
        },
        _ => PublicEstimate::Unavailable {},
    };
    Json(response)
}

/// Operator view: the full internal state and why the public endpoint
/// masks it, if it does.
#[derive(Serialize)]
struct InternalEstimate {
    estimate: Option<InternalFields>,
    masked: bool,
    masked_reason: Option<&'static str>,
}

#[derive(Serialize)]
struct InternalFields {
    ts_us: u64,
    wait_min: f32,
    people: f32,
    level: f32,
    class: String,
    confidence: f32,
    reliable: bool,
}

async fn internal_estimate(State(state): State<AppState>) -> Json<InternalEstimate> {
    let latest = state.latest();
    let (masked, reason) = match &latest {
        None => (true, Some("no estimate yet")),
        Some(estimate) if !estimate.reliable => (true, Some("low confidence")),
        Some(estimate) if state.is_stale(estimate) => (true, Some("stale")),
        Some(_) => (false, None),
    };
    Json(InternalEstimate {
        estimate: latest.map(|e| InternalFields {
            ts_us: e.ts_us,
            wait_min: e.wait_minutes,
            people: e.people,
            level: e.level,
            class: e.display_class.to_string(),
            confidence: e.confidence,
            reliable: e.reliable,
        }),
        masked,
        masked_reason: reason,
    })
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use flow_core::DensityClass;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    use super::*;

    fn estimate_at(ts_us: u64, reliable: bool) -> WaitEstimate {
        WaitEstimate {
            ts_us,
            wait_minutes: 2.65,
            people: 10.6,
            level: 1.9,
            display_class: DensityClass::Medium,
            confidence: 0.87,
            reliable,
        }
    }

    async fn get_json(router: Router, path: &str) -> serde_json::Value {
        let request = axum::http::Request::builder()
            .uri(path)
            .body(axum::body::Body::empty())
            .unwrap();
        let response = router.oneshot(request).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn health_answers_ok() {
        let (_tx, rx) = estimate_channel();
        let body = get_json(router(AppState::new(rx, None)), "/health").await;
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn estimate_is_unavailable_before_any_data() {
        let (_tx, rx) = estimate_channel();
        let body = get_json(router(AppState::new(rx, None)), "/estimate").await;
        assert_eq!(body["status"], "unavailable");
    }

    #[tokio::test]
    async fn reliable_fresh_estimate_is_served() {
        let (tx, rx) = estimate_channel();
        tx.send(Some(estimate_at(now_us(), true))).unwrap();
        let body = get_json(router(AppState::new(rx, Some(15_000_000))), "/estimate").await;
        assert_eq!(body["status"], "available");
        assert_eq!(body["class"], "medium");
        assert!((body["wait_min"].as_f64().unwrap() - 2.65).abs() < 1e-6);
    }

    #[tokio::test]
    async fn unreliable_estimate_is_masked_publicly_but_visible_internally() {
        let (tx, rx) = estimate_channel();
        tx.send(Some(estimate_at(now_us(), false))).unwrap();
        let state = AppState::new(rx, None);

        let public = get_json(router(state.clone()), "/estimate").await;
        assert_eq!(public["status"], "unavailable");
        assert!(public.get("wait_min").is_none(), "must not leak values");

        let internal = get_json(router(state), "/internal/estimate").await;
        assert_eq!(internal["masked"], true);
        assert_eq!(internal["masked_reason"], "low confidence");
        assert_eq!(internal["estimate"]["reliable"], false);
        assert!((internal["estimate"]["people"].as_f64().unwrap() - 10.6).abs() < 1e-5);
    }

    #[tokio::test]
    async fn stale_estimate_is_masked() {
        let (tx, rx) = estimate_channel();
        // An estimate stamped at t=0 is far older than any max age.
        tx.send(Some(estimate_at(0, true))).unwrap();
        let state = AppState::new(rx, Some(15_000_000));

        let public = get_json(router(state.clone()), "/estimate").await;
        assert_eq!(public["status"], "unavailable");

        let internal = get_json(router(state), "/internal/estimate").await;
        assert_eq!(internal["masked_reason"], "stale");
    }

    #[tokio::test]
    async fn staleness_check_can_be_disabled() {
        let (tx, rx) = estimate_channel();
        tx.send(Some(estimate_at(0, true))).unwrap();
        let body = get_json(router(AppState::new(rx, None)), "/estimate").await;
        assert_eq!(body["status"], "available");
    }
}
