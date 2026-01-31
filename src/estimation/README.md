# Estimation Models

This folder contains the estimation pipeline and calibration schema. The actual model
implementations now live in the Python model service (`model_service/models/`).

## Common calibration fields (required)

All models share these top-level fields in `config/calibration.json`:

- `model`: string identifier for the model.
- `occupancy_threshold_mm`: distance threshold used to compute whether a sensor is obstructed.
- `sensor_min_mm` / `sensor_max_mm`: valid distance range for readings.
- `params`: model-specific parameters.

Example skeleton:

```json
{
  "model": "<model_name>",
  "occupancy_threshold_mm": 1200,
  "sensor_min_mm": 10,
  "sensor_max_mm": 4000,
  "params": { }
}
```

## Python models

### linear_v1

- Behavior: converts obstructions to occupancy percent and applies a slope/intercept line.
- Formula: `wait_time = intercept + slope * occupancy_percent`.

Parameters:
- `slope` (f64)
- `intercept` (f64)
- `min_wait_minutes` (optional u32)
- `max_wait_minutes` (optional u32)

Example `config/calibration.json`:

```json
{
  "model": "linear_v1",
  "occupancy_threshold_mm": 1200,
  "sensor_min_mm": 10,
  "sensor_max_mm": 4000,
  "params": {
    "slope": 1.5,
    "intercept": 0.0,
    "min_wait_minutes": 0,
    "max_wait_minutes": 30
  }
}
```

### linear_v2

- Behavior: converts obstructions to occupancy percent, then linearly interpolates between
  `wait_time_at_empty` and `wait_time_at_full`.

Parameters:
- `wait_time_at_empty` (f64)
- `wait_time_at_full` (f64)

Example `config/calibration.json`:

```json
{
  "model": "linear_v2",
  "occupancy_threshold_mm": 1200,
  "sensor_min_mm": 10,
  "sensor_max_mm": 4000,
  "params": {
    "wait_time_at_empty": 2.0,
    "wait_time_at_full": 18.0
  }
}
```

### obstruction_count_v1

- Behavior: uses the raw obstruction count directly (no occupancy percent).
- Formula: `wait_time = base_minutes + per_obstruction_minutes * obstructed_count`.

Parameters:
- `base_minutes` (f64)
- `per_obstruction_minutes` (f64)
- `min_wait_minutes` (optional u32)
- `max_wait_minutes` (optional u32)

Example `config/calibration.json`:

```json
{
  "model": "obstruction_count_v1",
  "occupancy_threshold_mm": 1200,
  "sensor_min_mm": 10,
  "sensor_max_mm": 4000,
  "params": {
    "base_minutes": 2.0,
    "per_obstruction_minutes": 3.0,
    "min_wait_minutes": 0,
    "max_wait_minutes": 30
  }
}
```

## Service Python (modèles)

Mariam Flow délègue le calcul du modèle à un service local Python.
`calibration.json` reste la source de vérité pour `model` et `params`,
mais le calcul est effectué par le service Python.

Configuration dans `config/config.toml` :

```toml
[model]
remote_url = "http://127.0.0.1:5001/predict"
timeout_ms = 800
```

Le service Python reçoit les obstructions et renvoie `wait_time_minutes`.

Les modèles Python sont dans `model_service/models/` et utilisent le même `model` dans
`config/calibration.json`.
