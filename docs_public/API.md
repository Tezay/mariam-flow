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

### Notes

- `queue_length` is optional and may be omitted if not available.
- If the estimate is not available, the endpoint returns an error response.

## GET /api/health

Returns global health status for the device.

### Success Response (200)

```json
{
  "status": "ok",
  "timestamp": "2026-01-11T12:30:00Z"
}
```

### Status Rules

- `ok`: all sensors healthy
- `degraded`: some sensors in error, system still functioning
- `ko`: system not functioning

### Error Response

- HTTP 503 for `ko` with an error response payload.

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

### Notes

- `sensor_id` must be stable across boots.
- `i2c_address` uses 7-bit address format in hex.
