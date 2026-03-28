#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_DIR"

if [[ ! -f ".env" ]]; then
  echo "Missing .env file"
  exit 1
fi

# Keep artifacts under the project unless the caller overrides (helps some CI/sandbox setups).
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${PROJECT_DIR}/target}"

usage() {
  cat <<'EOF'
Usage:
  register-commands.sh              Register slash commands globally (ignores DEV_GUILD_ID in .env for this run).
  register-commands.sh --global     Same as no arguments.
  register-commands.sh <guild_id>   Register slash commands only to that guild (fast iteration).

Environment:
  REGISTER_COMMANDS is forced to true by this script.

After registration succeeds, stop the process (Ctrl+C) if it keeps running, and keep REGISTER_COMMANDS=false
for normal operation. See docs/setup.md for verification and caveats.
EOF
}

MODE="global"
GUILD_ID=""

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--global" || "${1:-}" == "-g" ]]; then
  MODE="global"
  shift || true
  if [[ -n "${1:-}" ]]; then
    echo "Unexpected extra argument after --global: $1" >&2
    usage >&2
    exit 1
  fi
elif [[ -n "${1:-}" ]]; then
  MODE="guild"
  GUILD_ID="$1"
fi

if [[ "${MODE}" == "guild" ]]; then
  echo "Registering commands to guild ${GUILD_ID}..."
  REGISTER_COMMANDS=true DEV_GUILD_ID="${GUILD_ID}" cargo run --release
else
  echo "Registering commands globally (Discord may take up to ~1h to show updates in all clients; API updates quickly)..."
  echo "Tip: set HEALTH_PORT=0 for this run if you do not want the health server to bind."
  # Empty DEV_GUILD_ID must be set in the environment so dotenv does not repopulate it from .env
  # (dotenvy does not override existing variables).
  REGISTER_COMMANDS=true DEV_GUILD_ID= cargo run --release
fi
