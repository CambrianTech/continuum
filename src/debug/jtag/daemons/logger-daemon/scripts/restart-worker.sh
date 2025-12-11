#!/bin/bash
# Rebuild and restart Rust logger worker

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
JTAG_ROOT="$SCRIPT_DIR/../../.."
WORKER_DIR="$JTAG_ROOT/workers/logger"
SOCKET_PATH="/tmp/jtag-logger-worker.sock"
LOG_FILE="$JTAG_ROOT/.continuum/jtag/logs/system/rust-worker.log"

echo "========================================="
echo "Restarting Rust Logger Worker"
echo "========================================="

# 1. Kill old worker
echo "1. Killing old worker..."
pkill -f logger-worker || true
sleep 1

# 2. Clean up
echo "2. Cleaning up..."
rm -f "$SOCKET_PATH"
rm -f /tmp/rust-worker-debug.log
rm -f /tmp/worker-client-debug.log

# 3. Build Rust worker
echo "3. Building Rust worker..."
cd "$WORKER_DIR"
cargo build --release

# 4. Check binary
BINARY="$WORKER_DIR/target/release/logger-worker"
if [ ! -f "$BINARY" ]; then
    echo "ERROR: Binary not found at $BINARY"
    exit 1
fi

echo "   Binary: $(ls -lh $BINARY | awk '{print $5, $9}')"
echo "   MD5: $(md5 -q $BINARY)"

# 5. Start new worker
echo "4. Starting new worker..."
mkdir -p "$(dirname "$LOG_FILE")"
"$BINARY" "$SOCKET_PATH" > "$LOG_FILE" 2>&1 &
WORKER_PID=$!

sleep 2

# 6. Verify it's running
if ps -p $WORKER_PID > /dev/null; then
    echo "✅ Worker started successfully (PID: $WORKER_PID)"
    echo "   Socket: $SOCKET_PATH"
    echo "   Logs: $LOG_FILE"
    echo "   Debug: /tmp/rust-worker-debug.log"
else
    echo "❌ Worker failed to start"
    cat "$LOG_FILE"
    exit 1
fi

echo ""
echo "Debug logs:"
echo "  TypeScript: /tmp/worker-client-debug.log"
echo "  Rust: /tmp/rust-worker-debug.log"
echo ""
echo "========================================="
