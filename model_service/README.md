# Mariam Flow Model Service

Service Python local pour l'estimation du temps d'attente. Il reçoit les obstructions détectées
par le service Mariam Flow et renvoie une estimation en HTTP/JSON.

## Endpoints

- `POST /predict`
- `GET /health`

## Modèles disponibles

Les modèles sont dans `model_service/models/`.

| Model ID | Fichier | Description |
| --- | --- | --- |
| `linear_v1` | `models/linear_v1.py` | Linéaire slope/intercept |
| `linear_v2` | `models/linear_v2.py` | Linéaire min/max |
| `obstruction_count_v1` | `models/obstruction_count_v1.py` | Compte d'obstructions |

Pour activer un modèle, changez `model` dans `config/calibration.json`.

## Ajouter un nouveau modèle

1. Créez un fichier dans `model_service/models/<model_id>.py` avec une fonction `predict(...)`.
2. Ajoutez l'import + le mapping dans `model_service/models/__init__.py`.
3. Enregistrez le `model_id` dans l'import des modèles (`from models import ...` dans `model_service/app.py`).
3. Enregistrez le `model_id` dans `MODEL_HANDLERS` (`model_service/app.py`).
4. Documentez le modèle ici avec un exemple de `calibration.json`.

## Formules

Tous les modèles actuels utilisent la même logique d'occupation :  
`occupancy_percent = (occupied_count / valid_count) * 100`  
Si `valid_count == 0` → `status = degraded` et `error_code = NO_DATA`.

### linear_v1

Formule :  
`wait_time = intercept + slope * occupancy_percent`  
Paramètres :
- `slope` (float)
- `intercept` (float)
- `min_wait_minutes` (optionnel)
- `max_wait_minutes` (optionnel)

### linear_v2

Formule :  
`wait_time = wait_time_at_empty + (occupancy_percent / 100) * (wait_time_at_full - wait_time_at_empty)`  
Paramètres :
- `wait_time_at_empty` (float)
- `wait_time_at_full` (float)

### obstruction_count_v1

Formule :  
`wait_time = base_minutes + per_obstruction_minutes * obstructed_count`  
Paramètres :
- `base_minutes` (float)
- `per_obstruction_minutes` (float)
- `min_wait_minutes` (optionnel)
- `max_wait_minutes` (optionnel)

## Démarrage local

```bash
cd model_service
python3 -m venv .venv   # Powershell : python -m venv .venv
. .venv/bin/activate    # Powershell : .\\.venv\\Scripts\\Activate.ps1
pip install -r requirements.txt
python app.py --host 127.0.0.1 --port 5001
```

## Exemple de requête

```bash
curl -X POST http://127.0.0.1:5001/predict \
  -H "Content-Type: application/json" \
  -d '{
    "api_version": "1.0",
    "model_id": "linear_v1",
    "params": {"slope": 0.6, "intercept": 0.0},
    "timestamp": "2026-01-29T12:00:00Z",
    "obstructions": [
      {"sensor_id": 1, "obstructed": true, "timestamp": "2026-01-29T12:00:00Z"},
      {"sensor_id": 2, "obstructed": false, "timestamp": "2026-01-29T12:00:00Z"}
    ]
  }'
```

## Réponse attendue

```json
{
  "wait_time_minutes": 30.0,
  "status": "ok",
  "error_code": null,
  "timestamp": "2026-01-29T12:00:00Z"
}
```

## Variables d'environnement

- `MARIAM_MODEL_HOST` (défaut: `127.0.0.1`)
- `MARIAM_MODEL_PORT` (défaut: `5001`)
- `MARIAM_MODEL_LOG_LEVEL` (défaut: `info`)
