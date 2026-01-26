use crate::api::responses::{
    HealthErrorCode, HealthErrorResponse, HealthStatus, HealthSuccessResponse, QueueErrorCode,
    QueueErrorResponse, QueueSuccessResponse, SensorErrorCode, SensorStatus, SensorStatusResponse,
    SensorsErrorCode, SensorsErrorResponse, SensorsSuccessResponse,
};
use crate::sensor::{I2C_7BIT_MAX, SensorStatus as DeviceSensorStatus};
use crate::state::{AppState, WaitTimeStatus};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tracing::error;

const INTERNAL_ERROR_MESSAGE: &str = "Internal server error";

#[derive(Debug)]
enum TimestampError {
    Format(time::error::Format),
}

impl fmt::Display for TimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimestampError::Format(err) => write!(f, "timestamp format error: {err}"),
        }
    }
}

pub enum QueueResponse {
    Success(QueueSuccessResponse),
    Error {
        status: StatusCode,
        body: QueueErrorResponse,
    },
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

pub enum HealthResponse {
    Success {
        status: StatusCode,
        body: HealthSuccessResponse,
    },
    Error {
        status: StatusCode,
        body: HealthErrorResponse,
    },
}

impl IntoResponse for HealthResponse {
    fn into_response(self) -> Response {
        match self {
            HealthResponse::Success { status, body } => (status, Json(body)).into_response(),
            HealthResponse::Error { status, body } => (status, Json(body)).into_response(),
        }
    }
}

pub async fn get_health(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    build_health_response(state, SystemTime::now())
}

pub enum SensorsResponse {
    Success(SensorsSuccessResponse),
    Error {
        status: StatusCode,
        body: SensorsErrorResponse,
    },
}

impl IntoResponse for SensorsResponse {
    fn into_response(self) -> Response {
        match self {
            SensorsResponse::Success(body) => (StatusCode::OK, Json(body)).into_response(),
            SensorsResponse::Error { status, body } => (status, Json(body)).into_response(),
        }
    }
}

pub async fn get_sensors(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    build_sensors_response(state, SystemTime::now())
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
        Err(_err) => internal_error("timestamp formatting failure"),
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
        Err(_err) => internal_error("timestamp formatting failure"),
    }
}

fn internal_error(message: &str) -> QueueResponse {
    error!(
        message = message,
        "Internal error while handling /api/queue"
    );
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

fn format_timestamp(timestamp: SystemTime) -> Result<String, TimestampError> {
    let datetime = OffsetDateTime::from(timestamp);
    datetime.format(&Rfc3339).map_err(TimestampError::Format)
}

fn build_health_response(state: Arc<RwLock<AppState>>, now: SystemTime) -> HealthResponse {
    let guard = match state.read() {
        Ok(guard) => guard,
        Err(_) => {
            return health_internal_error("state lock poisoned while reading sensors");
        }
    };

    let status = derive_health_status(guard.sensors());
    drop(guard);

    let timestamp = match format_timestamp(now) {
        Ok(formatted) => formatted,
        Err(_) => {
            return health_internal_error("timestamp formatting failure");
        }
    };

    let status_code = match status {
        HealthStatus::Ko => StatusCode::SERVICE_UNAVAILABLE,
        HealthStatus::Ok | HealthStatus::Degraded => StatusCode::OK,
    };

    HealthResponse::Success {
        status: status_code,
        body: HealthSuccessResponse { status, timestamp },
    }
}

fn derive_health_status(sensors: &[crate::sensor::SensorInfo]) -> HealthStatus {
    if sensors.is_empty() {
        return HealthStatus::Ko;
    }

    let mut has_ready = false;
    let mut has_error = false;

    for sensor in sensors {
        match sensor.status {
            DeviceSensorStatus::Ready => has_ready = true,
            DeviceSensorStatus::Error { .. } => has_error = true,
        }
    }

    match (has_ready, has_error) {
        (true, true) => HealthStatus::Degraded,
        (true, false) => HealthStatus::Ok,
        (false, _) => HealthStatus::Ko,
    }
}

fn health_internal_error(message: &str) -> HealthResponse {
    error!(
        message = message,
        "Internal error while handling /api/health"
    );
    let formatted = format_timestamp(SystemTime::now()).unwrap_or_else(|err| {
        error!(error = %err, "Failed to format health error timestamp");
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    });

    HealthResponse::Error {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        body: HealthErrorResponse {
            error_code: HealthErrorCode::InternalError,
            error_message: INTERNAL_ERROR_MESSAGE.to_string(),
            timestamp: formatted,
        },
    }
}

fn build_sensors_response(state: Arc<RwLock<AppState>>, now: SystemTime) -> SensorsResponse {
    let guard = match state.read() {
        Ok(guard) => guard,
        Err(_) => {
            return sensors_internal_error("state lock poisoned while reading sensors");
        }
    };

    let sensors = guard.sensors();
    if sensors.is_empty() {
        drop(guard);
        return sensors_unavailable_response(now);
    }

    let mut mapped_sensors = Vec::with_capacity(sensors.len());
    for sensor in sensors {
        match map_sensor_info(sensor) {
            Ok(mapped) => mapped_sensors.push(mapped),
            Err(message) => {
                drop(guard);
                return sensors_internal_error(message);
            }
        }
    }
    drop(guard);

    let timestamp = match format_timestamp(now) {
        Ok(formatted) => formatted,
        Err(_) => {
            return sensors_internal_error("timestamp formatting failure");
        }
    };

    SensorsResponse::Success(SensorsSuccessResponse {
        sensors: mapped_sensors,
        timestamp,
    })
}

fn map_sensor_info(
    sensor: &crate::sensor::SensorInfo,
) -> Result<SensorStatusResponse, &'static str> {
    if sensor.i2c_address > I2C_7BIT_MAX {
        return Err("invalid i2c address for sensor status response");
    }
    let (status, error_code) = match &sensor.status {
        DeviceSensorStatus::Ready => (SensorStatus::Ok, None),
        DeviceSensorStatus::Error { message } => {
            (SensorStatus::Error, Some(map_sensor_error_code(message)))
        }
    };

    Ok(SensorStatusResponse {
        sensor_id: format!("sensor-{}", sensor.sensor_id),
        i2c_address: format!("0x{:02x}", sensor.i2c_address),
        status,
        error_code,
    })
}

fn map_sensor_error_code(message: &str) -> SensorErrorCode {
    let message_lower = message.to_lowercase();
    if message_lower.contains("i2c") {
        SensorErrorCode::I2cError
    } else if message_lower.contains("timeout") {
        SensorErrorCode::Timeout
    } else if message_lower.contains("range") || message_lower.contains("invalid") {
        SensorErrorCode::InvalidReading
    } else {
        SensorErrorCode::NoResponse
    }
}

fn sensors_unavailable_response(now: SystemTime) -> SensorsResponse {
    match format_timestamp(now) {
        Ok(formatted) => SensorsResponse::Error {
            status: StatusCode::SERVICE_UNAVAILABLE,
            body: SensorsErrorResponse {
                error_code: SensorsErrorCode::SensorUnavailable,
                error_message: "Sensor list unavailable".to_string(),
                timestamp: formatted,
            },
        },
        Err(_) => sensors_internal_error("timestamp formatting failure"),
    }
}

fn sensors_internal_error(message: &str) -> SensorsResponse {
    error!(
        message = message,
        "Internal error while handling /api/sensors"
    );
    let formatted = format_timestamp(SystemTime::now()).unwrap_or_else(|err| {
        error!(error = %err, "Failed to format sensors error timestamp");
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
    });

    SensorsResponse::Error {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        body: SensorsErrorResponse {
            error_code: SensorsErrorCode::InternalError,
            error_message: INTERNAL_ERROR_MESSAGE.to_string(),
            timestamp: formatted,
        },
    }
}

// Debug Readings Handler

use crate::state::ReadingStatus;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DebugReadingsResponse {
    pub occupancy_threshold_mm: u16,
    pub sensors: Vec<DebugSensorReading>,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct DebugSensorReading {
    pub sensor_id: u32,
    pub distance_mm: u16,
    pub obstructed: Option<bool>,
    pub status: String,
}

pub async fn get_debug_readings(State(state): State<Arc<RwLock<AppState>>>) -> impl IntoResponse {
    build_debug_readings_response(state, SystemTime::now())
}

fn build_debug_readings_response(
    state: Arc<RwLock<AppState>>,
    now: SystemTime,
) -> impl IntoResponse {
    let guard = match state.read() {
        Ok(guard) => guard,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "state lock poisoned"
                })),
            );
        }
    };

    let model = state.read().map(|s| s.model().clone()).ok();
    // Default fallback if we can't get model (shouldn't happen)
    let threshold_mm = model
        .map(|m| m.occupancy_config().threshold_mm)
        .unwrap_or(1200);

    let readings = guard.readings();
    let mut sensors = Vec::with_capacity(readings.len());
    for reading in readings {
        let (status_str, is_valid) = match &reading.status {
            ReadingStatus::Ok { range_status } => (format!("ok ({:?})", range_status), true),
            ReadingStatus::Error { reason } => (format!("error: {}", reason), false),
        };

        let obstructed = if is_valid {
            Some(reading.distance_mm <= threshold_mm)
        } else {
            None
        };

        sensors.push(DebugSensorReading {
            sensor_id: reading.sensor_id,
            distance_mm: reading.distance_mm,
            obstructed,
            status: status_str,
        });
    }
    drop(guard);

    let timestamp = format_timestamp(now).unwrap_or_else(|_| "unknown".to_string());

    (
        StatusCode::OK,
        Json(serde_json::json!(DebugReadingsResponse {
            occupancy_threshold_mm: threshold_mm,
            sensors,
            timestamp,
        })),
    )
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
        app_state.set_wait_time(estimate).expect("set wait time");
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
        app_state.set_wait_time(estimate).expect("set wait time");
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

    fn sensor_info_with_address(
        sensor_id: u32,
        status: DeviceSensorStatus,
        i2c_address: u8,
    ) -> crate::sensor::SensorInfo {
        crate::sensor::SensorInfo {
            sensor_id,
            xshut_pin: 17,
            i2c_address,
            status,
        }
    }

    #[test]
    fn health_handler_returns_ok_when_all_ready() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_sensors();
        app_state
            .set_sensors(vec![
                sensor_info_with_address(1, DeviceSensorStatus::Ready, 0x30),
                sensor_info_with_address(2, DeviceSensorStatus::Ready, 0x31),
            ])
            .expect("set sensors");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_health_response(state, UNIX_EPOCH + Duration::from_secs(2));

        match response {
            HealthResponse::Success { status, body } => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body.status, HealthStatus::Ok);
                assert_eq!(body.timestamp, "1970-01-01T00:00:02Z");
            }
            HealthResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }

    #[test]
    fn health_handler_returns_degraded_when_mixed_status() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_sensors();
        app_state
            .set_sensors(vec![
                sensor_info_with_address(1, DeviceSensorStatus::Ready, 0x30),
                sensor_info_with_address(
                    2,
                    DeviceSensorStatus::Error {
                        message: "no response".to_string(),
                    },
                    0x31,
                ),
            ])
            .expect("set sensors");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_health_response(state, UNIX_EPOCH + Duration::from_secs(3));

        match response {
            HealthResponse::Success { status, body } => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body.status, HealthStatus::Degraded);
                assert_eq!(body.timestamp, "1970-01-01T00:00:03Z");
            }
            HealthResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }

    #[test]
    fn health_handler_returns_ko_when_none_ready() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_sensors();
        app_state
            .set_sensors(vec![sensor_info_with_address(
                1,
                DeviceSensorStatus::Error {
                    message: "failed".to_string(),
                },
                0x30,
            )])
            .expect("set sensors");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_health_response(state, UNIX_EPOCH + Duration::from_secs(4));

        match response {
            HealthResponse::Success { status, body } => {
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(body.status, HealthStatus::Ko);
                assert_eq!(body.timestamp, "1970-01-01T00:00:04Z");
            }
            HealthResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }

    #[test]
    fn health_handler_returns_internal_error_when_lock_poisoned() {
        let state = Arc::new(RwLock::new(AppState::new()));
        let state_for_thread = Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = state_for_thread.write().expect("lock for poison");
            panic!("poison lock");
        })
        .join();

        let response = build_health_response(state, UNIX_EPOCH + Duration::from_secs(5));

        match response {
            HealthResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body.error_code, HealthErrorCode::InternalError);
                assert_eq!(body.error_message, "Internal server error");
            }
            HealthResponse::Success { .. } => {
                panic!("expected internal error response");
            }
        }
    }

    #[test]
    fn health_handler_returns_ko_when_no_sensors() {
        let state = Arc::new(RwLock::new(AppState::new()));

        let response = build_health_response(state, UNIX_EPOCH + Duration::from_secs(6));

        match response {
            HealthResponse::Success { status, body } => {
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(body.status, HealthStatus::Ko);
                assert_eq!(body.timestamp, "1970-01-01T00:00:06Z");
            }
            HealthResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }

    #[test]
    fn sensors_handler_returns_success_when_sensors_available() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_sensors();
        app_state
            .set_sensors(vec![
                sensor_info_with_address(1, DeviceSensorStatus::Ready, 0x30),
                sensor_info_with_address(
                    2,
                    DeviceSensorStatus::Error {
                        message: "i2c failure".to_string(),
                    },
                    0x31,
                ),
            ])
            .expect("set sensors");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_sensors_response(state, UNIX_EPOCH + Duration::from_secs(7));

        match response {
            SensorsResponse::Success(body) => {
                assert_eq!(body.sensors.len(), 2);
                assert_eq!(body.sensors[0].sensor_id, "sensor-1");
                assert_eq!(body.sensors[0].i2c_address, "0x30");
                assert_eq!(body.sensors[0].status, SensorStatus::Ok);
                assert_eq!(body.sensors[0].error_code, None);
                assert_eq!(body.sensors[1].sensor_id, "sensor-2");
                assert_eq!(body.sensors[1].i2c_address, "0x31");
                assert_eq!(body.sensors[1].status, SensorStatus::Error);
                assert_eq!(body.sensors[1].error_code, Some(SensorErrorCode::I2cError));
                assert_eq!(body.timestamp, "1970-01-01T00:00:07Z");
            }
            SensorsResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }

    #[test]
    fn sensors_handler_returns_service_unavailable_when_empty() {
        let state = Arc::new(RwLock::new(AppState::new()));

        let response = build_sensors_response(state, UNIX_EPOCH + Duration::from_secs(8));

        match response {
            SensorsResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(body.error_code, SensorsErrorCode::SensorUnavailable);
            }
            SensorsResponse::Success(_) => {
                panic!("expected sensor unavailable response");
            }
        }
    }

    #[test]
    fn sensors_handler_returns_internal_error_when_lock_poisoned() {
        let state = Arc::new(RwLock::new(AppState::new()));
        let state_for_thread = Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = state_for_thread.write().expect("lock for poison");
            panic!("poison lock");
        })
        .join();

        let response = build_sensors_response(state, UNIX_EPOCH + Duration::from_secs(9));

        match response {
            SensorsResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body.error_code, SensorsErrorCode::InternalError);
                assert_eq!(body.error_message, "Internal server error");
            }
            SensorsResponse::Success(_) => {
                panic!("expected internal error response");
            }
        }
    }

    #[test]
    fn sensors_handler_validates_i2c_address() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_sensors();
        app_state
            .set_sensors(vec![sensor_info_with_address(
                1,
                DeviceSensorStatus::Ready,
                0x80,
            )])
            .expect("set sensors");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_sensors_response(state, UNIX_EPOCH + Duration::from_secs(10));

        match response {
            SensorsResponse::Error { status, body } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body.error_code, SensorsErrorCode::InternalError);
            }
            SensorsResponse::Success(_) => {
                panic!("expected internal error response");
            }
        }
    }

    #[test]
    fn sensors_handler_maps_error_codes_by_message() {
        let mut app_state = AppState::new();
        let _receiver = app_state.subscribe_sensors();
        app_state
            .set_sensors(vec![
                sensor_info_with_address(
                    1,
                    DeviceSensorStatus::Error {
                        message: "timeout while reading".to_string(),
                    },
                    0x30,
                ),
                sensor_info_with_address(
                    2,
                    DeviceSensorStatus::Error {
                        message: "range out of bounds".to_string(),
                    },
                    0x31,
                ),
                sensor_info_with_address(
                    3,
                    DeviceSensorStatus::Error {
                        message: "invalid reading".to_string(),
                    },
                    0x32,
                ),
                sensor_info_with_address(
                    4,
                    DeviceSensorStatus::Error {
                        message: "no response".to_string(),
                    },
                    0x33,
                ),
            ])
            .expect("set sensors");
        let state = Arc::new(RwLock::new(app_state));

        let response = build_sensors_response(state, UNIX_EPOCH + Duration::from_secs(11));

        match response {
            SensorsResponse::Success(body) => {
                assert_eq!(body.sensors[0].error_code, Some(SensorErrorCode::Timeout));
                assert_eq!(
                    body.sensors[1].error_code,
                    Some(SensorErrorCode::InvalidReading)
                );
                assert_eq!(
                    body.sensors[2].error_code,
                    Some(SensorErrorCode::InvalidReading)
                );
                assert_eq!(
                    body.sensors[3].error_code,
                    Some(SensorErrorCode::NoResponse)
                );
            }
            SensorsResponse::Error { status, .. } => {
                panic!("expected success response, got error: {status}");
            }
        }
    }
}
