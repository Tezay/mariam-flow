use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueSuccessResponse {
    pub wait_time_minutes: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queue_length: Option<u32>,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueErrorResponse {
    pub error_code: QueueErrorCode,
    pub error_message: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Ok,
    Degraded,
    Ko,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HealthSuccessResponse {
    pub status: HealthStatus,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HealthErrorResponse {
    pub error_code: HealthErrorCode,
    pub error_message: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SensorStatus {
    Ok,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SensorsSuccessResponse {
    pub sensors: Vec<SensorStatusResponse>,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SensorStatusResponse {
    pub sensor_id: String,
    pub i2c_address: String,
    pub status: SensorStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<SensorErrorCode>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SensorsErrorResponse {
    pub error_code: SensorsErrorCode,
    pub error_message: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthErrorCode {
    InternalError,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SensorErrorCode {
    NoResponse,
    I2cError,
    Timeout,
    InvalidReading,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SensorsErrorCode {
    SensorUnavailable,
    InternalError,
}

#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QueueErrorCode {
    NoData,
    InternalError,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn success_response_omits_queue_length_when_none() {
        let response = QueueSuccessResponse {
            wait_time_minutes: 7.0,
            queue_length: None,
            timestamp: "2026-01-11T12:30:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize success response");
        assert_eq!(
            value,
            json!({
                "wait_time_minutes": 7.0,
                "timestamp": "2026-01-11T12:30:00Z"
            })
        );
    }

    #[test]
    fn success_response_includes_queue_length_when_present() {
        let response = QueueSuccessResponse {
            wait_time_minutes: 8.5,
            queue_length: Some(12),
            timestamp: "2026-01-11T12:31:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize success response");
        assert_eq!(
            value,
            json!({
                "wait_time_minutes": 8.5,
                "queue_length": 12,
                "timestamp": "2026-01-11T12:31:00Z"
            })
        );
    }

    #[test]
    fn error_response_uses_screaming_snake_case_code() {
        let response = QueueErrorResponse {
            error_code: QueueErrorCode::NoData,
            error_message: "no estimate available".to_string(),
            timestamp: "2026-01-11T12:32:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize error response");
        assert_eq!(
            value,
            json!({
                "error_code": "NO_DATA",
                "error_message": "no estimate available",
                "timestamp": "2026-01-11T12:32:00Z"
            })
        );
    }

    #[test]
    fn health_success_response_serializes_status() {
        let response = HealthSuccessResponse {
            status: HealthStatus::Ok,
            timestamp: "2026-01-11T12:33:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize health success response");
        assert_eq!(
            value,
            json!({
                "status": "ok",
                "timestamp": "2026-01-11T12:33:00Z"
            })
        );
    }

    #[test]
    fn health_error_response_uses_screaming_snake_case_code() {
        let response = HealthErrorResponse {
            error_code: HealthErrorCode::InternalError,
            error_message: "boom".to_string(),
            timestamp: "2026-01-11T12:34:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize health error response");
        assert_eq!(
            value,
            json!({
                "error_code": "INTERNAL_ERROR",
                "error_message": "boom",
                "timestamp": "2026-01-11T12:34:00Z"
            })
        );
    }

    #[test]
    fn sensors_success_response_serializes_with_optional_error_code() {
        let response = SensorsSuccessResponse {
            sensors: vec![
                SensorStatusResponse {
                    sensor_id: "sensor-1".to_string(),
                    i2c_address: "0x30".to_string(),
                    status: SensorStatus::Ok,
                    error_code: None,
                },
                SensorStatusResponse {
                    sensor_id: "sensor-2".to_string(),
                    i2c_address: "0x31".to_string(),
                    status: SensorStatus::Error,
                    error_code: Some(SensorErrorCode::NoResponse),
                },
            ],
            timestamp: "2026-01-11T12:30:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize sensors success response");
        assert_eq!(
            value,
            json!({
                "sensors": [
                    {
                        "sensor_id": "sensor-1",
                        "i2c_address": "0x30",
                        "status": "ok"
                    },
                    {
                        "sensor_id": "sensor-2",
                        "i2c_address": "0x31",
                        "status": "error",
                        "error_code": "NO_RESPONSE"
                    }
                ],
                "timestamp": "2026-01-11T12:30:00Z"
            })
        );
    }

    #[test]
    fn sensors_error_response_uses_screaming_snake_case_code() {
        let response = SensorsErrorResponse {
            error_code: SensorsErrorCode::SensorUnavailable,
            error_message: "no sensors".to_string(),
            timestamp: "2026-01-11T12:35:00Z".to_string(),
        };

        let value = serde_json::to_value(response).expect("serialize sensors error response");
        assert_eq!(
            value,
            json!({
                "error_code": "SENSOR_UNAVAILABLE",
                "error_message": "no sensors",
                "timestamp": "2026-01-11T12:35:00Z"
            })
        );
    }
}
