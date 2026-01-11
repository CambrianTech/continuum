#!/bin/bash
# Setup Rust toolchain and build all workers
# Run this on new machines to set up the Rust infrastructure
set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}🦀 Rust Setup for JTAG Workers${NC}"
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
# Step 2: Check/Install jq (required for worker scripts)
# ============================================================================

echo -e "${YELLOW}2. Checking jq installation...${NC}"

if command -v jq &> /dev/null; then
  echo -e "   ${GREEN}✅ jq installed${NC}"
else
  echo -e "   ${YELLOW}⚠️  jq not found. Installing...${NC}"

  if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    if command -v brew &> /dev/null; then
      brew install jq
    else
      echo -e "   ${RED}❌ Homebrew not found. Install jq manually: brew install jq${NC}"
      exit 1
    fi
  elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    if command -v apt-get &> /dev/null; then
      sudo apt-get update && sudo apt-get install -y jq
    elif command -v yum &> /dev/null; then
      sudo yum install -y jq
    else
      echo -e "   ${RED}❌ Package manager not found. Install jq manually.${NC}"
      exit 1
    fi
  fi

  echo -e "   ${GREEN}✅ jq installed${NC}"
fi

# ============================================================================
# Step 3: Build all Rust workers
# ============================================================================

echo -e "${YELLOW}3. Building Rust workers...${NC}"

# Get script directory and navigate to jtag root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JTAG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$JTAG_ROOT"

CONFIG_FILE="workers/workers-config.json"

if [ ! -f "$CONFIG_FILE" ]; then
  echo -e "   ${RED}❌ Worker config not found: $CONFIG_FILE${NC}"
  exit 1
fi

# Count enabled workers
ENABLED_COUNT=$(jq '[.workers[] | select(.enabled != false)] | length' "$CONFIG_FILE")
echo -e "   Found ${ENABLED_COUNT} enabled workers"

# Build each enabled worker
BUILT=0
jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE" | while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  binary=$(echo "$worker" | jq -r '.binary')
  build_dir=$(dirname "$binary" | sed 's|/target/.*||')

  echo -e "   ${YELLOW}Building ${name}-worker...${NC}"

  if [ -d "$build_dir" ]; then
    (cd "$build_dir" && cargo build --release 2>&1 | tail -3)

    # Verify binary was created
    if [ -f "$binary" ]; then
      SIZE=$(du -h "$binary" | cut -f1)
      echo -e "   ${GREEN}✅ ${name}-worker built ($SIZE)${NC}"
    else
      echo -e "   ${RED}❌ ${name}-worker build failed${NC}"
    fi
  else
    echo -e "   ${RED}❌ Build directory not found: $build_dir${NC}"
  fi
done

# ============================================================================
# Step 4: Verify builds
# ============================================================================

echo -e "${YELLOW}4. Verifying builds...${NC}"

ALL_GOOD=true
jq -c '.workers[] | select(.enabled != false)' "$CONFIG_FILE" | while read -r worker; do
  name=$(echo "$worker" | jq -r '.name')
  binary=$(echo "$worker" | jq -r '.binary')

  if [ -f "$binary" ]; then
    echo -e "   ${GREEN}✅ ${name}-worker${NC}"
  else
    echo -e "   ${RED}❌ ${name}-worker missing${NC}"
    ALL_GOOD=false
  fi
done

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${BLUE}=================================${NC}"
echo -e "${GREEN}🎉 Rust setup complete!${NC}"
echo ""
echo -e "Next steps:"
echo -e "  ${YELLOW}npm run worker:start${NC}  - Start all workers"
echo -e "  ${YELLOW}npm run worker:status${NC} - Check worker status"
echo -e "  ${YELLOW}npm start${NC}             - Start full system (includes workers)"
echo ""
