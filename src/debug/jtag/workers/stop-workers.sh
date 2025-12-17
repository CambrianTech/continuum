#!/bin/bash
# Mirror of start-workers.sh - reads from workers-config.json for single source of truth

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

CONFIG_FILE="$(dirname "$0")/workers-config.json"

echo -e "${YELLOW}🛑 Stopping Rust workers (using config: $CONFIG_FILE)...${NC}"

# Check if jq is available
if ! command -v jq &> /dev/null; then
  echo -e "${RED}❌ jq not found. Install with: brew install jq${NC}"
  exit 1
fi

# Read worker names from config and kill them
jq -r '.workers[].name' "$CONFIG_FILE" | while read -r worker_name; do
  echo -e "   Stopping ${worker_name}-worker..."
  pkill -f "${worker_name}-worker" || true
done

# Give processes time to die and flush (macOS needs more time)
sleep 1.5

# Remove all worker sockets from config
echo -e "${YELLOW}🧹 Cleaning up sockets...${NC}"
jq -r '.workers[].socket' "$CONFIG_FILE" | while read -r socket_path; do
  if [ -S "$socket_path" ]; then
    rm -f "$socket_path"
    echo -e "   Removed: $socket_path"
  fi
done

# Remove shared sockets
jq -r '.sharedSockets[]' "$CONFIG_FILE" | while read -r socket_path; do
  if [ -S "$socket_path" ]; then
    rm -f "$socket_path"
    echo -e "   Removed: $socket_path"
  fi
done

# Extra safety: wait for sockets to be fully removed
sleep 0.5

# Verify workers are stopped by checking each worker name from config
STILL_RUNNING=false
jq -r '.workers[].name' "$CONFIG_FILE" | while read -r worker_name; do
  if pgrep -f "${worker_name}-worker" > /dev/null; then
    echo -e "${RED}❌ ${worker_name}-worker still running${NC}"
    pgrep -f "${worker_name}-worker" | while read pid; do
      echo -e "   Orphaned PID: $pid"
    done
    STILL_RUNNING=true
  fi
done

if [ "$STILL_RUNNING" = true ]; then
  exit 1
else
  echo -e "${GREEN}✅ All workers stopped successfully${NC}"
  exit 0
fi
