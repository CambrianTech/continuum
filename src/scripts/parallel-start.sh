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

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

echo -e "${YELLOW}🚀 JTAG System Start${NC}"
START_TIME=$(date +%s)

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
if ./jtag ping --timeout=3000 >/dev/null 2>&1; then
  HOT_RESTART=true
  echo -e "${GREEN}Phase 1: System already running — hot restart (browser preserved)${NC}"
  # Kill ONLY the orchestrator process — leave browser, sockets, ports alone.
  # The browser tab keeps its WebSocket open to the old server until it dies,
  # then reconnects when the new server starts on the same port.
  for pattern in "launch-active-example" "minimal-server" "launch-and-capture"; do
    while IFS= read -r pid; do
      if [ -n "$pid" ] && [ "$pid" != "$$" ] && [ "$pid" != "$PPID" ]; then
        echo -e "  Killing orchestrator ($pattern PID $pid)"
        kill "$pid" 2>/dev/null || true
      fi
    done < <(pgrep -f "$pattern" 2>/dev/null || true)
  done
  # Brief pause for orchestrator to die — NOT the full nuclear 2s sleep
  sleep 1
else
  echo -e "${YELLOW}Phase 1: No system running — cold start${NC}"
  bash scripts/system-stop.sh
fi

# Version bump (once, before builds — not inside prebuild where it triggers cascading rebuilds)
npm version patch --no-git-tag-version --silent 2>/dev/null || true

# Phase 2: Parallel builds — TypeScript and Rust are independent
echo -e "\n${YELLOW}Phase 2: Parallel build (TypeScript + Rust + voice models)${NC}"

# Run TypeScript build in background
(
  npx tsx scripts/smart-build.ts 2>&1 | sed 's/^/  [TS] /'
  exit ${PIPESTATUS[0]}
) &
TS_PID=$!

# Run Rust build + voice model check in background (both are Rust/binary work)
(
  # Voice models: fast check, downloads only if missing
  ./scripts/download-voice-models.sh 2>&1 | sed 's/^/  [Models] /'

  # Rust: cargo handles incremental builds — only recompiles what changed
  # Suppress ts-rs serde parse warnings (harmless, just noisy)
  cd workers
  echo -e "  [Rust] Building workers (cargo incremental)..."
  cargo build --release --quiet 2>&1 | grep -v -E "ts-rs failed to parse|failed to parse serde|= note:|skip_serializing_if|^\s*\|?\s*$|^$" | sed 's/^/  [Rust] /'
  RESULT=${PIPESTATUS[0]}
  if [ $RESULT -eq 0 ]; then
    echo -e "  [Rust] ${GREEN}✅ Build complete${NC}"
  else
    echo -e "  [Rust] ${RED}❌ Build failed${NC}"
    exit $RESULT
  fi

  # Post-build: Normalize VRM models for Bevy compatibility
  # The converter handles two fixes (both idempotent):
  #   1. KHR_texture_basisu → move texture sources to standard field
  #   2. VRM 1.0 orientation → rotate skeleton root 180° Y to face -Z
  CONVERTER="target/release/vrm-convert-textures"
  if [ -x "$CONVERTER" ]; then
    cd ..  # Back to src/
    CONVERTED=0
    for vrm in models/avatars/*.vrm; do
      [ -f "$vrm" ] || continue
      # Quick check: does this VRM need conversion?
      # Run converter on VRM 1.0 files (VRMC_vrm) or files with KHR_texture_basisu.
      # The converter itself is idempotent — it detects when no work is needed.
      NEEDS_WORK=false
      if grep -q 'KHR_texture_basisu' "$vrm" 2>/dev/null; then
        NEEDS_WORK=true
      elif grep -q 'VRMC_vrm' "$vrm" 2>/dev/null; then
        # VRM 1.0 — might need orientation fix. Converter checks internally.
        NEEDS_WORK=true
      fi
      if [ "$NEEDS_WORK" = true ]; then
        echo -e "  [VRM] Normalizing: $(basename $vrm)"
        workers/$CONVERTER "$vrm" 2>&1 | sed 's/^/  [VRM]   /'
        CONVERTED=$((CONVERTED + 1))
      fi
    done
    if [ "$CONVERTED" -gt 0 ]; then
      echo -e "  [VRM] ${GREEN}✅ Normalized $CONVERTED VRM models${NC}"
    fi
  fi
) &
RUST_PID=$!

# Wait for both builds
TS_OK=true
RUST_OK=true

wait $TS_PID || TS_OK=false
wait $RUST_PID || RUST_OK=false

if [ "$TS_OK" = false ]; then
  echo -e "${RED}❌ TypeScript build failed${NC}"
  exit 1
fi

if [ "$RUST_OK" = false ]; then
  echo -e "${RED}❌ Rust build failed${NC}"
  exit 1
fi

BUILD_TIME=$(date +%s)
BUILD_ELAPSED=$((BUILD_TIME - START_TIME))
echo -e "${GREEN}✅ All builds complete (${BUILD_ELAPSED}s)${NC}"

# Phase 3: Start workers (skip build — already done above)
echo -e "\n${YELLOW}Phase 3: Start workers${NC}"
bash workers/start-workers.sh --skip-build

# Phase 4: Launch system (server + browser) as background daemon
echo -e "\n${YELLOW}Phase 4: Launch system${NC}"

# Ensure log directory exists
mkdir -p .continuum/jtag/logs/system

# Start the orchestrator as a daemon — it runs forever (WebSocket server is in-process).
# Redirect output to log file. system-stop.sh finds it by pattern "launch-active-example".
nohup npx tsx scripts/launch-active-example.ts \
  >> .continuum/jtag/logs/system/orchestrator.log 2>&1 &
LAUNCH_PID=$!
disown $LAUNCH_PID
echo "$LAUNCH_PID" > .continuum/jtag/logs/system/npm-start.pid
echo -e "  Orchestrator started (PID $LAUNCH_PID, log: .continuum/jtag/logs/system/orchestrator.log)"

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
    echo -e "${RED}❌ Orchestrator process died (check .continuum/jtag/logs/system/orchestrator.log)${NC}"
    exit 1
  fi

  # Try ping — check for BOTH server AND browser connection.
  # This ensures we don't exit before the orchestrator finishes browser detection.
  PING_OUTPUT=$(./jtag ping --timeout=5000 2>/dev/null || echo '{}')
  if echo "$PING_OUTPUT" | grep -q '"success"' 2>/dev/null; then
    HAS_BROWSER=$(echo "$PING_OUTPUT" | grep -c '"browser"' 2>/dev/null || echo "0")
    if [ "$HAS_BROWSER" -gt 0 ]; then
      HEALTHY=true
      break
    else
      echo -e "  ⏳ Server up, waiting for browser... (${ELAPSED}s / ${MAX_WAIT}s)"
    fi
  else
    echo -e "  ⏳ Waiting for server... (${ELAPSED}s / ${MAX_WAIT}s)"
  fi
done

if [ "$HEALTHY" = false ]; then
  # If server is up but browser never connected, still report success
  # (user may have closed the tab intentionally)
  if echo "$PING_OUTPUT" | grep -q '"success"' 2>/dev/null; then
    echo -e "\n${YELLOW}⚠️ Server is UP but no browser detected. Open manually or run: npm start${NC}"
  else
    echo -e "${RED}❌ System did not become healthy within ${MAX_WAIT}s${NC}"
    echo -e "${RED}   Check log: .continuum/jtag/logs/system/orchestrator.log${NC}"
    exit 1
  fi
fi

END_TIME=$(date +%s)
TOTAL_ELAPSED=$((END_TIME - START_TIME))
if [ "$HOT_RESTART" = true ]; then
  echo -e "\n${GREEN}🎉 Hot restart complete! (${TOTAL_ELAPSED}s) — browser refreshed${NC}"
else
  echo -e "\n${GREEN}🎉 System is UP! Total startup time: ${TOTAL_ELAPSED}s${NC}"
fi
echo -e "${GREEN}   Orchestrator running as daemon (PID $LAUNCH_PID)${NC}"
echo -e "${GREEN}   Stop with: npm stop${NC}"
