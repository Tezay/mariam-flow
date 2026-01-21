# Mariam Flow API

Version: 1.0 (Epic 2 target)

## Conventions

- Base path: `/api`
- Content-Type: `application/json`
- Timestamp format: ISO-8601 UTC, e.g. `2026-01-11T12:30:00Z`
- JSON fields use `snake_case`

## Error Response (All Endpoints)

```json
{
  "error_code": "ERROR_CODE",
  "error_message": "Description",
  "timestamp": "2026-01-11T12:30:00Z"
}
```

### Error Codes

- `NO_DATA`: estimation unavailable
- `SENSOR_UNAVAILABLE`: sensor data not available or sensor list missing
- `INTERNAL_ERROR`: unexpected server error
- `CONFIG_ERROR`: configuration invalid or missing required fields

## GET /api/queue

Returns the current wait-time estimate and queue data.

### Success Response (200)

```json
{
  "wait_time_minutes": 7,
  "queue_length": 12,
  "timestamp": "2026-01-11T12:30:00Z"
}
```

### Required vs Optional Fields

- Required: `wait_time_minutes`, `timestamp`
- Optional: `queue_length` (omitted if not available)

### Notes

- If the estimate is not available, the endpoint returns an error response.

### Error Responses

- HTTP 503 with `error_code=NO_DATA` when the estimate is unavailable.
- HTTP 500 with `error_code=INTERNAL_ERROR` on unexpected errors.

## GET /api/health

Returns global health status for the device.

### Success Response (200)

```json
{
  "status": "ok",
  "timestamp": "2026-01-11T12:30:00Z"
}
```

### Required vs Optional Fields

- Required: `status`, `timestamp`
- Optional: none

### Status Rules

- `ok`: all sensors healthy
- `degraded`: some sensors in error, system still functioning
- `ko`: system not functioning

### Non-OK Responses

- HTTP 503 with the same payload shape when `status=ko`.
- HTTP 500 with `error_code=INTERNAL_ERROR` on unexpected errors.

## GET /api/sensors

Returns the status of each sensor.

### Success Response (200)

```json
{
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
}
```

### Required vs Optional Fields

- Required: `sensors`, `timestamp`
- Optional: per-sensor `error_code` (present only when status is `error`)

### Notes

- `sensor_id` must be stable across boots.
- `i2c_address` uses 7-bit address format in hex.

### Per-Sensor Error Codes

- `NO_RESPONSE`: sensor did not respond on I2C
- `I2C_ERROR`: bus-level I2C error during read/write
- `TIMEOUT`: sensor did not return a range within expected time
- `INVALID_READING`: reading out of range or invalid

### Error Responses

- HTTP 503 with `error_code=SENSOR_UNAVAILABLE` when sensor list is unavailable.
- HTTP 500 with `error_code=INTERNAL_ERROR` on unexpected errors.
