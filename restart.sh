#!/bin/bash
# Mascord Bot Restart Script
# Stops the running bot and starts a new instance

set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_DIR"

BUILD_MODE="${1:-release}"

# Validate build mode
if [[ "$BUILD_MODE" != "debug" && "$BUILD_MODE" != "release" ]]; then
    echo "Usage: $0 [debug|release]"
    exit 1
fi

echo "🔄 Restarting Mascord Bot..."
echo ""

# Find and kill existing bot process
echo "⏹️  Stopping existing bot process..."
if pgrep -f "target/$BUILD_MODE/mascord" > /dev/null; then
    pkill -f "target/$BUILD_MODE/mascord" || true
    sleep 2
    # Force kill if still running
    if pgrep -f "target/$BUILD_MODE/mascord" > /dev/null; then
        echo "⚠️  Process still running, force killing..."
        pkill -9 -f "target/$BUILD_MODE/mascord" || true
        sleep 1
    fi
    echo "✓ Bot stopped"
else
    echo "ℹ️  No running bot process found"
fi

echo ""
echo "🚀 Starting bot..."
echo ""

# Start the bot using bot.sh
exec "$PROJECT_DIR/bot.sh" "$BUILD_MODE"
