#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_DIR"

echo "Building release binary..."
cargo build --release

if command -v systemctl >/dev/null 2>&1; then
  if systemctl is-enabled --quiet mascord 2>/dev/null || systemctl is-active --quiet mascord 2>/dev/null; then
    echo "Restarting systemd service: mascord"
    sudo systemctl restart mascord
    sudo systemctl --no-pager --full status mascord || true
  else
    echo "systemd service 'mascord' not installed/enabled. Build completed."
  fi
else
  echo "systemctl not found. Build completed."
fi
