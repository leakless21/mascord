#!/bin/bash
# Mascord Bot Runner with Auto-Restart Loop
# Use this in zellij for persistent bot sessions that survive /restart commands
#
# Optional environment (defaults preserve previous behavior):
#   MASCORD_PERSISTENT_CLEAN_DELAY_SECS   — sleep after a clean exit (default: 2)
#   MASCORD_PERSISTENT_CRASH_BACKOFF_START — first sleep after non-zero exit (default: 2)
#   MASCORD_PERSISTENT_CRASH_BACKOFF_MAX   — cap for exponential backoff (default: 120)
#   MASCORD_PERSISTENT_MAX_CONSECUTIVE_CRASHES — stop after N consecutive non-zero exits (0 = unlimited)

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

CLEAN_DELAY="${MASCORD_PERSISTENT_CLEAN_DELAY_SECS:-2}"
BACKOFF_START="${MASCORD_PERSISTENT_CRASH_BACKOFF_START:-2}"
BACKOFF_MAX="${MASCORD_PERSISTENT_CRASH_BACKOFF_MAX:-120}"
MAX_CONSEC_CRASHES="${MASCORD_PERSISTENT_MAX_CONSECUTIVE_CRASHES:-0}"

echo "🤖 Mascord Bot Persistent Runner (Auto-Restart)"
echo "==============================================="
echo ""
echo "Bot will automatically restart after:"
echo "  • /restart command in Discord"
echo "  • Unexpected crashes"
echo ""
echo "To stop: Press Ctrl+C"
echo ""
if [[ "$MAX_CONSEC_CRASHES" != "0" ]]; then
    echo "Crash limit: $MAX_CONSEC_CRASHES consecutive non-zero exits (then script stops)."
fi
echo ""

RESTART_COUNT=0
CONSECUTIVE_CRASHES=0
CRASH_BACKOFF="$BACKOFF_START"

while true; do
    RESTART_COUNT=$((RESTART_COUNT + 1))

    if [ "$RESTART_COUNT" -eq 1 ]; then
        echo "🚀 Starting bot..."
    else
        echo ""
        echo "🔄 Restarting bot... (run #$RESTART_COUNT)"
    fi

    "$BINARY"
    EXIT_CODE=$?

    if [ "$EXIT_CODE" -eq 0 ]; then
        echo "✓ Bot exited cleanly"
        CONSECUTIVE_CRASHES=0
        CRASH_BACKOFF="$BACKOFF_START"
        echo "Restarting in ${CLEAN_DELAY}s... (Press Ctrl+C to stop)"
        sleep "$CLEAN_DELAY"
    else
        echo "⚠️  Bot exited with code $EXIT_CODE"
        CONSECUTIVE_CRASHES=$((CONSECUTIVE_CRASHES + 1))
        if [[ "$MAX_CONSEC_CRASHES" =~ ^[0-9]+$ ]] && [ "$MAX_CONSEC_CRASHES" -gt 0 ] \
            && [ "$CONSECUTIVE_CRASHES" -ge "$MAX_CONSEC_CRASHES" ]; then
            echo "Stopping: reached $CONSECUTIVE_CRASHES consecutive non-zero exits (limit $MAX_CONSEC_CRASHES). Set MASCORD_PERSISTENT_MAX_CONSECUTIVE_CRASHES=0 to disable."
            exit "$EXIT_CODE"
        fi
        echo "Restarting after crash backoff (${CRASH_BACKOFF}s, max ${BACKOFF_MAX}s)... (Ctrl+C to stop)"
        sleep "$CRASH_BACKOFF"
        NEXT=$((CRASH_BACKOFF * 2))
        if [ "$NEXT" -gt "$BACKOFF_MAX" ]; then
            CRASH_BACKOFF="$BACKOFF_MAX"
        else
            CRASH_BACKOFF="$NEXT"
        fi
    fi
done
