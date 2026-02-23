#!/bin/bash
# Parallel Start — Runs TypeScript and Rust builds concurrently, then starts the system.
# This is the main entry point for `npm start`.

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

# Phase 1: Stop any existing system (fast — ~2s)
echo -e "${YELLOW}Phase 1: Cleanup${NC}"
bash scripts/system-stop.sh

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

# Phase 4: Launch system (server + browser detection)
echo -e "\n${YELLOW}Phase 4: Launch system${NC}"
npx tsx scripts/launch-active-example.ts

END_TIME=$(date +%s)
TOTAL_ELAPSED=$((END_TIME - START_TIME))
echo -e "\n${GREEN}🎉 Total startup time: ${TOTAL_ELAPSED}s${NC}"
