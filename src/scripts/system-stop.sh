#!/bin/bash
# System Stop — Nuclear process cleanup
# Kills ALL JTAG-related processes and cleans up sockets/signals.
# This is the ONLY stop script. Keep it simple and brutal.

# No set -e — stop is best-effort, don't abort on kill failures

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/shared/preflight.sh"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="$PROJECT_DIR/workers/workers-config.json"

# All data lives at $HOME/.continuum — matches SystemPaths.root in TypeScript.
CONTINUUM_ROOT="${CONTINUUM_ROOT:-$HOME/.continuum}"

echo -e "${YELLOW}🛑 Stopping JTAG system...${NC}"

# 1. Kill tmux session (if any)
# Try known session name patterns
for session in $(tmux list-sessions -F '#{session_name}' 2>/dev/null | grep '^jtag-' || true); do
  echo -e "   Killing tmux session: $session"
  tmux kill-session -t "$session" 2>/dev/null || true
done

# 2. Kill LiveKit server (SIGKILL — holds UDP ports that linger on SIGTERM)
if pgrep -f "livekit-server" > /dev/null 2>&1; then
  echo -e "   Stopping LiveKit server..."
  pkill -9 -f "livekit-server" 2>/dev/null || true
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

# 8. Kill orphaned sentinel child processes (training, builds, etc.)
# Each sentinel writes a PID file — kill the process GROUP (setsid) to get all descendants.
# This catches training processes that survive server death.
SENTINEL_LOGS="$CONTINUUM_ROOT/jtag/logs/system/sentinels"
if [ -d "$SENTINEL_LOGS" ]; then
  orphan_count=0
  for pid_file in "$SENTINEL_LOGS"/*/pid; do
    if [ -f "$pid_file" ]; then
      pid=$(cat "$pid_file" 2>/dev/null)
      if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        echo -e "   Killing sentinel process group (PID $pid)"
        kill -- "-$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
        orphan_count=$((orphan_count + 1))
      fi
      rm -f "$pid_file" 2>/dev/null || true
    fi
  done
  if [ "$orphan_count" -gt 0 ]; then
    echo -e "   Killed $orphan_count orphaned sentinel process groups"
    sleep 1
    # Force kill any survivors
    for pid_file in "$SENTINEL_LOGS"/*/pid; do
      if [ -f "$pid_file" ]; then
        pid=$(cat "$pid_file" 2>/dev/null)
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
          kill -9 -- "-$pid" 2>/dev/null || kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$pid_file" 2>/dev/null || true
      fi
    done
  fi
fi

# 9. Clean up sockets
if [ -d "$CONTINUUM_ROOT/sockets" ]; then
  echo -e "   Cleaning sockets..."
  rm -f "$CONTINUUM_ROOT/sockets/"*.sock 2>/dev/null || true
fi

# 10. Clear ready signal
if [ -f "$CONTINUUM_ROOT/jtag/system-ready.signal" ]; then
  rm -f "$CONTINUUM_ROOT/jtag/system-ready.signal"
fi

# 11. Remove PID files
rm -f "$CONTINUUM_ROOT/jtag/logs/system/npm-start.pid" 2>/dev/null || true
rm -f "$CONTINUUM_ROOT/jtag/system.lock" 2>/dev/null || true

echo -e "${GREEN}✅ JTAG system stopped${NC}"
