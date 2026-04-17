#!/bin/bash
# test-heartbeat.sh — End-to-end heartbeat slice for a continuum image set.
#
# This is the integration heartbeat — proves the WHOLE STACK (continuum-core
# + node-server + widget-server + postgres + livekit-bridge + model-init) boots
# and serves a real persona reply to a real chat send, with llama.cpp inference
# traces visible. If a slice probe verifies one component works, this verifies
# they all work TOGETHER for the actual user-facing workflow.
#
# Per the PR891 ship-pipeline + QoS plan, this is the gate-before-merge: any
# PR whose images fail the heartbeat doesn't ship. It runs locally for dev
# validation and in CI for merge gating.
#
# Usage:
#   scripts/test-heartbeat.sh [image-tag]
#
#   image-tag defaults to the current git HEAD's :<sha>. Override to validate
#   a specific PR's :pr-<N> tag, or :latest, or any sha.
#
# Examples:
#   scripts/test-heartbeat.sh                # this branch's HEAD :<sha>
#   scripts/test-heartbeat.sh pr-891         # validate PR891's staged images
#   scripts/test-heartbeat.sh latest         # validate main's published set
#
# Variant selection: the script picks the right compose file set based on
# host capabilities — Mac with Podman+krunkit gets docker-compose.mac.yml
# (vulkan), Linux with NVIDIA gets docker-compose.gpu.yml (cuda), neither
# gets the bare CPU baseline (which will likely fail the inference assertions
# — that's the point, CPU isn't a shipping path per the never-CPU directive).
#
# Exit codes:
#   0 = heartbeat green (full stack healthy + persona replied + inference traced)
#   1 = pre-flight error (missing tools, daemon down, no compose files)
#   2 = a heartbeat assertion failed (specific failure named in stderr)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Tag resolution: arg > git sha > latest. Compose file CONTINUUM_IMAGE_TAG
# variable consumes whatever we export.
SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo latest)"
TAG="${1:-$SHA}"
export CONTINUUM_IMAGE_TAG="$TAG"

# Host detection + path selection.
# Mac: continuum-core runs NATIVE (via npm start), support services in Docker
# Desktop containers. LLM inference routes to Docker Model Runner's vllm-metal
# (also host-native). Heartbeat probes the native socket + model-runner HTTP.
# Linux: continuum-core runs CONTAINERIZED with GPU passthrough (cuda or
# vulkan). Heartbeat probes the container's inference directly.
HOST_OS="$(uname -s)"
CONTAINER_CMD=docker
if ! command -v docker &>/dev/null; then
  echo "ERROR: docker not installed. Heartbeat requires Docker Desktop (Mac) or Docker Engine (Linux)." >&2
  exit 1
fi
if ! docker info &>/dev/null; then
  echo "ERROR: docker daemon not reachable." >&2
  case "$HOST_OS" in
    Darwin) echo "       Start Docker Desktop, wait for it to be ready, then re-run." >&2 ;;
    *)      echo "       Start Docker Engine / Desktop and retry." >&2 ;;
  esac
  exit 1
fi

# Compose file selection — match install.sh's logic.
COMPOSE_FILES="-f $REPO_ROOT/docker-compose.yml"
PROFILE_ARGS=""
case "$HOST_OS" in
  Darwin)
    [[ -f "$REPO_ROOT/docker-compose.mac.yml" ]] || {
      echo "ERROR: docker-compose.mac.yml missing. Mac heartbeat needs the override that excludes continuum-core from containers." >&2
      exit 1
    }
    COMPOSE_FILES="$COMPOSE_FILES -f $REPO_ROOT/docker-compose.mac.yml"
    HEARTBEAT_VARIANT="mac-native (support services in Docker, continuum-core native, LLM via Docker Model Runner)"
    # Model Runner required on Mac.
    if ! docker model --help &>/dev/null 2>&1; then
      echo "ERROR: 'docker model' not available. Needs Docker Desktop 4.62+ with Model Runner." >&2
      exit 1
    fi
    ;;
  Linux)
    if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null; then
      [[ -f "$REPO_ROOT/docker-compose.gpu.yml" ]] || {
        echo "ERROR: docker-compose.gpu.yml missing. Nvidia host heartbeat needs cuda override." >&2
        exit 1
      }
      COMPOSE_FILES="$COMPOSE_FILES -f $REPO_ROOT/docker-compose.gpu.yml"
      PROFILE_ARGS="--profile gpu"
      HEARTBEAT_VARIANT="cuda (container GPU passthrough)"
    else
      HEARTBEAT_VARIANT="vulkan (Linux AMD/Intel/VirtIO — container /dev/dri passthrough)"
    fi
    ;;
esac

# ── Helpers ─────────────────────────────────────────────────────────
FAILS=()

pass() { echo "  ✓ $1"; }
fail() {
  echo "  ✗ $1: $2" >&2
  FAILS+=("$1")
}

cleanup() {
  echo ""
  echo "→ Tearing down stack…"
  # Stop this run's compose stack
  $CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS down --timeout 5 >/dev/null 2>&1 || true
  # Also reap any stale containers from prior heartbeat runs that may still
  # be holding ports (7882/9001/9003 etc). We don't kill the user's native
  # continuum-core-server — that's intentionally untouched.
  for stale in $($CONTAINER_CMD ps --filter "label=com.docker.compose.project" --format "{{.ID}}" 2>/dev/null); do
    PROJ=$($CONTAINER_CMD inspect --format '{{ index .Config.Labels "com.docker.compose.project" }}' "$stale" 2>/dev/null)
    if [[ "$PROJ" == "continuum" ]]; then
      $CONTAINER_CMD rm -f "$stale" >/dev/null 2>&1 || true
    fi
  done
}
trap cleanup EXIT

# Pre-flight: run cleanup ONCE at start too, so re-runs don't hit port
# conflicts from prior aborted heartbeats. Idempotent.
cleanup 2>/dev/null || true

# Pre-flight (Mac-only): detect stale/duplicate native continuum-core-server.
# Option B runs continuum-core NATIVE on the host; a leftover process from a
# prior `npm start` silently loses the Unix socket + TCP listener bind race
# against a new one, leaving orchestration half-dead. Symptom matches the
# 180s "JTAG system did not become ready" timeout that seed-continuum.ts
# hits — ping returns but systemReady never flips, Rust IPC never confirms.
#
# NEVER killed silently — a legit in-use session stays untouched. Fail loud
# with exact cleanup commands so the user decides.
if [[ "$HEARTBEAT_VARIANT" == mac-native* ]]; then
  CORE_PIDS=$(pgrep -fl "continuum-core-server" 2>/dev/null | awk '{print $1}' | tr '\n' ' ' | sed 's/ $//')
  if [[ -n "$CORE_PIDS" ]]; then
    CORE_COUNT=$(echo "$CORE_PIDS" | wc -w | tr -d ' ')
    if [[ "$CORE_COUNT" -gt 1 ]]; then
      echo "ERROR: $CORE_COUNT continuum-core-server processes running: $CORE_PIDS" >&2
      echo "       Multiple native cores fight for the Unix socket + TCP 9100;" >&2
      echo "       orchestration wedges. Kill all and re-run:" >&2
      echo "         pkill -9 -f continuum-core-server" >&2
      echo "         rm -f ~/.continuum/sockets/*.sock" >&2
      echo "         cd src && CONTINUUM_CORE_TCP=9100 npm start" >&2
      exit 1
    fi
    # Single core — verify it's responsive (not wedged with MEMLEAK etc).
    if ! timeout 5 "$REPO_ROOT/src/jtag" ping >/dev/null 2>&1; then
      echo "ERROR: continuum-core-server PID $CORE_PIDS is running but unresponsive" >&2
      echo "       (./jtag ping timed out at 5s). Likely wedged. Kill and restart:" >&2
      echo "         pkill -9 -f continuum-core-server" >&2
      echo "         rm -f ~/.continuum/sockets/*.sock" >&2
      echo "         cd src && CONTINUUM_CORE_TCP=9100 npm start" >&2
      exit 1
    fi
    echo "  ✓ pre-flight: single responsive continuum-core-server (PID $CORE_PIDS)"
  fi
fi

echo ""
echo "━━━ heartbeat: variant=$HEARTBEAT_VARIANT  tag=$TAG ━━━"
echo ""

# ── Slice 1: images available (registry OR locally) + compose up ────
# Heartbeat is used for two cases:
#   (a) Dev validation on a freshly-built local image (via push-image.sh
#       Phase 1 --load, before the push). Registry won't have the tag yet.
#   (b) CI / reviewer validation against a published :pr-<N> or :<sha> tag.
#       Registry has it.
# So: try pull, IGNORE failure if the image exists locally (case a), FAIL
# loud only if neither local nor registry has it.
echo "→ Resolving image set ($TAG)…"
$CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS pull --quiet 2>/dev/null \
  && pass "pull (images fetched from registry)" \
  || {
    # Pull failed. Check if images exist locally for each service that
    # compose needs — on Mac that's support services only (continuum-core
    # runs native), on Linux it's the full set.
    # Build a list of services that will ACTUALLY run (replicas > 0).
    # On Mac the override sets continuum-core.replicas=0 because it runs
    # natively — don't require its image to be present.
    CONFIG=$($CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS config 2>/dev/null)
    MISSING=""
    for svc in $($CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS config --services 2>/dev/null); do
      # Parse this service's block from compose config output to get image + replicas
      BLOCK=$(echo "$CONFIG" | awk -v svc="$svc" '
          $0 ~ "^  "svc":" {found=1; next}
          found && $0 ~ "^  [a-z]" {found=0}
          found {print}')
      IMG=$(echo "$BLOCK" | awk '$1 == "image:" {print $2; exit}')
      # deploy.replicas nests under deploy: so look for it inside the block
      REPLICAS=$(echo "$BLOCK" | awk '
          $1 == "deploy:" {in_deploy=1; next}
          in_deploy && $1 == "replicas:" {print $2; exit}
          in_deploy && $0 ~ "^    [a-z]" {next}
          in_deploy && $0 !~ "^  " {in_deploy=0}')
      # Skip services that won't actually run
      if [[ "$REPLICAS" == "0" ]]; then
        continue
      fi
      if [[ -n "$IMG" ]] && ! $CONTAINER_CMD image inspect "$IMG" &>/dev/null; then
        MISSING+="\n    $svc: $IMG"
      fi
    done
    if [[ -z "$MISSING" ]]; then
      pass "pull skipped — all images available locally (dev build mode)"
    else
      fail "images-available" "registry pull failed AND images missing locally:$MISSING"
      echo "  Fix: build locally via scripts/push-image.sh <variant>, OR wait for CI to publish." >&2
      exit 2
    fi
  }

echo "→ Composing up…"
if ! $CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS up -d >/dev/null 2>&1; then
  fail "compose-up" "compose failed to start; check '$CONTAINER_CMD compose logs'"
  exit 2
fi
pass "compose-up (all services started)"

# ── Slice 2: widget reachable ───────────────────────────────────────
WIDGET_URL_HTTP="http://localhost:9003"
WIDGET_URL_HTTPS="https://localhost:9003"

WIDGET_READY=false
for _ in $(seq 1 60); do
  if curl -sf "$WIDGET_URL_HTTP" >/dev/null 2>&1 \
     || curl -sf "$WIDGET_URL_HTTPS" -k >/dev/null 2>&1; then
    WIDGET_READY=true
    break
  fi
  sleep 2
done
if $WIDGET_READY; then
  pass "widget-reachable (HTTP 200 within 120s)"
else
  fail "widget-reachable" "widget never returned 200 within 120s"
  echo "  recent compose logs:" >&2
  $CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS logs --tail=40 widget-server 2>&1 | sed 's/^/    /' >&2
  exit 2
fi

# ── Slice 3: persona inference round-trip ───────────────────────────
# Send a chat to Helper AI via continuum-core's IPC (through node-server).
# Then poll the chat log inside the continuum-core container for a reply.
PROBE_MSG="heartbeat probe $(date +%s)"
echo "→ Sending probe chat to Helper AI…"

# This uses the same path users hit — the widget's chat command via node-server's
# REST/WS surface. If continuum-core's running ./jtag binary is in PATH inside
# the container, we can invoke it; otherwise fall back to a curl against the
# widget API.
SEND_RESULT=$($CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS exec -T node-server \
  curl -sf -X POST http://continuum-core:9000/chat/send \
  -H "Content-Type: application/json" \
  -d "{\"room\":\"general\",\"message\":\"$PROBE_MSG\",\"to\":\"helper\"}" 2>&1 || true)

if [[ -n "$SEND_RESULT" ]]; then
  pass "chat-send (probe message accepted)"
else
  fail "chat-send" "POST /chat/send returned no body — node-server may not be wired to continuum-core IPC"
  # Don't exit — continue to log-trace check, may still see useful signal
fi

# ── Slice 4: inference traces present ───────────────────────────────
echo "→ Waiting up to 90s for llama.cpp inference traces…"
TRACE_FOUND=false
for _ in $(seq 1 30); do
  LOGS=$($CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS logs --tail=200 continuum-core 2>&1 || true)
  # Look for any of: model loader output, kernel compile (Metal/CUDA/Vulkan),
  # generate trace, or persona response generation.
  if echo "$LOGS" | grep -qE "llama_model_loader|ggml_metal_library_compile|ggml_cuda_init|ggml_vulkan|generate:|llama_new_context_with_model"; then
    TRACE_FOUND=true
    break
  fi
  sleep 3
done
if $TRACE_FOUND; then
  pass "inference-traces (llama.cpp activity in continuum-core log)"
else
  fail "inference-traces" "no llama.cpp model-load or generate traces in continuum-core log within 90s"
  echo "  last 30 lines of continuum-core log:" >&2
  $CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS logs --tail=30 continuum-core 2>&1 | sed 's/^/    /' >&2
fi

# ── Slice 5: acceleration engaged (variant-specific) ───────────────
case "$HEARTBEAT_VARIANT" in
  mac-native*)
    # Mac path: continuum-core is NATIVE on host. Check that the IPC socket
    # exists where we expect it — simpler than lsof output parsing and
    # doesn't depend on permissions or lsof flavor differences.
    CORE_SOCK="$HOME/.continuum/sockets/continuum-core.sock"
    if [[ -S "$CORE_SOCK" ]]; then
      pass "native-core-running (socket $CORE_SOCK present)"
    else
      # Diagnose WHY it's missing — pgrep for the process to help the user
      if pgrep -f "continuum-core-server" &>/dev/null; then
        fail "native-core-running" "continuum-core-server process IS running (pgrep found it) but socket $CORE_SOCK not found. Socket-path mismatch? Check the IPC Socket line in the server's startup log."
      else
        fail "native-core-running" "no continuum-core-server process running. Launch with: cd src && npm start  (or run the binary directly with an explicit socket path)"
      fi
    fi
    # Docker Model Runner's vllm backend status (host-native, managed by Docker Desktop)
    # `docker model status` shows each registered backend in a BACKEND col.
    # Look for a line starting with 'vllm' + STATUS=Running.
    if docker model status 2>/dev/null | awk '/^vllm[[:space:]]+Running/{found=1} END{exit !found}'; then
      pass "model-runner-vllm (vllm-metal backend registered + Running)"
    else
      fail "model-runner-vllm" "vllm backend not registered. Run: docker model install-runner --backend vllm"
    fi
    # Verify continuum-core log shows Metal activity (ggml_metal kernel compile
    # = real Metal on Apple GPU, not any form of emulation).
    METAL_LOG="$HOME/.continuum/jtag/logs/system/continuum-core.log"
    if [[ -f "$METAL_LOG" ]] && grep -qE "ggml_metal_library_compile|ggml_metal_init" "$METAL_LOG"; then
      pass "metal-engaged (ggml_metal kernel compilation visible in log — real Apple GPU)"
    else
      fail "metal-engaged" "no ggml_metal traces in continuum-core.log — native Metal not firing"
    fi
    ;;
  cuda*)
    GPU_BUSY=$($CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS exec -T continuum-core \
      nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>/dev/null | head -1 || echo "0")
    if [[ "$GPU_BUSY" -gt 1000 ]]; then
      pass "cuda-gpu-engaged (VRAM used: ${GPU_BUSY}MB — model is loaded on GPU)"
    else
      fail "cuda-gpu-engaged" "nvidia-smi shows ${GPU_BUSY}MB VRAM in use — model didn't load on GPU"
    fi
    ;;
  vulkan*)
    DEV=$($CONTAINER_CMD compose $COMPOSE_FILES $PROFILE_ARGS exec -T continuum-core \
      vulkaninfo --summary 2>&1 | grep -E "deviceName" | head -1 | sed 's/.*= *//' || true)
    if [[ -n "$DEV" ]]; then
      pass "vulkan-device ($DEV)"
      # Reject llvmpipe at heartbeat-level — that's CPU emulation, banned per
      # feedback_no_emulation_at_inference.md
      if echo "$DEV" | grep -qi "llvmpipe"; then
        fail "vulkan-real-gpu" "ICD selected llvmpipe (software/CPU) — heartbeat requires real GPU. Check VK_ICD_FILENAMES."
      else
        pass "vulkan-real-gpu (not llvmpipe — passthrough to real GPU)"
      fi
    else
      fail "vulkan-device" "vulkaninfo enumerated zero devices — ICD loader broken in container"
    fi
    ;;
esac

# ── Summary ─────────────────────────────────────────────────────────
echo ""
if [[ ${#FAILS[@]} -eq 0 ]]; then
  echo "━━━ heartbeat $TAG ($HEARTBEAT_VARIANT): GREEN ━━━"
  echo "    full stack served a probe, inference fired, GPU engaged."
  exit 0
else
  echo "━━━ heartbeat $TAG ($HEARTBEAT_VARIANT): ${#FAILS[@]} FAILURE(S) ━━━" >&2
  for f in "${FAILS[@]}"; do
    echo "  - $f" >&2
  done
  exit 2
fi
