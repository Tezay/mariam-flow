use crate::estimation::model::{EstimationModel, OccupancyConfig};
use crate::state::{SensorObstruction, WaitTimeErrorCode, WaitTimeEstimate, WaitTimeStatus};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, SystemTime};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::warn;

const API_VERSION: &str = "1.0";

pub struct RemoteModel {
    endpoint: String,
    timeout: Duration,
    model_id: String,
    params: serde_json::Value,
    occupancy_config: OccupancyConfig,
    fallback_model: Option<Box<dyn EstimationModel>>,
}

impl RemoteModel {
    pub fn new(
        endpoint: String,
        timeout: Duration,
        model_id: String,
        params: serde_json::Value,
        occupancy_config: OccupancyConfig,
        fallback_model: Option<Box<dyn EstimationModel>>,
    ) -> Self {
        Self {
            endpoint,
            timeout,
            model_id,
            params,
            occupancy_config,
            fallback_model,
        }
    }

    fn call_remote(
        &self,
        obstructions: &[SensorObstruction],
        timestamp: SystemTime,
    ) -> Result<PredictResponse, RemoteError> {
        let request = PredictRequest::new(
            &self.model_id,
            &self.params,
            obstructions,
            timestamp,
        )?;
        let payload = serde_json::to_string(&request).map_err(RemoteError::Json)?;
        let response_body = send_http_json(&self.endpoint, &payload, self.timeout)?;
        let response: PredictResponse = serde_json::from_str(&response_body).map_err(RemoteError::Json)?;
        Ok(response)
    }
}

impl fmt::Debug for RemoteModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteModel")
            .field("endpoint", &self.endpoint)
            .field("timeout", &self.timeout)
            .field("model_id", &self.model_id)
            .field("has_fallback", &self.fallback_model.is_some())
            .finish()
    }
}

impl EstimationModel for RemoteModel {
    fn compute_wait_time(
        &self,
        obstructions: &[SensorObstruction],
        timestamp: SystemTime,
    ) -> WaitTimeEstimate {
        match self.call_remote(obstructions, timestamp) {
            Ok(response) => WaitTimeEstimate {
                wait_time_minutes: response.wait_time_minutes,
                timestamp,
                status: response.status.into(),
                error_code: response.error_code,
            },
            Err(err) => {
                warn!(error = %err, "Remote model call failed");
                if let Some(fallback) = self.fallback_model.as_ref() {
                    warn!("Falling back to local model");
                    return fallback.compute_wait_time(obstructions, timestamp);
                }
                WaitTimeEstimate {
                    wait_time_minutes: None,
                    timestamp,
                    status: WaitTimeStatus::Degraded,
                    error_code: Some(WaitTimeErrorCode::NoData),
                }
            }
        }
    }

    fn occupancy_config(&self) -> &OccupancyConfig {
        &self.occupancy_config
    }
}

#[derive(Debug, Serialize)]
struct PredictRequest<'a> {
    api_version: &'static str,
    model_id: &'a str,
    params: &'a serde_json::Value,
    timestamp: String,
    obstructions: Vec<RemoteObstruction>,
}

impl<'a> PredictRequest<'a> {
    fn new(
        model_id: &'a str,
        params: &'a serde_json::Value,
        obstructions: &[SensorObstruction],
        timestamp: SystemTime,
    ) -> Result<Self, RemoteError> {
        let formatted = format_timestamp(timestamp)?;
        let mut payload = Vec::with_capacity(obstructions.len());
        for obstruction in obstructions {
            let obstruction_timestamp = format_timestamp(obstruction.timestamp)?;
            payload.push(RemoteObstruction {
                sensor_id: obstruction.sensor_id,
                obstructed: obstruction.obstructed,
                timestamp: obstruction_timestamp,
            });
        }

        Ok(Self {
            api_version: API_VERSION,
            model_id,
            params,
            timestamp: formatted,
            obstructions: payload,
        })
    }
}

#[derive(Debug, Serialize)]
struct RemoteObstruction {
    sensor_id: u32,
    obstructed: Option<bool>,
    timestamp: String,
}

#[derive(Debug, Deserialize)]
struct PredictResponse {
    wait_time_minutes: Option<f64>,
    status: RemoteStatus,
    #[serde(default)]
    error_code: Option<WaitTimeErrorCode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RemoteStatus {
    Ok,
    Degraded,
}

impl From<RemoteStatus> for WaitTimeStatus {
    fn from(status: RemoteStatus) -> Self {
        match status {
            RemoteStatus::Ok => WaitTimeStatus::Ok,
            RemoteStatus::Degraded => WaitTimeStatus::Degraded,
        }
    }
}

#[derive(Debug)]
enum RemoteError {
    InvalidUrl(String),
    Dns(String),
    Connect(std::io::Error),
    Io(std::io::Error),
    Http(u16, String),
    Json(serde_json::Error),
    Timestamp(time::error::Format),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemoteError::InvalidUrl(msg) => write!(f, "invalid url: {msg}"),
            RemoteError::Dns(msg) => write!(f, "dns error: {msg}"),
            RemoteError::Connect(err) => write!(f, "connect error: {err}"),
            RemoteError::Io(err) => write!(f, "io error: {err}"),
            RemoteError::Http(code, body) => {
                write!(f, "http status {code} ({})", body.trim())
            }
            RemoteError::Json(err) => write!(f, "json error: {err}"),
            RemoteError::Timestamp(err) => write!(f, "timestamp error: {err}"),
        }
    }
}

fn format_timestamp(timestamp: SystemTime) -> Result<String, RemoteError> {
    let datetime = OffsetDateTime::from(timestamp);
    datetime
        .format(&Rfc3339)
        .map_err(RemoteError::Timestamp)
}

struct ParsedUrl {
    host: String,
    port: u16,
    path: String,
}

fn parse_http_url(endpoint: &str) -> Result<ParsedUrl, RemoteError> {
    let trimmed = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| RemoteError::InvalidUrl("only http:// supported".to_string()))?;

    let mut parts = trimmed.splitn(2, '/');
    let host_port = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RemoteError::InvalidUrl("missing host".to_string()))?;
    let path = match parts.next() {
        Some(path) if !path.is_empty() => format!("/{path}"),
        _ => "/".to_string(),
    };

    let mut host_parts = host_port.splitn(2, ':');
    let host = host_parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RemoteError::InvalidUrl("missing host".to_string()))?;
    let port = match host_parts.next() {
        Some(port_str) if !port_str.is_empty() => port_str
            .parse::<u16>()
            .map_err(|_| RemoteError::InvalidUrl("invalid port".to_string()))?,
        _ => 80,
    };

    Ok(ParsedUrl {
        host: host.to_string(),
        port,
        path,
    })
}

fn send_http_json(endpoint: &str, body: &str, timeout: Duration) -> Result<String, RemoteError> {
    let parsed = parse_http_url(endpoint)?;
    let addr = (parsed.host.as_str(), parsed.port)
        .to_socket_addrs()
        .map_err(|err| RemoteError::Dns(err.to_string()))?
        .next()
        .ok_or_else(|| RemoteError::Dns("no addresses resolved".to_string()))?;

    let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(RemoteError::Connect)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(RemoteError::Io)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(RemoteError::Io)?;

    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        parsed.path,
        parsed.host,
        body.as_bytes().len(),
        body
    );

    stream
        .write_all(request.as_bytes())
        .map_err(RemoteError::Io)?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(RemoteError::Io)?;

    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| RemoteError::Http(0, "invalid http response".to_string()))?;

    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| RemoteError::Http(0, "missing status line".to_string()))?;
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| RemoteError::Http(0, "missing status code".to_string()))?
        .parse::<u16>()
        .map_err(|_| RemoteError::Http(0, "invalid status code".to_string()))?;

    if status_code >= 400 {
        return Err(RemoteError::Http(status_code, body.to_string()));
    }

    Ok(body.to_string())
}
