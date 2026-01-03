#!/bin/bash
# Start Rust workers - reads from workers-config.json for single source of truth
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

CONFIG_FILE="$(dirname "$0")/workers-config.json"

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
jq -r '.workers[].name' "$CONFIG_FILE" | while read -r worker_name; do
  pkill -f "${worker_name}-worker" || true
done

# Give processes time to die and release sockets (macOS needs more time, especially on external drives)
sleep 2.0

# Remove old sockets
jq -r '.workers[].socket' "$CONFIG_FILE" | while read -r socket_path; do
  rm -f "$socket_path"
done

jq -r '.sharedSockets[]' "$CONFIG_FILE" | while read -r socket_path; do
  rm -f "$socket_path"
done

# Extra safety: wait for sockets to be fully removed before starting new workers
sleep 1.0

# Start each enabled worker
declare -a WORKER_PIDS=()
declare -a WORKER_NAMES=()

jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE" | while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  binary=$(echo "$worker" | jq -r '.binary')
  socket=$(echo "$worker" | jq -r '.socket')
  description=$(echo "$worker" | jq -r '.description')

  # Get args array (may be empty)
  args=$(echo "$worker" | jq -r '.args[]?' || echo "")

  echo -e "${YELLOW}🚀 Starting ${name}-worker...${NC}"
  echo -e "   ${description}"

  # Build command with socket first, then args
  if [ -z "$args" ]; then
    "$binary" "$socket" >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
  else
    # Convert newline-separated args to array
    arg_array=()
    while IFS= read -r arg; do
      arg_array+=("$arg")
    done <<< "$args"
    "$binary" "$socket" "${arg_array[@]}" >> .continuum/jtag/logs/system/rust-worker.log 2>&1 &
  fi

  WORKER_PID=$!

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
done

# Verify all enabled workers are running
sleep 0.5
ALL_RUNNING=true
jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE" | while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  if ! pgrep -f "${name}-worker" > /dev/null; then
    echo -e "${RED}❌ ${name}-worker not running${NC}"
    ALL_RUNNING=false
  fi
done

if [ "$ALL_RUNNING" = true ]; then
  echo -e "${GREEN}✅ All workers running successfully${NC}"

  # Show status
  jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE" | while read -r worker; do
    name=$(echo "$worker" | jq -r '.name')
    socket=$(echo "$worker" | jq -r '.socket')
    pid=$(pgrep -f "${name}-worker" | head -1)
    echo -e "   ${name}-worker: PID $pid ($socket)"
  done
  exit 0
else
  echo -e "${RED}❌ One or more workers failed to start${NC}"
  exit 1
fi
