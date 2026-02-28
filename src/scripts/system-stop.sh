#!/bin/bash
# System Stop — Nuclear process cleanup
# Kills ALL JTAG-related processes and cleans up sockets/signals.
# This is the ONLY stop script. Keep it simple and brutal.

# No set -e — stop is best-effort, don't abort on kill failures

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="$PROJECT_DIR/workers/workers-config.json"

echo -e "${YELLOW}🛑 Stopping JTAG system...${NC}"

# 1. Kill tmux session (if any)
# Try known session name patterns
for session in $(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep '^jtag-' || true); do
  echo -e "   Killing tmux session: $session"
  tmux kill-session -t "$session" 2>/dev/null || true
done

# 2. Kill LiveKit server
if pgrep -f "livekit-server" > /dev/null 2>&1; then
  echo -e "   Stopping LiveKit server..."
  pkill -f "livekit-server" 2>/dev/null || true
fi

# 3. Kill Rust workers (from workers-config.json)
if command -v jq &> /dev/null && [ -f "$CONFIG_FILE" ]; then
  while read -r binary_path; do
    binary_name=$(basename "$binary_path")
    if pgrep -f "$binary_name" > /dev/null 2>&1; then
      echo -e "   Stopping worker: $binary_name"
      pkill -f "$binary_name" 2>/dev/null || true
    fi
  done < <(jq -r '.workers[].binary' "$CONFIG_FILE")
fi

# 4. Kill any cargo build in progress (in our workspace only)
if pgrep -f "cargo build.*release" > /dev/null 2>&1; then
  echo -e "   Stopping cargo build..."
  pkill -f "cargo build.*release" 2>/dev/null || true
fi

# 5. Kill known JTAG server processes (not generic npm/node — that would kill our caller)
# Target specific scripts that run the server
for pattern in "launch-active-example" "minimal-server" "launch-and-capture" "smart-deploy" "signal-system-ready"; do
  while IFS= read -r pid; do
    if [ -n "$pid" ] && [ "$pid" != "$$" ] && [ "$pid" != "$PPID" ]; then
      echo -e "   Killing $pattern (PID $pid)"
      kill "$pid" 2>/dev/null || true
    fi
  done < <(pgrep -f "$pattern" 2>/dev/null || true)
done

# 6. Give processes 2 seconds to die
sleep 2

# 7. Force kill anything still on our ports
for port in 9000 9001 7880; do
  pids=$(lsof -ti ":$port" 2>/dev/null || true)
  if [ -n "$pids" ]; then
    echo -e "   Force killing processes on port $port: $pids"
    echo "$pids" | xargs kill -9 2>/dev/null || true
  fi
done

# 8. Clean up sockets
if [ -d "$PROJECT_DIR/.continuum/sockets" ]; then
  echo -e "   Cleaning sockets..."
  rm -f "$PROJECT_DIR/.continuum/sockets/"*.sock 2>/dev/null || true
fi

# 9. Clear ready signal
if [ -f "$PROJECT_DIR/.continuum/jtag/system-ready.signal" ]; then
  rm -f "$PROJECT_DIR/.continuum/jtag/system-ready.signal"
fi

# 10. Remove PID files
rm -f "$PROJECT_DIR/.continuum/jtag/logs/system/npm-start.pid" 2>/dev/null || true
rm -f "$PROJECT_DIR/.continuum/jtag/system.lock" 2>/dev/null || true

echo -e "${GREEN}✅ JTAG system stopped${NC}"
