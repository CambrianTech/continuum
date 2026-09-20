#!/bin/bash
# Setup Rust toolchain and build all workers
# Run this on new machines to set up the Rust infrastructure
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/shared/preflight.sh"

echo -e "${BLUE}🦀 Rust build prerequisites for continuum-core${NC}"
echo -e "================================="
echo ""

# ============================================================================
# Step 1: Check/Install Rust
# ============================================================================

echo -e "${YELLOW}1. Checking Rust installation...${NC}"

if command -v rustc &> /dev/null; then
  RUST_VERSION=$(rustc --version)
  echo -e "   ${GREEN}✅ Rust installed: $RUST_VERSION${NC}"
else
  echo -e "   ${YELLOW}⚠️  Rust not found. Installing via rustup...${NC}"

  # Install rustup (the Rust toolchain installer)
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

  # Source cargo env for this session
  source "$HOME/.cargo/env"

  echo -e "   ${GREEN}✅ Rust installed: $(rustc --version)${NC}"
fi

# Ensure cargo is in path for this session
export PATH="$HOME/.cargo/bin:$PATH"

# ============================================================================
# Step 2: Check build tools (Xcode on macOS, gcc/make on Linux)
# ============================================================================

echo -e "${YELLOW}2. Checking build tools...${NC}"
preflight_check_build_tools
echo -e "   ${GREEN}✅ Build tools OK${NC}"

# ============================================================================
# Step 3: Check/Install jq (required for worker scripts)
# ============================================================================

echo -e "${YELLOW}3. Checking jq installation...${NC}"

if command -v jq &>/dev/null; then
  echo -e "   ${GREEN}✅ jq installed${NC}"
else
  echo -e "   ${YELLOW}⚠️  jq not found. Installing...${NC}"
  preflight_pkg_install jq
  echo -e "   ${GREEN}✅ jq installed${NC}"
fi

# ============================================================================
# Step 3.5: Native build prereqs — cmake + vendored git submodules
# ============================================================================
# continuum-core compiles vendored llama.cpp/whisper.cpp via cmake. On a fresh
# clone the submodules are empty (no CMakeLists.txt) and cmake may be absent —
# either one makes `cargo build` (and therefore `npm start`) fail. Ensure both.

echo -e "${YELLOW}3.5 Checking cmake + vendored submodules...${NC}"

if command -v cmake &>/dev/null; then
  echo -e "   ${GREEN}✅ cmake installed${NC}"
else
  echo -e "   ${YELLOW}⚠️  cmake not found. Installing...${NC}"
  preflight_pkg_install cmake
  echo -e "   ${GREEN}✅ cmake installed${NC}"
fi

# `git submodule update` is a no-op if already initialized; git resolves the
# repo root regardless of cwd, so this is safe to run from tools/scripts.
echo -e "   ${YELLOW}Initializing vendored submodules (llama.cpp, whisper.cpp)...${NC}"
git submodule update --init --recursive
echo -e "   ${GREEN}✅ Vendored submodules ready${NC}"

# ============================================================================
# The Rust WORKERS build used to live here (steps 4-5) and was removed.
#
# It read `workers/workers-config.json` and built each binary under
# `workers/target/release/`. That subsystem was consolidated into
# `continuum-core`: neither `workers/` nor `core/workers/` exists, so the step
# could only ever `exit 1` with "Worker config not found" — which it did on
# EVERY run of the README-documented `npm run setup:rust` (README.md:134).
#
# The prerequisite work above (rust toolchain, build tools, jq, cmake, vendored
# submodules) is what README.md:129 actually promises this script does, and it
# succeeded before dying on the dead step. Removed rather than repointed: there
# is no worker layout left to point at. `npm start` builds continuum-core.
#
# Sibling stale references to the same vanished layout (task #80): 
#   tools/scripts/system-stop.sh:11, core/stop-workers.sh:2,
#   tools/generator/generate-worker-registry.ts:112.
# ============================================================================

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}=================================${NC}"
echo -e "${GREEN}🎉 Rust setup complete!${NC}"
echo ""
# `npm run worker:start` / `worker:status` used to be advertised here. Neither
# script exists in package.json — the last thing this script told a new user to
# do was run two commands that fail. `npm start` (package.json:18) is the real
# next step and builds continuum-core natively.
echo -e "Next steps:"
echo -e "  ${YELLOW}npm start${NC}  - build + start the system (continuum-core, native)"
echo ""
