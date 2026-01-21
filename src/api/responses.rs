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
}
