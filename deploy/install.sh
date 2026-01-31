#!/usr/bin/env bash
set -euo pipefail

PREFIX=${MARIAM_FLOW_PREFIX:-/opt/mariam-flow}
APP_USER=${MARIAM_FLOW_USER:-pi}
APP_GROUP=${MARIAM_FLOW_GROUP:-pi}

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT_DIR=$(cd "${SCRIPT_DIR}/.." && pwd)

sudo mkdir -p "$PREFIX"

sudo cp -a "$ROOT_DIR/mariam-flow" "$PREFIX/"
sudo cp -a "$ROOT_DIR/model_service" "$PREFIX/"
sudo cp -a "$ROOT_DIR/deploy" "$PREFIX/"
sudo chmod +x "$PREFIX/mariam-flow"

if [ ! -d "$PREFIX/config" ]; then
  sudo cp -a "$ROOT_DIR/config" "$PREFIX/"
  echo "Config installed to $PREFIX/config"
else
  echo "Config already exists in $PREFIX/config (kept as-is)."
fi

sudo chown -R "$APP_USER:$APP_GROUP" "$PREFIX"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required. Install it before running this script."
  exit 1
fi

python3 -m venv "$PREFIX/model_service/venv"
"$PREFIX/model_service/venv/bin/pip" install --upgrade pip

if [ -d "$PREFIX/model_service/wheelhouse" ]; then
  "$PREFIX/model_service/venv/bin/pip" install \
    --no-index \
    --find-links "$PREFIX/model_service/wheelhouse" \
    -r "$PREFIX/model_service/requirements.txt"
else
  "$PREFIX/model_service/venv/bin/pip" install -r "$PREFIX/model_service/requirements.txt"
fi

sudo cp "$PREFIX/deploy/systemd/"*.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable mariam-model.service mariam-flow.service
sudo systemctl restart mariam-model.service mariam-flow.service

echo "Installation complete. Use: systemctl status mariam-flow"
