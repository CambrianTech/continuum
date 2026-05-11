#!/bin/bash
# Start Rust workers - reads from workers-config.json for single source of truth
set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

CONFIG_FILE="$(dirname "$0")/workers-config.json"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# All data lives at $HOME/.continuum — matches SystemPaths.root in TypeScript.
CONTINUUM_ROOT="${CONTINUUM_ROOT:-$HOME/.continuum}"

# Resolve .continuum paths from workers-config.json to absolute $HOME paths
resolve_path() {
  echo "$1" | sed "s|^\.continuum|$CONTINUUM_ROOT|"
}

env_truthy() {
  local name="$1"
  local value="${!name:-}"
  case "$value" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

worker_enabled_for_runtime() {
  local worker="$1"
  local enabled_env

  if [ "$(echo "$worker" | jq -r '.enabled // true')" = "false" ]; then
    return 1
  fi

  enabled_env=$(echo "$worker" | jq -r '.enabledEnv // empty')
  if [ -n "$enabled_env" ] && ! env_truthy "$enabled_env"; then
    return 1
  fi

  return 0
}

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

default_core_memory_limit() {
  local phys_mib=""
  if [ "$(uname -s)" = "Darwin" ] && command -v sysctl >/dev/null 2>&1; then
    phys_mib=$(sysctl -n hw.memsize 2>/dev/null | awk '{print int($1/1024/1024)}')
  elif [ -f /proc/meminfo ]; then
    phys_mib=$(awk '/^MemTotal:/{print int($2/1024)}' /proc/meminfo)
  fi

  if [ -z "$phys_mib" ] || [ "$phys_mib" -le 0 ]; then
    echo "16G"
    return
  fi

  local phys_gb=$((phys_mib / 1024))
  if [ "$phys_gb" -ge 32 ]; then
    echo "$((phys_gb - 10))G"
  elif [ "$phys_gb" -ge 20 ]; then
    echo "$((phys_gb - 8))G"
  else
    echo "10G"
  fi
}

# Source config.env to get API keys (HF_TOKEN, etc.) for workers
if [ -f "$HOME/.continuum/config.env" ]; then
  set -a  # Auto-export all variables
  source "$HOME/.continuum/config.env"
  set +a
  echo -e "${GREEN}✅ Loaded config.env (HF_TOKEN, API keys)${NC}"
fi

# Vulkan sanity check — warn if NVIDIA GPU present but no Vulkan ICD installed.
# install.sh handles installing libnvidia-gl; this is a safety net for manual setups.
if (command -v nvidia-smi &>/dev/null || [ -d /usr/lib/wsl/lib ]) && \
   ! [ -f /usr/share/vulkan/icd.d/nvidia_icd.json ]; then
  echo -e "${RED}⚠️  NVIDIA GPU detected but no Vulkan ICD found!${NC}"
  echo -e "${RED}   Bevy will fall back to software rendering (unusable).${NC}"
  echo -e "${RED}   Fix: run 'bash scripts/install.sh' to install libnvidia-gl.${NC}"
fi
# Clean up stale user-space ICD (previous versions created one that confused the loader)
rm -rf "$CONTINUUM_ROOT/vulkan" 2>/dev/null

echo -e "${YELLOW}📋 Loading worker config: $CONFIG_FILE${NC}"

# Check if jq is available
if ! command -v jq &> /dev/null; then
  echo -e "${RED}❌ jq not found. Install with: brew install jq${NC}"
  exit 1
fi

# Setup runtime directories (BEFORE anything writes to them)
mkdir -p "$CONTINUUM_ROOT/jtag/logs/system/modules"
mkdir -p "$CONTINUUM_ROOT/jtag/logs/system/daemons"
mkdir -p "$CONTINUUM_ROOT/sockets"

# Start native LiveKit only when the native live profile is explicitly enabled.
# Default npm start stays text/chat-light; Docker live mode uses compose instead.
if env_truthy CONTINUUM_LIVEKIT_NATIVE; then
  # Check brew first, then manual install location
  LIVEKIT_BIN=$(command -v livekit-server 2>/dev/null || echo "$HOME/.continuum/bin/livekit-server")
  LIVEKIT_LOG="$CONTINUUM_ROOT/jtag/logs/system/livekit-server.log"
  if [ -x "$LIVEKIT_BIN" ] || command -v livekit-server &>/dev/null; then
  # Kill existing LiveKit server (SIGKILL for clean port release)
  pkill -9 -f "livekit-server" 2>/dev/null || true
  # Wait for UDP ports to be fully released (7880 TCP, 7881-7882 UDP)
  # macOS UDP sockets can linger — 3s is safe
  sleep 3

  echo -e "${YELLOW}🔊 Starting LiveKit SFU server...${NC}"
  # Truncate log on startup (prevents multi-MB bloat) and reduce log level
  : > "$LIVEKIT_LOG"

  LIVEKIT_EXTRA_ARGS=""
  LIVEKIT_CONFIG=""
  if grep -qi microsoft /proc/version 2>/dev/null; then
    # WSL2: use YAML config with enable_loopback_candidate (not available as CLI flag).
    # This makes LiveKit generate 127.0.0.1 ICE candidates so the browser on
    # localhost can actually connect. force_tcp because WSL2 Hyper-V firewall
    # blocks inbound UDP (known WSL2 bug, unfixed since 2023).
    LIVEKIT_CONFIG="$CONTINUUM_ROOT/livekit-wsl2.yaml"
    cat > "$LIVEKIT_CONFIG" << 'YAML'
port: 7880
bind_addresses:
  - 0.0.0.0
rtc:
  tcp_port: 7881
  node_ip: 127.0.0.1
  enable_loopback_candidate: true
  force_tcp: true
  use_ice_lite: true
  use_external_ip: false
keys:
  devkey: secret
YAML
    LIVEKIT_EXTRA_ARGS="--config $LIVEKIT_CONFIG"
    echo -e "${YELLOW}   WSL2 — YAML config with loopback ICE + TCP${NC}"
  else
    # Check for TLS certs (Tailscale or manual) in ~/.continuum/
    LIVEKIT_CERT=$(ls "$CONTINUUM_ROOT"/*.crt 2>/dev/null | head -1)
    LIVEKIT_KEY=$(ls "$CONTINUUM_ROOT"/*.key 2>/dev/null | head -1)
    if [ -n "$LIVEKIT_CERT" ] && [ -n "$LIVEKIT_KEY" ]; then
      # TLS available — use config file instead of --dev
      LIVEKIT_CONFIG="$CONTINUUM_ROOT/livekit-tls.yaml"
      LIVEKIT_HOSTNAME=$(echo "$LIVEKIT_CERT" | sed 's|.*/||;s|\.crt$||')
      cat > "$LIVEKIT_CONFIG" << YAML
port: 7880
bind_addresses:
  - 0.0.0.0
rtc:
  tcp_port: 7881
  node_ip: 127.0.0.1
  enable_loopback_candidate: true
keys:
  devkey: secret
turn:
  enabled: true
  domain: $LIVEKIT_HOSTNAME
  tls_port: 5349
  cert_file: $LIVEKIT_CERT
  key_file: $LIVEKIT_KEY
YAML
      LIVEKIT_EXTRA_ARGS="--config $LIVEKIT_CONFIG"
      echo -e "${GREEN}   🔒 TLS enabled: $(basename "$LIVEKIT_CERT")${NC}"
    else
      LIVEKIT_EXTRA_ARGS="--dev --bind 127.0.0.1 --node-ip 127.0.0.1"
    fi
  fi

  livekit_args=()
  if [ -n "$LIVEKIT_EXTRA_ARGS" ]; then
    # shellcheck disable=SC2206
    livekit_args=($LIVEKIT_EXTRA_ARGS)
  fi
  LIVEKIT_PID=$(node "$PROJECT_DIR/scripts/spawn-detached.mjs" \
    --cwd "$PROJECT_DIR" \
    --log "$LIVEKIT_LOG" \
    --env LIVEKIT_LOG_LEVEL=info \
    -- "$LIVEKIT_BIN" "${livekit_args[@]}")

  # Wait for LiveKit to be ready (port 7880)
  for i in {1..20}; do
    if lsof -i :7880 -sTCP:LISTEN > /dev/null 2>&1; then
      echo -e "${GREEN}✅ LiveKit SFU started (PID: $LIVEKIT_PID, port: 7880)${NC}"
      break
    fi
    if [ $i -eq 20 ]; then
      echo -e "${RED}⚠️  LiveKit SFU failed to start (port 7880 not listening after 10s)${NC}"
      echo -e "${YELLOW}💡 Install with: ./scripts/install-livekit.sh${NC}"
    fi
    sleep 0.5
  done
  else
    echo -e "${RED}⚠️  LiveKit server not installed — voice/video calls will NOT work${NC}"
    echo -e "   Install with: ./scripts/install-livekit.sh"
  fi
else
  echo -e "${YELLOW}⏭️  Native LiveKit disabled (set CONTINUUM_LIVEKIT_NATIVE=1 for live media)${NC}"
fi

# Build Rust workers — let cargo handle incremental compilation (it's smart enough)
SCRIPT_DIR="$(dirname "$0")"

# Skip build if --skip-build flag passed (caller already built)
if [[ " $* " == *" --skip-build "* ]]; then
  echo -e "${GREEN}✅ Rust build skipped (--skip-build)${NC}"
else
  echo -e "${YELLOW}🔨 Building Rust workers (cargo incremental)...${NC}"
  (cd "$SCRIPT_DIR" && cargo build --release --quiet)
  echo -e "${GREEN}✅ Rust build complete${NC}"
fi

# Truncate all worker logs on restart (prevents multi-hundred-MB bloat)
# Each session starts clean — old logs are not useful across restarts
for logfile in "$CONTINUUM_ROOT"/jtag/logs/system/*.log "$CONTINUUM_ROOT"/jtag/logs/system/modules/*.log "$CONTINUUM_ROOT"/jtag/logs/system/daemons/*.log; do
  [ -f "$logfile" ] && : > "$logfile"
done
echo -e "${GREEN}✅ Logs truncated${NC}"

# Kill existing workers and clean sockets (same as stop-workers.sh)
echo -e "${YELLOW}🔄 Stopping existing workers...${NC}"
# Use process substitution to avoid subshell (backgrounded processes survive)
# Use binary name from config (single source of truth)
while read -r binary_path; do
  binary_name=$(basename "$binary_path")
  pkill -f "$binary_name" || true
done < <(jq -r '.workers[].binary' "$CONFIG_FILE")

# Give processes time to die and release sockets (macOS needs more time, especially on external drives)
sleep 2.0

# Remove old sockets (use process substitution to avoid subshell)
while read -r socket_path; do
  rm -f "$(resolve_path "$socket_path")"
done < <(jq -r '.workers[].socket' "$CONFIG_FILE")

while read -r socket_path; do
  rm -f "$(resolve_path "$socket_path")"
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

# Set ORT_DYLIB_PATH for ONNX Runtime (needed by Silero VAD in live mode)
# The ort crate with load-dynamic feature needs to find libonnxruntime.dylib
if [ -f "/opt/homebrew/lib/libonnxruntime.dylib" ]; then
  export ORT_DYLIB_PATH="/opt/homebrew/lib/libonnxruntime.dylib"
elif [ -f "/usr/local/lib/libonnxruntime.so" ]; then
  export ORT_DYLIB_PATH="/usr/local/lib/libonnxruntime.so"
fi

while read -r worker; do
  if ! worker_enabled_for_runtime "$worker"; then
    name=$(echo "$worker" | jq -r '.name')
    enabled_env=$(echo "$worker" | jq -r '.enabledEnv // empty')
    if [ -n "$enabled_env" ]; then
      echo -e "${YELLOW}⏭️  Skipping ${name} (${enabled_env} not enabled)${NC}"
    fi
    continue
  fi

  name=$(echo "$worker" | jq -r '.name')
  binary=$(echo "$worker" | jq -r '.binary')
  socket=$(resolve_path "$(echo "$worker" | jq -r '.socket // empty')")
  port=$(echo "$worker" | jq -r '.port // empty')
  worker_type=$(echo "$worker" | jq -r '.type // "socket"')
  description=$(echo "$worker" | jq -r '.description')
  mem_limit=$(echo "$worker" | jq -r '.memoryLimit // empty')
  if [ "$name" = "continuum-core" ] && [ -z "$mem_limit" ]; then
    mem_limit="${CONTINUUM_CORE_MEM:-$(default_core_memory_limit)}"
  fi

  # Get args array (may be empty) — resolve .continuum paths to absolute
  args=$(echo "$worker" | jq -r '.args[]?' | while read -r arg; do resolve_path "$arg"; done || echo "")

  # Calculate memory limit in KB for ulimit
  MEM_LIMIT_KB=$(parse_memory_limit "$mem_limit" "$DEFAULT_MEM_LIMIT")

  echo -e "${YELLOW}🚀 Starting ${name}...${NC}"
  echo -e "   ${description}"
  echo -e "   Memory limit: ${mem_limit:-$DEFAULT_MEM_LIMIT} (${MEM_LIMIT_KB} KB)"

  # ulimit -v: only enforce on macOS. Linux enforces strictly and CUDA/WebRTC
  # need far more virtual memory than the configured limit allows.
  spawn_memory_args=()
  if [ "$(uname -s)" = "Darwin" ]; then
    spawn_memory_args=(--ulimit-v-kb "$MEM_LIMIT_KB")
  fi

  if [ "$worker_type" = "tcp" ]; then
    # TCP worker (e.g., gRPC server) - no socket argument
    WORKER_PID=$(node "$PROJECT_DIR/scripts/spawn-detached.mjs" \
      --cwd "$PROJECT_DIR" \
      --log "$CONTINUUM_ROOT/jtag/logs/system/${name}.log" \
      "${spawn_memory_args[@]}" \
      -- "$binary")

    # Wait for TCP port to be listening
    for i in {1..40}; do
      if lsof -i :$port -sTCP:LISTEN > /dev/null 2>&1; then
        echo -e "${GREEN}✅ ${name} started (PID: $WORKER_PID, port: $port)${NC}"
        break
      fi
      if [ $i -eq 40 ]; then
        echo -e "${RED}❌ ${name} failed to start (port $port not listening after 20s)${NC}"
        echo -e "${YELLOW}💡 Try: tail -50 $CONTINUUM_ROOT/jtag/logs/system/${name}.log${NC}"
        # Don't exit - let other workers start
      fi
      sleep 0.5
    done
  else
    # Unix socket worker - each gets its own log file for better segregation
    arg_array=()
    if [ -n "$args" ]; then
      while IFS= read -r arg; do
        arg_array+=("$arg")
      done <<< "$args"
    fi

    WORKER_PID=$(node "$PROJECT_DIR/scripts/spawn-detached.mjs" \
      --cwd "$PROJECT_DIR" \
      --log "$CONTINUUM_ROOT/jtag/logs/system/${name}.log" \
      "${spawn_memory_args[@]}" \
      -- "$binary" "$socket" "${arg_array[@]}")

    # Wait for socket to be created (30s timeout)
    for i in {1..60}; do
      if [ -S "$socket" ]; then
        echo -e "${GREEN}✅ ${name} started (PID: $WORKER_PID)${NC}"
        break
      fi
      if [ $i -eq 60 ]; then
        echo -e "${RED}❌ ${name} failed to start (socket not created after 30s)${NC}"
        echo -e "${YELLOW}💡 Try: tail -20 $CONTINUUM_ROOT/jtag/logs/system/${name}.log${NC}"
        # Don't exit — non-critical workers shouldn't block server startup.
        # The server will degrade gracefully without search/archive.
        # CRITICAL workers (continuum-core, data-daemon, logger) are checked below.
        break
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
done < <(jq -c '.workers[]' "$CONFIG_FILE")

# Verify all enabled workers are running
sleep 0.5
ALL_RUNNING=true

while read -r worker; do
  if ! worker_enabled_for_runtime "$worker"; then
    continue
  fi

  name=$(echo "$worker" | jq -r '.name')
  binary_name=$(basename "$(echo "$worker" | jq -r '.binary')")
  worker_type=$(echo "$worker" | jq -r '.type // "socket"')
  port=$(echo "$worker" | jq -r '.port // empty')

  if [ "$worker_type" = "tcp" ]; then
    if ! lsof -i :$port -sTCP:LISTEN > /dev/null 2>&1; then
      echo -e "${RED}❌ ${name} not running (port $port not listening)${NC}"
      ALL_RUNNING=false
    fi
  else
    if ! pgrep -f "$binary_name" > /dev/null; then
      echo -e "${RED}❌ ${name} not running${NC}"
      ALL_RUNNING=false
    fi
  fi
done < <(jq -c '.workers[]' "$CONFIG_FILE")

if [ "$ALL_RUNNING" = true ]; then
  echo -e "${GREEN}✅ All workers running successfully${NC}"

  # Show status
  while read -r worker; do
    if ! worker_enabled_for_runtime "$worker"; then
      continue
    fi

    name=$(echo "$worker" | jq -r '.name')
    binary_name=$(basename "$(echo "$worker" | jq -r '.binary')")
    socket=$(echo "$worker" | jq -r '.socket // empty')
    port=$(echo "$worker" | jq -r '.port // empty')
    worker_type=$(echo "$worker" | jq -r '.type // "socket"')

    if [ "$worker_type" = "tcp" ]; then
      pid=$(lsof -i :$port -sTCP:LISTEN -t 2>/dev/null | head -1)
      echo -e "   ${name}: PID $pid (port $port)"
    else
      pid=$(pgrep -f "$binary_name" | head -1)
      echo -e "   ${name}: PID $pid ($socket)"
    fi
  done < <(jq -c '.workers[]' "$CONFIG_FILE")
  exit 0
else
  echo -e "${RED}❌ One or more workers failed to start${NC}"
  exit 1
fi
