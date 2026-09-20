#!/bin/bash
# Parallel Start — Runs TypeScript and Rust builds concurrently, then starts the system.
# This is the main entry point for `npm start`.
#
# Two modes:
#   HOT RESTART: System already running (browser connected). Builds new code,
#                kills only the orchestrator, restarts. Browser reconnects and
#                gets refreshed — no new window.
#   COLD START:  No system running. Full cleanup, build, launch fresh.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

source "$SCRIPT_DIR/shared/preflight.sh"

# Ensure cargo is in PATH (rustup installs to ~/.cargo/bin, not always in non-interactive shells)
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
# Raise open file limit (default 1024 on Linux/WSL is too low for rayon + tokio thread pools)
ulimit -n 65536 2>/dev/null || true
# Ensure nvm is in PATH (for Linux/WSL installs via nvm)
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
# Source config.env — API keys, DATABASE_URL, storage paths.
# Must happen before BOTH workers (start-workers.sh) AND the TS orchestrator.
if [ -f "$HOME/.continuum/config.env" ]; then
  set -a; source "$HOME/.continuum/config.env"; set +a
fi
# ONNX Runtime — tell ort crate where to find the shared library.
# Mac: homebrew installs to /opt/homebrew/lib; Linux: user-space install to ~/.continuum/lib
if [ -z "$ORT_DYLIB_PATH" ]; then
  if [ -f "$HOME/.continuum/lib/libonnxruntime.so" ]; then
    export ORT_DYLIB_PATH="$HOME/.continuum/lib/libonnxruntime.so"
  elif [ -f "/opt/homebrew/lib/libonnxruntime.dylib" ]; then
    export ORT_DYLIB_PATH="/opt/homebrew/lib/libonnxruntime.dylib"
  fi
fi

cd "$PROJECT_DIR"

# All data lives at $HOME/.continuum — matches SystemPaths.root in TypeScript.
CONTINUUM_ROOT="${CONTINUUM_ROOT:-$HOME/.continuum}"

# ── Ensure system dependencies are installed (idempotent — skips what exists) ──
# Runs only the dependency-check portion of install.sh. On repeat runs this
# completes in <1s (everything already installed). First run prompts for
# sudo password if system packages are needed.
if [ -f "$SCRIPT_DIR/install.sh" ]; then
  CONTINUUM_DEPS_ONLY=1 bash "$SCRIPT_DIR/install.sh"
fi

echo -e "${YELLOW}🚀 JTAG System Start${NC}"
START_TIME=$(date +%s)

# ── Cross-mode collision detection + port pre-flight ─────────────────
# Continuum has two operating modes that bind the same ports:
#   Dev mode (npm start, this script):   native continuum-core-server +
#                                        native node-server via tsx.
#   Carl mode (docker compose up):       everything containerized
#                                        (continuum-core on Linux; support
#                                        services on Mac Option B).
# Run both at once → port fights on 9001, 9100, 7880-7882, 9003, 5432.
# Both halves end up half-broken with cryptic EADDRINUSE deep in tsx logs.
#
# Three independent signals tell us which mode(s) are active:
MODE_DOCKER=0; MODE_NATIVE_CORE=0; MODE_NATIVE_TSX=0
if command -v docker &>/dev/null && docker info &>/dev/null 2>&1; then
  if docker ps --filter "label=com.docker.compose.project=continuum" \
       --format '{{.ID}}' 2>/dev/null | grep -q .; then
    MODE_DOCKER=1
  fi
fi
pgrep -x 'continuum-core-server' >/dev/null 2>&1 && MODE_NATIVE_CORE=1
pgrep -f 'tsx.*scripts/launch-active-example' >/dev/null 2>&1 && MODE_NATIVE_TSX=1

if [ "$MODE_DOCKER" -eq 1 ]; then
  echo -e "${YELLOW}⚠  Containerized continuum stack is running (Carl mode).${NC}"
  echo -e "${YELLOW}   npm start (Dev mode) will collide on ports 9001/9100/7880-82/9003/5432.${NC}"
  echo -e "${YELLOW}   Run 'npm stop' (kills BOTH modes in one call) and re-try.${NC}"
  echo ""
fi
if [ "$MODE_NATIVE_CORE" -eq 1 ] || [ "$MODE_NATIVE_TSX" -eq 1 ]; then
  echo -e "${YELLOW}⚠  Native continuum processes from a prior npm start are still alive:${NC}"
  [ "$MODE_NATIVE_CORE" -eq 1 ] && echo -e "${YELLOW}     continuum-core-server PID $(pgrep -x continuum-core-server | head -1)${NC}"
  [ "$MODE_NATIVE_TSX" -eq 1 ] && echo -e "${YELLOW}     tsx orchestrator PID $(pgrep -f 'tsx.*scripts/launch-active-example' | head -1)${NC}"
  echo -e "${YELLOW}   They will fight this npm start for their bound ports. Run 'npm stop' first.${NC}"
  echo ""
fi

# Port pre-flight — fail fast, not deep in tsx with cryptic EADDRINUSE.
# Check the two ports npm start ALWAYS binds (WS 9001 + Rust IPC TCP 9100
# if env is set). LiveKit/widget ports only bind if those subsystems
# actually start, so they're less likely to fail at entry.
PORT_HOLDERS=""
NODE_WS_PORT="${NODE_WS_PORT:-9001}"
if lsof -iTCP:"$NODE_WS_PORT" -sTCP:LISTEN 2>/dev/null | grep -q LISTEN; then
  HOLDER_CMD=$(lsof -nP -iTCP:"$NODE_WS_PORT" -sTCP:LISTEN 2>/dev/null | awk 'NR==2 {print $1" (PID "$2")"}')
  PORT_HOLDERS="$PORT_HOLDERS\n     :$NODE_WS_PORT by $HOLDER_CMD"
fi
if [ -n "${CONTINUUM_CORE_TCP:-}" ] && lsof -iTCP:"$CONTINUUM_CORE_TCP" -sTCP:LISTEN 2>/dev/null | grep -q LISTEN; then
  HOLDER_CMD=$(lsof -nP -iTCP:"$CONTINUUM_CORE_TCP" -sTCP:LISTEN 2>/dev/null | awk 'NR==2 {print $1" (PID "$2")"}')
  PORT_HOLDERS="$PORT_HOLDERS\n     :$CONTINUUM_CORE_TCP by $HOLDER_CMD"
fi
if [ -n "$PORT_HOLDERS" ]; then
  echo -e "${RED}✗ Ports needed by npm start are already bound:${NC}"
  echo -e "$PORT_HOLDERS"
  echo -e "${RED}   Refusing to start — you'd hit cryptic EADDRINUSE deep in tsx logs.${NC}"
  echo -e "${RED}   Run 'npm stop' to clear Dev-mode processes (and any Carl-mode stack).${NC}"
  exit 1
fi

# Pre-flight: catch Xcode issues NOW, not buried in build output 30 lines deep
preflight_check_xcode

# Pre-flight: self-heal Tailscale SSH state. If the user has tailscale and
# is authenticated but --ssh got dropped (common after a reboot or a plain
# `tailscale up`), re-add it. Silent no-op if tailscale isn't installed or
# the user opted out via CONTINUUM_NO_TAILSCALE_PREFLIGHT=1.
preflight_check_tailscale_ssh

# Phase 1: Detect existing system state
# If the system is already running, we do a HOT RESTART:
#   - Don't nuke everything (browser stays alive)
#   - Build new code
#   - Kill only the orchestrator (not ports, not sockets, not browser)
#   - Restart orchestrator — it detects the browser and refreshes it
# If the system is NOT running, we do a COLD START:
#   - Nuclear cleanup (system-stop.sh)
#   - Build, start everything fresh, open new browser

HOT_RESTART=false
if [ -f $CONTINUUM_ROOT/jtag/logs/system/npm-start.pid ]; then
  OLD_PID=$(cat $CONTINUUM_ROOT/jtag/logs/system/npm-start.pid)
  if kill -0 "$OLD_PID" 2>/dev/null; then
    HOT_RESTART=true
    echo -e "${GREEN}Phase 1: System running (PID $OLD_PID) — hot restart (browser preserved)${NC}"
    # Kill the known orchestrator PID — leave browser, sockets, ports alone.
    # The browser tab keeps its WebSocket open to the old server until it dies,
    # then reconnects when the new server starts on the same port.
    kill "$OLD_PID" 2>/dev/null || true
    # Also kill any other orchestrator patterns (safety — catches orphaned processes)
    for pattern in "launch-active-example" "minimal-server" "launch-and-capture"; do
      while IFS= read -r pid; do
        if [ -n "$pid" ] && [ "$pid" != "$$" ] && [ "$pid" != "$PPID" ]; then
          kill "$pid" 2>/dev/null || true
        fi
      done < <(pgrep -f "$pattern" 2>/dev/null || true)
    done
    # Brief pause for orchestrator to die — NOT the full nuclear 2s sleep
    sleep 1
  fi
fi

if [ "$HOT_RESTART" = false ]; then
  echo -e "${YELLOW}Phase 1: No system running — cold start${NC}"
  bash scripts/system-stop.sh
fi

# Version bump removed from npm start — bumps belong in npm run pack (release builds only)
# This was dirtying package.json + version.ts on every deploy, polluting git status

# Phase 2: Build pipeline — Rust first (exclusive cargo access), then TS + VRM in parallel
#
# Why not fully parallel? cargo test (ts-rs binding gen) and cargo build --release
# both target the same target/release directory. Running them concurrently causes
# cache invalidation → full recompilation → 300s timeout. Sequential Rust build
# followed by parallel TS+VRM is actually FASTER than the broken parallel approach.
echo -e "\n${YELLOW}Phase 2a: Rust build + voice models${NC}"

# Model downloads + scene generation in parallel with cargo build
# All downloads are NON-FATAL — system starts without them, downloads continue
(
  # Avatar models (VRM) — required for 3D live mode
  ./scripts/download-avatar-models.sh 2>&1 | sed 's/^/  [Avatars] /' || {
    echo -e "  [Avatars] ${YELLOW}⚠️ Avatar model download failed — live mode will have no avatars${NC}"
  }
  # Voice models (TTS/STT)
  ./scripts/download-voice-models.sh 2>&1 | sed 's/^/  [Voice] /' || {
    echo -e "  [Voice] ${YELLOW}⚠️ Voice model download failed — system will start without STT/TTS${NC}"
  }
  # Generate scene environment GLBs (idempotent — skips existing)
  npx tsx scripts/generate-scene-models.ts 2>&1 | sed 's/^/  [Scenes] /' || {
    echo -e "  [Scenes] ${YELLOW}⚠️ Scene generation failed — non-fatal${NC}"
  }
) &
MODELS_PID=$!

# Rust: cargo build (exclusive access to target directory)
# GPU features selected by platform: Metal (macOS), CUDA (Linux+NVIDIA), CPU-only (fallback).
cd workers
source "$SCRIPT_DIR/shared/cargo-features.sh"
echo -e "  [Rust] Building workers (cargo incremental)... ${CARGO_GPU_FEATURES:-[cpu-only]}"
# Build GPU-aware crates individually (workspace --features only applies to focused package).
# Non-GPU crates (archive, jtag-mcp) build normally.
GPU_FEAT="${CARGO_GPU_FEATURES#--features }"  # "metal,accelerate" or "cuda" or ""
# GPU backend feature only (no accelerate or load-dynamic-ort — those are continuum-core only)
GPU_BACKEND=$(echo "$GPU_FEAT" | sed 's/,accelerate//;s/accelerate,//;s/accelerate//' | sed 's/,load-dynamic-ort//;s/load-dynamic-ort,//;s/load-dynamic-ort//')
BUILD_OUTPUT=""
RESULT=0
# First run detection: if no release binary exists, this is a long build — show progress
CARGO_QUIET="--quiet"
if [ ! -f "target/release/continuum-core-server" ]; then
  echo -e "  [Rust] ${YELLOW}First build detected — this takes 5-15 minutes. Showing progress...${NC}"
  CARGO_QUIET=""
fi

# Wrapper around `cargo build -p <pkg>`. On incremental builds (CARGO_QUIET
# non-empty) we capture-then-display, which keeps the log clean. On first
# builds (CARGO_QUIET empty) we tee so cargo's "Compiling crate vX.Y.Z"
# lines stream live to the terminal — without this, the user saw the
# "First build detected — Showing progress..." banner then total silence
# for 5-15 minutes because $(cargo ...) blocks until cargo exits. We still
# capture into $OUT for preflight_check_cargo_xcode + the failure path.
build_pkg() {
  local pkg="$1"; shift
  if [ -n "$CARGO_QUIET" ]; then
    OUT=$(cargo build --release -p "$pkg" "$@" --quiet 2>&1) \
      || { BUILD_OUTPUT+="$OUT"; RESULT=1; }
  else
    local tmp
    tmp=$(mktemp)
    cargo build --release -p "$pkg" "$@" 2>&1 | tee "$tmp"
    local rc=${PIPESTATUS[0]}
    OUT=$(cat "$tmp")
    rm -f "$tmp"
    if [ "$rc" -ne 0 ]; then
      BUILD_OUTPUT+="$OUT"
      RESULT=1
    fi
  fi
}

for pkg in archive-worker jtag-mcp; do
  build_pkg "$pkg"
done
# continuum-core: all GPU features (metal+accelerate on macOS, cuda on Linux)
if [ -n "$GPU_FEAT" ]; then
  build_pkg continuum-core --features "$GPU_FEAT"
else
  build_pkg continuum-core
fi
# inference-grpc: GPU backend only (metal or cuda, no accelerate)
if [ -n "$GPU_BACKEND" ]; then
  build_pkg inference-grpc --features "$GPU_BACKEND"
else
  build_pkg inference-grpc
fi
# Filter ts-rs noise and display
echo "$BUILD_OUTPUT" | grep -v -E "ts-rs failed to parse|failed to parse serde|= note:|skip_serializing_if|^\s*\|?\s*$|^$" | sed 's/^/  [Rust] /'
if [ $RESULT -eq 0 ]; then
  echo -e "  [Rust] ${GREEN}✅ Build complete${NC}"
else
  # Detect specific failures with actionable messages
  if preflight_check_cargo_xcode "$BUILD_OUTPUT"; then
    : # Xcode issue detected — message already printed
  else
    echo -e "  [Rust] ${RED}❌ Build failed${NC}"
  fi
  exit $RESULT
fi
cd ..

# Wait for voice models before VRM conversion
wait $MODELS_PID || true

RUST_TIME=$(date +%s)
RUST_ELAPSED=$((RUST_TIME - START_TIME))
echo -e "${GREEN}✅ Rust build complete (${RUST_ELAPSED}s)${NC}"

# Phase 2b: TS build + VRM conversion in parallel
# Now cargo is idle — binding generation (cargo test --release) hits warm cache (~4s)
echo -e "\n${YELLOW}Phase 2b: TypeScript build + VRM conversion${NC}"

# TS build: prebuild (includes binding gen on warm cache) + compile + bundle
(
  npx tsx scripts/smart-build.ts 2>&1 | sed 's/^/  [TS] /'
  exit ${PIPESTATUS[0]}
) &
TS_PID=$!

# VRM conversion (uses binary from cargo build, no cargo contention)
(
  CONVERTER="workers/target/release/vrm-convert-textures"
  if [ -x "$CONVERTER" ]; then
    CONVERTED=0
    for vrm in models/avatars/*.vrm; do
      [ -f "$vrm" ] || continue
      NEEDS_WORK=false
      if grep -q 'KHR_texture_basisu' "$vrm" 2>/dev/null; then
        NEEDS_WORK=true
      elif grep -q 'VRMC_vrm' "$vrm" 2>/dev/null; then
        NEEDS_WORK=true
      fi
      if [ "$NEEDS_WORK" = true ]; then
        echo -e "  [VRM] Normalizing: $(basename $vrm)"
        $CONVERTER "$vrm" 2>&1 | sed 's/^/  [VRM]   /'
        CONVERTED=$((CONVERTED + 1))
      fi
    done
    if [ "$CONVERTED" -gt 0 ]; then
      echo -e "  [VRM] ${GREEN}✅ Normalized $CONVERTED VRM models${NC}"
    fi
  fi
) &
VRM_PID=$!

# Wait for TS + VRM
TS_OK=true
VRM_OK=true

wait $TS_PID || TS_OK=false
wait $VRM_PID || VRM_OK=true  # VRM failure is non-fatal

if [ "$TS_OK" = false ]; then
  echo -e "${RED}❌ TypeScript build failed${NC}"
  exit 1
fi

BUILD_TIME=$(date +%s)
BUILD_ELAPSED=$((BUILD_TIME - START_TIME))
echo -e "${GREEN}✅ All builds complete (${BUILD_ELAPSED}s)${NC}"

# Phase 2.5: Database health check — ensure Postgres `continuum` database exists
# After OOM crashes or system failures, Postgres can lose the database entirely.
# Detect and auto-recover rather than failing cryptically during data daemon init.
echo -e "\n${YELLOW}Phase 2.5: Database health check${NC}"

# Find psql binary (Homebrew PostgreSQL 17, then PATH)
PSQL=""
for candidate in /opt/homebrew/opt/postgresql@17/bin/psql /usr/local/opt/postgresql@17/bin/psql; do
  if [ -x "$candidate" ]; then
    PSQL="$candidate"
    break
  fi
done
if [ -z "$PSQL" ]; then
  PSQL=$(command -v psql 2>/dev/null || true)
fi

if [ -n "$PSQL" ]; then
  CREATEDB="$(dirname "$PSQL")/createdb"

  # Check if Postgres is accepting connections (start it if not)
  if ! "$PSQL" -h localhost -p 5432 -U "${USER}" -c "SELECT 1" postgres >/dev/null 2>&1; then
    echo -e "  ${YELLOW}⚠️ Postgres not responding — attempting to start...${NC}"
    if command -v brew >/dev/null 2>&1; then
      # macOS: Homebrew service
      brew services start postgresql@17 2>/dev/null || true
    elif sudo -n true 2>/dev/null; then
      # Linux/WSL2: passwordless sudo available
      sudo -n service postgresql start 2>/dev/null || true
    else
      # Can't start without password — tell user
      echo -e "  ${RED}Postgres is down. Run: sudo service postgresql start${NC}"
    fi
    sleep 2
  fi
  if "$PSQL" -h localhost -p 5432 -U "${USER}" -c "SELECT 1" postgres >/dev/null 2>&1; then
    # Check if continuum database exists
    if "$PSQL" -h localhost -p 5432 -U "${USER}" -lqt 2>/dev/null | grep -qw continuum; then
      echo -e "  ${GREEN}✅ Database 'continuum' exists${NC}"
    else
      echo -e "  ${YELLOW}⚠️ Database 'continuum' missing — creating...${NC}"
      if "$CREATEDB" -h localhost -p 5432 -U "${USER}" continuum 2>&1; then
        echo -e "  ${GREEN}✅ Database 'continuum' created${NC}"
        # Flag that we need to seed after system is healthy
        NEEDS_SEED=true
      else
        echo -e "  ${RED}❌ Failed to create database 'continuum' — data daemon may fail${NC}"
      fi
    fi
  else
    echo -e "  ${YELLOW}⚠️ Postgres not responding on localhost:5432 — skipping DB check${NC}"
  fi
else
  echo -e "  ${YELLOW}⚠️ psql not found — skipping DB check${NC}"
fi

# Phase 3: Start workers (skip build — already done above)
echo -e "\n${YELLOW}Phase 3: Start workers${NC}"
bash workers/start-workers.sh --skip-build

# Phase 4: Launch system (server + browser) as background daemon
echo -e "\n${YELLOW}Phase 4: Launch system${NC}"

# Ensure log directory exists
mkdir -p "$CONTINUUM_ROOT/jtag/logs/system"
STARTUP_AUTONOMOUS_PAUSE="$CONTINUUM_ROOT/jtag/startup-autonomous-work.paused"
echo "$$" > "$STARTUP_AUTONOMOUS_PAUSE"
cleanup_startup_pause() {
  rm -f "$STARTUP_AUTONOMOUS_PAUSE"
}
trap cleanup_startup_pause EXIT

# Start the orchestrator as a daemon — it runs forever (WebSocket server is in-process).
# Use the project-local tsx binary directly; `npx` is a short-lived wrapper and
# has caused false "daemon" starts where the launcher dies after npm start exits.
# Redirect stdin as well as output so parent shell/PTY teardown cannot touch it.
# system-stop.sh finds it by pattern "launch-active-example".
# Browser attachment happens after seed below. Starting the orchestrator with
# browser management enabled lets stale tabs reconnect during seed and trigger
# persona/RAG/model work while the database is still being synchronized.
TSX_BIN="$PROJECT_DIR/node_modules/.bin/tsx"
LAUNCH_PID=$(node "$PROJECT_DIR/scripts/spawn-detached.mjs" \
  --cwd "$PROJECT_DIR" \
  --log "$CONTINUUM_ROOT/jtag/logs/system/orchestrator.log" \
  --env CONTINUUM_DEFER_BROWSER=1 \
  -- "$TSX_BIN" scripts/launch-active-example.ts)
echo "$LAUNCH_PID" > $CONTINUUM_ROOT/jtag/logs/system/npm-start.pid
echo -e "  Orchestrator started (PID $LAUNCH_PID, log: $CONTINUUM_ROOT/jtag/logs/system/orchestrator.log)"

# Phase 5: Wait for system health (poll ./jtag ping)
# On HOT RESTART, the orchestrator's detectAndManageBrowser() will find the
# existing browser tab (it reconnects to the new server) and refresh it.
# On COLD START, the orchestrator opens a new browser tab.
# We wait for BROWSER connection, not just server — so the orchestrator has
# time to complete its browser detection before we declare victory.
echo -e "\n${YELLOW}Phase 5: Waiting for system health...${NC}"
MAX_WAIT=120
ELAPSED=0
HEALTHY=false

while [ $ELAPSED -lt $MAX_WAIT ]; do
  sleep 3
  ELAPSED=$((ELAPSED + 3))

  # Check if daemon process is still alive
  if ! kill -0 $LAUNCH_PID 2>/dev/null; then
    echo -e "${RED}❌ Orchestrator process died (check $CONTINUUM_ROOT/jtag/logs/system/orchestrator.log)${NC}"
    exit 1
  fi

  # Try ping — server success is sufficient (browser may not exist in headless/CLI mode).
  PING_OUTPUT=$(./jtag ping --timeout=5000 2>/dev/null || echo '{}')
  if echo "$PING_OUTPUT" | grep -q '"success"' 2>/dev/null; then
    HEALTHY=true
    if echo "$PING_OUTPUT" | grep -q '"browser"' 2>/dev/null; then
      echo -e "  ${GREEN}Server + browser connected${NC}"
    else
      echo -e "  ${GREEN}Server up (no browser — headless/CLI mode OK)${NC}"
    fi
    break
  else
    echo -e "  ⏳ Waiting for server... (${ELAPSED}s / ${MAX_WAIT}s)"
  fi
done

if [ "$HEALTHY" = false ]; then
  if echo "$PING_OUTPUT" | grep -q '"success"' 2>/dev/null; then
    HEALTHY=true
    echo -e "  ${GREEN}Server up (no browser — headless/CLI mode OK)${NC}"
  else
    echo -e "${RED}❌ System did not become healthy within ${MAX_WAIT}s${NC}"
    echo -e "${RED}   Check log: $CONTINUUM_ROOT/jtag/logs/system/orchestrator.log${NC}"
    exit 1
  fi
fi

# Phase 5.5: Seed database BEFORE browser connects
# Critical: Browser must connect AFTER seeding so findSeededHumanOwner() finds Joel.
# Without this, browser connects → anonymous user created → wrong userId in session.
echo -e "\n${YELLOW}Phase 5.5: Ensuring database is seeded...${NC}"
# Capture data:seed's exit code via PIPESTATUS — without this the pipe
# to sed always succeeds and we'd print "✅ Seed complete" even after
# seed failed (#980 Bug 3, observed live on M1 Carl pass: seed timed
# out at 480s, then this script printed "✅ Seed complete" + "🎉 System
# is UP!" anyway, then chat went silent because no personas existed).
# Same PIPESTATUS pattern as the TS build subshell at ~line 278.
npm run data:seed 2>&1 | sed 's/^/  [Seed] /'
SEED_RC=${PIPESTATUS[0]}
SEED_OK=true
if [ "$SEED_RC" -ne 0 ]; then
  SEED_OK=false
  echo -e "  ${RED}❌ Seeding failed (exit $SEED_RC) — first chat will likely have no AI responder.${NC}"
  echo -e "  ${YELLOW}   Common cause: continuum-core didn't register commands within the seed${NC}"
  echo -e "  ${YELLOW}   wait window (480s). Check orchestrator + core logs for SIGABRT / crash:${NC}"
  echo -e "  ${YELLOW}     tail -100 \$HOME/.continuum/jtag/logs/system/orchestrator.log${NC}"
  echo -e "  ${YELLOW}     tail -100 \$HOME/.continuum/jtag/logs/system/continuum-core.log${NC}"
  echo -e "  ${YELLOW}   System will still start, but chat won't have personas. Re-seed after fixing:${NC}"
  echo -e "  ${YELLOW}     npm run data:seed${NC}"
  # Don't exit here — system may still be partially usable + user can
  # re-seed once they've fixed the underlying core failure. But the
  # final "System is UP" banner below tells the truth (degraded vs ok).
else
  echo -e "  ${GREEN}✅ Seed complete${NC}"
fi
cleanup_startup_pause

<<<<<<<< HEAD:legacy/node-startup/parallel-start.sh
# Phase 6: HEADLESS IS THE DEFAULT. `npm start` brings up the substrate (core +
# server) and NEVER launches a desktop — servers (AWS, BigMama, grid nodes) run
# headless, which is the first-class operating mode. The desktop is a DEPENDENT
# overlay that attaches LATER, deliberately, through its one owner — the Rust
# `LaunchModeModule` (`system/launch-mode/{get,set}`) driven by the menu-bar
# tray / a setting / a prompt. This launcher does NOT read or write the
# `CONTINUUM_LAUNCH_MODE` setting in config.env (one class owns that file — not
# bash scripts cutting through it), and it does NOT open a browser window.
#
# The one thing it still does: if a human ALREADY has a UI tab connected, refresh
# it post-seed so it doesn't render against a half-seeded DB. That's refreshing an
# existing overlay the human chose to open — not launching the desktop.
BROWSER_CONNECTED=false
if [ "$SEED_OK" = true ]; then
  PING_OUTPUT=$(./jtag ping --timeout=5000 2>/dev/null || echo '{}')
  if echo "$PING_OUTPUT" | grep -q '"browser"' 2>/dev/null; then
    echo -e "  ${YELLOW}Refreshing already-connected UI overlay after seed...${NC}"
    if ./jtag interface/navigate >/dev/null 2>&1; then
      BROWSER_CONNECTED=true
      echo -e "  ${GREEN}UI overlay refreshed after seed${NC}"
    else
      ./jtag development/exec --code="location.reload()" >/dev/null 2>&1 || true
    fi
  else
    echo -e "  ${GREEN}Headless — no desktop launched (attach a UI via the tray / launch-mode setting)${NC}"
========
# Phase 6: Browser attach happens only after seed. This script owns the final
# post-seed refresh/open so the orchestrator cannot race UI hydration against
# database synchronization.
BROWSER_CONNECTED=false
if [ "$SEED_OK" = true ]; then
  echo -e "  ${YELLOW}Attaching browser after seed...${NC}"
  PING_OUTPUT=$(./jtag ping --timeout=5000 2>/dev/null || echo '{}')
  if echo "$PING_OUTPUT" | grep -q '"browser"' 2>/dev/null; then
    if ./jtag interface/navigate >/dev/null 2>&1; then
      BROWSER_CONNECTED=true
      echo -e "  ${GREEN}Browser refreshed after seed${NC}"
    else
      ./jtag development/exec --code="location.reload()" >/dev/null 2>&1 || true
    fi
  elif command -v open >/dev/null 2>&1; then
    open "http://localhost:9000/chat/general" >/dev/null 2>&1 || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "http://localhost:9000/chat/general" >/dev/null 2>&1 || true
>>>>>>>> origin/main:tools/scripts/parallel-start.sh
  fi
fi
if [ "$HOT_RESTART" = true ]; then
  # Hot restart: give existing tab time to reconnect via WebSocket
  echo -e "  ⏳ Waiting for browser to reconnect..."
  for i in 1 2 3 4 5; do
    sleep 3
    PING_OUTPUT=$(./jtag ping --timeout=5000 2>/dev/null || echo '{}')
    if echo "$PING_OUTPUT" | grep -q '"browser"' 2>/dev/null; then
      BROWSER_CONNECTED=true
      echo -e "  ${GREEN}Browser reconnected${NC}"
      break
    fi
  done
fi

END_TIME=$(date +%s)
TOTAL_ELAPSED=$((END_TIME - START_TIME))
# Banner reflects the truth: if seed failed, system is DEGRADED (no
# personas, chat silent). Per Joel's silent-success-is-failure rule
# we don't print 🎉 over a known-broken state. #980 Bug 3.
if [ "$SEED_OK" != true ]; then
  echo -e "\n${RED}⚠️  System started in DEGRADED mode (${TOTAL_ELAPSED}s) — seed failed, chat will not have personas.${NC}"
  echo -e "${YELLOW}   See seeding error above + log paths for diagnosis.${NC}"
elif [ "$HOT_RESTART" = true ] && [ "$BROWSER_CONNECTED" = true ]; then
  echo -e "\n${GREEN}🎉 Hot restart complete! (${TOTAL_ELAPSED}s) — browser refreshed${NC}"
elif [ "$HOT_RESTART" = true ]; then
  echo -e "\n${GREEN}🎉 Hot restart complete! (${TOTAL_ELAPSED}s)${NC}"
else
  echo -e "\n${GREEN}🎉 System is UP! Total startup time: ${TOTAL_ELAPSED}s${NC}"
fi
echo -e "${GREEN}   Orchestrator running as daemon (PID $LAUNCH_PID)${NC}"
echo -e "${GREEN}   Stop with: npm stop${NC}"
