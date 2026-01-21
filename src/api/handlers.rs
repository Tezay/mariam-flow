use crate::api::responses::{QueueErrorCode, QueueErrorResponse, QueueSuccessResponse};
use crate::state::{AppState, WaitTimeStatus};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::error;

const INTERNAL_ERROR_MESSAGE: &str = "Internal server error";

pub enum QueueResponse {
    Success(QueueSuccessResponse),
    Error { status: StatusCode, body: QueueErrorResponse },
}

impl IntoResponse for QueueResponse {
    fn into_response(self) -> Response {
        match self {
            QueueResponse::Success(body) => (StatusCode::OK, Json(body)).into_response(),
            QueueResponse::Error { status, body } => (status, Json(body)).into_response(),
        }
    }
}

pub async fn get_queue(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    build_queue_response(state)
}

fn build_queue_response(state: Arc<RwLock<AppState>>) -> QueueResponse {
    let guard = match state.read() {
        Ok(guard) => guard,
        Err(_) => {
            return internal_error("state lock poisoned while reading wait_time");
        }
    };
    let estimate = guard.wait_time().cloned();
    drop(guard);

    match estimate {
        Some(estimate) => {
            if estimate.status == WaitTimeStatus::Ok {
                if let Some(wait_time_minutes) = estimate.wait_time_minutes
                    && wait_time_minutes.is_finite()
                    && wait_time_minutes >= 0.0
                {
                    return success_response(wait_time_minutes, estimate.timestamp);
                }
                return internal_error("wait_time status ok but value missing or invalid");
            }
            no_data_response(estimate.timestamp)
        }
        None => no_data_response(SystemTime::now()),
    }
}

fn success_response(wait_time_minutes: f64, timestamp: SystemTime) -> QueueResponse {
    match format_timestamp(timestamp) {
        Ok(formatted) => QueueResponse::Success(QueueSuccessResponse {
            wait_time_minutes,
            queue_length: None,
            timestamp: formatted,
        }),
        Err(_err) => {
            internal_error("timestamp formatting failure")
        }
    }
}

fn no_data_response(timestamp: SystemTime) -> QueueResponse {
    match format_timestamp(timestamp) {
        Ok(formatted) => QueueResponse::Error {
            status: StatusCode::SERVICE_UNAVAILABLE,
            body: QueueErrorResponse {
                error_code: QueueErrorCode::NoData,
                error_message: "No wait time estimate available".to_string(),
                timestamp: formatted,
            },
        },
        Err(_err) => {
            internal_error("timestamp formatting failure")
        }
    }
}

fn internal_error(message: &str) -> QueueResponse {
    error!(message = message, "Internal error while handling /api/queue");
    let formatted = format_timestamp(SystemTime::now()).unwrap_or_else(|err| {
        error!(error = %err, "Failed to format internal error timestamp");
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    });
    QueueResponse::Error {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        body: QueueErrorResponse {
            error_code: QueueErrorCode::InternalError,
            error_message: INTERNAL_ERROR_MESSAGE.to_string(),
            timestamp: formatted,
        },
    }
}

fn format_timestamp(timestamp: SystemTime) -> Result<String, time::error::Format> {
    OffsetDateTime::from(timestamp).format(&Rfc3339)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, WaitTimeEstimate, WaitTimeStatus};
    use axum::http::StatusCode;
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn queue_handler_returns_success_when_wait_time_available() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_wait_time();
        let estimate = WaitTimeEstimate {
            wait_time_minutes: Some(7.0),
            timestamp: UNIX_EPOCH + Duration::from_secs(1),
            status: WaitTimeStatus::Ok,
            error_code: None,
        };
        app_state
            .set_wait_time(estimate)
            .expect("set wait time");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_queue_response(state);

        match response {
            QueueResponse::Success(body) => {
                assert_eq!(body.wait_time_minutes, 7.0);
                assert_eq!(body.queue_length, None);
                assert_eq!(body.timestamp, "1970-01-01T00:00:01Z");
            }
            QueueResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }

    #[test]
    fn queue_handler_returns_no_data_when_missing() {
        let state = Arc::new(RwLock::new(AppState::new()));

        let response = build_queue_response(state);

        match response {
            QueueResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(body.error_code, QueueErrorCode::NoData);
            }
            QueueResponse::Success(_) => {
                panic!("expected no data error response");
            }
        }
    }

    #[test]
    fn queue_handler_returns_internal_error_when_lock_poisoned() {
        let state = Arc::new(RwLock::new(AppState::new()));
        let state_for_thread = Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = state_for_thread.write().expect("lock for poison");
            panic!("poison lock");
        })
        .join();

        let response = build_queue_response(state);

        match response {
            QueueResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body.error_code, QueueErrorCode::InternalError);
                assert_eq!(body.error_message, "Internal server error");
            }
            QueueResponse::Success(_) => {
                panic!("expected internal error response");
            }
        }
    }

    #[test]
    fn queue_handler_returns_internal_error_when_wait_time_invalid() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_wait_time();
        let estimate = WaitTimeEstimate {
            wait_time_minutes: None,
            timestamp: UNIX_EPOCH + Duration::from_secs(1),
            status: WaitTimeStatus::Ok,
            error_code: None,
        };
        app_state
            .set_wait_time(estimate)
            .expect("set wait time");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_queue_response(state);

        match response {
            QueueResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body.error_code, QueueErrorCode::InternalError);
                assert_eq!(body.error_message, "Internal server error");
            }
            QueueResponse::Success(_) => {
                panic!("expected internal error response");
            }
        }
    }
}
