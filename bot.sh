#!/bin/bash
# Mascord Bot Runner with proper setup

set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_DIR"

BUILD_MODE="${1:-release}"

# Validate build mode
if [[ "$BUILD_MODE" != "debug" && "$BUILD_MODE" != "release" ]]; then
    echo "Usage: $0 [debug|release]"
    exit 1
fi

echo "🤖 Mascord Bot Startup"
echo "===================="
echo ""

# Create data directory
echo "📁 Ensuring data directory exists..."
mkdir -p "$PROJECT_DIR/data"
chmod 755 "$PROJECT_DIR/data"

# Verify .env
if [[ ! -f "$PROJECT_DIR/.env" ]]; then
    echo "❌ .env file not found!"
    echo "Please create .env with your Discord credentials."
    exit 1
fi

# Check critical environment variables
if ! grep -q "DISCORD_TOKEN" "$PROJECT_DIR/.env"; then
    echo "❌ DISCORD_TOKEN not set in .env"
    exit 1
fi

if ! grep -q "APPLICATION_ID" "$PROJECT_DIR/.env"; then
    echo "❌ APPLICATION_ID not set in .env"
    exit 1
fi

echo "✓ Configuration verified"
echo ""

# Check if binary exists, build if needed
BINARY="$PROJECT_DIR/target/$BUILD_MODE/mascord"
if [[ ! -f "$BINARY" ]]; then
    echo "📦 Building $BUILD_MODE binary..."
    cargo build --$BUILD_MODE
    echo ""
fi

echo "🚀 Starting Mascord (using $BUILD_MODE binary)..."
echo ""
echo "═══════════════════════════════════════════════════"
echo ""

# Run the bot
exec "$BINARY"
