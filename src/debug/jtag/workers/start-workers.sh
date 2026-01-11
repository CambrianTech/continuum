#!/bin/bash
# Start Rust workers - reads from workers-config.json for single source of truth
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

CONFIG_FILE="$(dirname "$0")/workers-config.json"

# Memory limit helper - converts "8G" to bytes for ulimit
parse_memory_limit() {
  local limit="$1"
  local default="$2"

  if [ -z "$limit" ] || [ "$limit" = "null" ]; then
    limit="$default"
  fi

  # Extract number and unit
  local num=$(echo "$limit" | sed 's/[^0-9]//g')
  local unit=$(echo "$limit" | sed 's/[0-9]//g' | tr '[:lower:]' '[:upper:]')

  case "$unit" in
    G) echo $((num * 1024 * 1024));; # KB for ulimit -v
    M) echo $((num * 1024));;
    K) echo "$num";;
    *) echo $((4 * 1024 * 1024));; # Default 4GB
  esac
}

# Source config.env to get API keys (HF_TOKEN, etc.) for workers
if [ -f "$HOME/.continuum/config.env" ]; then
  set -a  # Auto-export all variables
  source "$HOME/.continuum/config.env"
  set +a
  echo -e "${GREEN}✅ Loaded config.env (HF_TOKEN, API keys)${NC}"
fi

echo -e "${YELLOW}📋 Loading worker config: $CONFIG_FILE${NC}"

# Check if jq is available
if ! command -v jq &> /dev/null; then
  echo -e "${RED}❌ jq not found. Install with: brew install jq${NC}"
  exit 1
fi

# Build all workers from workspace (single build for all crates)
echo -e "${YELLOW}🔨 Building Rust workers...${NC}"
SCRIPT_DIR="$(dirname "$0")"
(cd "$SCRIPT_DIR" && cargo build --release --quiet)
echo -e "${GREEN}✅ Build complete${NC}"

# Setup log directory
mkdir -p .continuum/jtag/logs/system

# Kill existing workers and clean sockets (same as stop-workers.sh)
echo -e "${YELLOW}🔄 Stopping existing workers...${NC}"
# Use process substitution to avoid subshell (backgrounded processes survive)
while read -r worker_name; do
  pkill -f "${worker_name}-worker" || true
done < <(jq -r '.workers[].name' "$CONFIG_FILE")

# Give processes time to die and release sockets (macOS needs more time, especially on external drives)
sleep 2.0

# Remove old sockets (use process substitution to avoid subshell)
while read -r socket_path; do
  rm -f "$socket_path"
done < <(jq -r '.workers[].socket' "$CONFIG_FILE")

while read -r socket_path; do
  rm -f "$socket_path"
done < <(jq -r '.sharedSockets[]' "$CONFIG_FILE")

# Extra safety: wait for sockets to be fully removed before starting new workers
sleep 1.0

# Start each enabled worker
# CRITICAL: Use process substitution to avoid subshell
# Backgrounded processes in piped while loops get SIGHUP when subshell exits
declare -a WORKER_PIDS=()
declare -a WORKER_NAMES=()

# Get default memory limit from config
DEFAULT_MEM_LIMIT=$(jq -r '.memoryLimits.default // "4G"' "$CONFIG_FILE")

while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  binary=$(echo "$worker" | jq -r '.binary')
  socket=$(echo "$worker" | jq -r '.socket // empty')
  port=$(echo "$worker" | jq -r '.port // empty')
  worker_type=$(echo "$worker" | jq -r '.type // "socket"')
  description=$(echo "$worker" | jq -r '.description')
  mem_limit=$(echo "$worker" | jq -r '.memoryLimit // empty')

  # Get args array (may be empty)
  args=$(echo "$worker" | jq -r '.args[]?' || echo "")

  # Calculate memory limit in KB for ulimit
  MEM_LIMIT_KB=$(parse_memory_limit "$mem_limit" "$DEFAULT_MEM_LIMIT")

  echo -e "${YELLOW}🚀 Starting ${name}...${NC}"
  echo -e "   ${description}"
  echo -e "   Memory limit: ${mem_limit:-$DEFAULT_MEM_LIMIT} (${MEM_LIMIT_KB} KB)"

  if [ "$worker_type" = "tcp" ]; then
    # TCP worker (e.g., gRPC server) - no socket argument
    # Note: ulimit -v sets virtual memory limit; may not be enforced on macOS
    (ulimit -v $MEM_LIMIT_KB 2>/dev/null || true; exec "$binary") >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
    WORKER_PID=$!
    disown $WORKER_PID

    # Wait for TCP port to be listening
    for i in {1..40}; do
      if lsof -i :$port -sTCP:LISTEN > /dev/null 2>&1; then
        echo -e "${GREEN}✅ ${name} started (PID: $WORKER_PID, port: $port)${NC}"
        break
      fi
      if [ $i -eq 40 ]; then
        echo -e "${RED}❌ ${name} failed to start (port $port not listening after 20s)${NC}"
        echo -e "${YELLOW}💡 Try: tail -50 .continuum/jtag/logs/system/rust-worker.log${NC}"
        # Don't exit - let other workers start
      fi
      sleep 0.5
    done
  else
    # Unix socket worker (original behavior)
    # Note: ulimit -v sets virtual memory limit; may not be enforced on macOS
    if [ -z "$args" ]; then
      (ulimit -v $MEM_LIMIT_KB 2>/dev/null || true; exec "$binary" "$socket") >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
    else
      # Convert newline-separated args to array
      arg_array=()
      while IFS= read -r arg; do
        arg_array+=("$arg")
      done <<< "$args"
      (ulimit -v $MEM_LIMIT_KB 2>/dev/null || true; exec "$binary" "$socket" "${arg_array[@]}") >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
    fi

    WORKER_PID=$!
    disown $WORKER_PID  # Fully detach from shell

    # Wait for socket to be created with increased timeout for macOS
    for i in {1..20}; do
      if [ -S "$socket" ]; then
        echo -e "${GREEN}✅ ${name} started (PID: $WORKER_PID)${NC}"
        break
      fi
      if [ $i -eq 20 ]; then
        echo -e "${RED}❌ ${name} failed to start (socket not created after 10s)${NC}"
        echo -e "${YELLOW}💡 Try: tail -20 .continuum/jtag/logs/system/rust-worker.log${NC}"
        exit 1
      fi
      sleep 0.5
    done

    # Preload models for inference worker (if configured)
    preload_models=$(echo "$worker" | jq -r '.preloadModels[]?' 2>/dev/null || echo "")
    if [ -n "$preload_models" ] && [ "$name" = "inference" ]; then
      echo -e "${YELLOW}📦 Preloading models for ${name}...${NC}"
      while IFS= read -r model_id; do
        if [ -n "$model_id" ]; then
          echo -e "   Loading: $model_id (may take 10-60s for first download)..."
          response=$(echo "{\"command\":\"model/load\",\"request_id\":\"preload\",\"model_id\":\"$model_id\"}" | timeout 300 nc -U "$socket" 2>&1)
          if echo "$response" | grep -q '"success":true'; then
            load_time=$(echo "$response" | grep -o '"load_time_ms":[0-9]*' | grep -o '[0-9]*' || echo "?")
            echo -e "   ${GREEN}✅ Loaded $model_id (${load_time}ms)${NC}"
          else
            echo -e "   ${YELLOW}⚠️ Failed to load $model_id: $(echo "$response" | head -c 200)${NC}"
          fi
        fi
      done <<< "$preload_models"
    fi
  fi
done < <(jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE")

# Verify all enabled workers are running
sleep 0.5
ALL_RUNNING=true

while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  worker_type=$(echo "$worker" | jq -r '.type // "socket"')
  port=$(echo "$worker" | jq -r '.port // empty')

  if [ "$worker_type" = "tcp" ]; then
    if ! lsof -i :$port -sTCP:LISTEN > /dev/null 2>&1; then
      echo -e "${RED}❌ ${name} not running (port $port not listening)${NC}"
      ALL_RUNNING=false
    fi
  else
    if ! pgrep -f "${name}" > /dev/null; then
      echo -e "${RED}❌ ${name} not running${NC}"
      ALL_RUNNING=false
    fi
  fi
done < <(jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE")

if [ "$ALL_RUNNING" = true ]; then
  echo -e "${GREEN}✅ All workers running successfully${NC}"

  # Show status
  while read -r worker; do
    name=$(echo "$worker" | jq -r '.name')
    socket=$(echo "$worker" | jq -r '.socket // empty')
    port=$(echo "$worker" | jq -r '.port // empty')
    worker_type=$(echo "$worker" | jq -r '.type // "socket"')

    if [ "$worker_type" = "tcp" ]; then
      pid=$(lsof -i :$port -sTCP:LISTEN -t 2>/dev/null | head -1)
      echo -e "   ${name}: PID $pid (port $port)"
    else
      pid=$(pgrep -f "${name}" | head -1)
      echo -e "   ${name}: PID $pid ($socket)"
    fi
  done < <(jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE")
  exit 0
else
  echo -e "${RED}❌ One or more workers failed to start${NC}"
  exit 1
fi
