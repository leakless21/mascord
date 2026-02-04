#!/bin/bash
# Mascord Bot Runner with Auto-Restart Loop
# Use this in zellij for persistent bot sessions that survive /restart commands

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_DIR"

BUILD_MODE="${1:-release}"

# Validate build mode
if [[ "$BUILD_MODE" != "debug" && "$BUILD_MODE" != "release" ]]; then
    echo "Usage: $0 [debug|release]"
    exit 1
fi

BINARY="$PROJECT_DIR/target/$BUILD_MODE/mascord"

# Check if binary exists
if [[ ! -f "$BINARY" ]]; then
    echo "❌ Binary not found: $BINARY"
    echo "Build with: cargo build --$BUILD_MODE"
    exit 1
fi

echo "🤖 Mascord Bot Persistent Runner (Auto-Restart)"
echo "==============================================="
echo ""
echo "Bot will automatically restart after:"
echo "  • /restart command in Discord"
echo "  • Unexpected crashes"
echo ""
echo "To stop: Press Ctrl+C"
echo ""

RESTART_COUNT=0
while true; do
    RESTART_COUNT=$((RESTART_COUNT + 1))
    
    if [ $RESTART_COUNT -eq 1 ]; then
        echo "🚀 Starting bot..."
    else
        echo ""
        echo "🔄 Restarting bot... (restart #$RESTART_COUNT)"
    fi
    
    # Run bot
    "$BINARY"
    EXIT_CODE=$?
    
    if [ $EXIT_CODE -eq 0 ]; then
        echo "✓ Bot exited cleanly"
    else
        echo "⚠️  Bot exited with code $EXIT_CODE"
    fi
    
    echo "Restarting in 2 seconds... (Press Ctrl+C to stop)"
    sleep 2
done
