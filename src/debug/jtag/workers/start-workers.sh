#!/bin/bash
# Start Rust workers - reads from workers-config.json for single source of truth
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

CONFIG_FILE="$(dirname "$0")/workers-config.json"

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

while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  binary=$(echo "$worker" | jq -r '.binary')
  socket=$(echo "$worker" | jq -r '.socket')
  description=$(echo "$worker" | jq -r '.description')

  # Get args array (may be empty)
  args=$(echo "$worker" | jq -r '.args[]?' || echo "")

  echo -e "${YELLOW}🚀 Starting ${name}-worker...${NC}"
  echo -e "   ${description}"

  # Build command with socket first, then args
  # Use nohup to prevent SIGHUP on shell exit
  if [ -z "$args" ]; then
    nohup "$binary" "$socket" >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
  else
    # Convert newline-separated args to array
    arg_array=()
    while IFS= read -r arg; do
      arg_array+=("$arg")
    done <<< "$args"
    nohup "$binary" "$socket" "${arg_array[@]}" >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
  fi

  WORKER_PID=$!
  disown $WORKER_PID  # Fully detach from shell

  # Wait for socket to be created with increased timeout for macOS
  # macOS can be slow with filesystem operations, especially on external drives
  for i in {1..20}; do
    if [ -S "$socket" ]; then
      echo -e "${GREEN}✅ ${name}-worker started (PID: $WORKER_PID)${NC}"
      break
    fi
    if [ $i -eq 20 ]; then
      echo -e "${RED}❌ ${name}-worker failed to start (socket not created after 10s)${NC}"
      echo -e "${YELLOW}💡 Try: tail -20 .continuum/jtag/logs/system/rust-worker.log${NC}"
      exit 1
    fi
    sleep 0.5
  done

  # Preload models for inference worker (if configured)
  preload_models=$(echo "$worker" | jq -r '.preloadModels[]?' 2>/dev/null || echo "")
  if [ -n "$preload_models" ] && [ "$name" = "inference" ]; then
    echo -e "${YELLOW}📦 Preloading models for ${name}-worker...${NC}"
    while IFS= read -r model_id; do
      if [ -n "$model_id" ]; then
        echo -e "   Loading: $model_id (may take 10-60s for first download)..."
        # Send model/load command with timeout
        # Use timeout command to limit wait time (5 min max for first-time HF downloads)
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
done < <(jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE")

# Verify all enabled workers are running
sleep 0.5
ALL_RUNNING=true

while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  if ! pgrep -f "${name}-worker" > /dev/null; then
    echo -e "${RED}❌ ${name}-worker not running${NC}"
    ALL_RUNNING=false
  fi
done < <(jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE")

if [ "$ALL_RUNNING" = true ]; then
  echo -e "${GREEN}✅ All workers running successfully${NC}"

  # Show status
  while read -r worker; do
    name=$(echo "$worker" | jq -r '.name')
    socket=$(echo "$worker" | jq -r '.socket')
    pid=$(pgrep -f "${name}-worker" | head -1)
    echo -e "   ${name}-worker: PID $pid ($socket)"
  done < <(jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE")
  exit 0
else
  echo -e "${RED}❌ One or more workers failed to start${NC}"
  exit 1
fi
