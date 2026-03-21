#!/bin/bash
# Continuum Bootstrap — One command to install and launch.
#
# Usage (from anywhere):
#   curl -fsSL https://raw.githubusercontent.com/CambrianTech/continuum/main/bootstrap.sh | bash
#
# Or with options:
#   curl -fsSL ... | bash -s -- --mode=headless
#   curl -fsSL ... | bash -s -- --mode=cli
#   curl -fsSL ... | bash -s -- --mode=browser   (default)
#
# What it does:
#   1. Clones the repo (or pulls if already cloned)
#   2. Runs install.sh (installs Node, Rust, Python ML, Postgres, system deps)
#   3. Runs npm start (builds, seeds, creates personas, opens browser)
#
# Works on: macOS (Apple Silicon), Ubuntu/Debian (x86_64), WSL2

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# Parse arguments
MODE="browser"
INSTALL_DIR="${CONTINUUM_DIR:-$HOME/continuum}"

for arg in "$@"; do
  case "$arg" in
    --mode=*) MODE="${arg#*=}" ;;
    --dir=*) INSTALL_DIR="${arg#*=}" ;;
    --help|-h)
      echo "Continuum Bootstrap"
      echo ""
      echo "Usage: curl -fsSL <url> | bash -s -- [options]"
      echo ""
      echo "Options:"
      echo "  --mode=browser    Full web UI (default)"
      echo "  --mode=cli        Cyberpunk terminal interface"
      echo "  --mode=headless   No UI, server only (training towers)"
      echo "  --dir=PATH        Install directory (default: ~/continuum)"
      echo ""
      exit 0
      ;;
  esac
done

# Validate mode
case "$MODE" in
  browser|cli|headless) ;;
  *)
    echo -e "${RED}Unknown mode: $MODE. Use browser, cli, or headless.${NC}"
    exit 1
    ;;
esac

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}  Continuum Bootstrap${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "  Mode:      ${GREEN}${MODE}${NC}"
echo -e "  Directory: ${GREEN}${INSTALL_DIR}${NC}"
echo ""

# ============================================================================
# Step 1: Ensure git is available
# ============================================================================

if ! command -v git &>/dev/null; then
  echo -e "${YELLOW}Installing git...${NC}"
  if command -v apt-get &>/dev/null; then
    sudo apt-get update -qq && sudo apt-get install -y git
  elif command -v brew &>/dev/null; then
    brew install git
  elif command -v dnf &>/dev/null; then
    sudo dnf install -y git
  else
    echo -e "${RED}Cannot install git automatically. Install git and try again.${NC}"
    exit 1
  fi
fi

# ============================================================================
# Step 2: Clone or update repository
# ============================================================================

echo -e "${YELLOW}[1/3] Repository${NC}"

if [ -d "$INSTALL_DIR/src/scripts/install.sh" ] || [ -f "$INSTALL_DIR/src/scripts/install.sh" ]; then
  echo -e "  Existing installation found — pulling latest..."
  cd "$INSTALL_DIR"
  git pull --ff-only 2>/dev/null || {
    echo -e "  ${YELLOW}Pull failed (local changes?) — continuing with current version${NC}"
  }
else
  echo -e "  Cloning Continuum..."
  git clone https://github.com/CambrianTech/continuum.git "$INSTALL_DIR"
  cd "$INSTALL_DIR"
fi

echo -e "  ${GREEN}Repository ready${NC}"

# ============================================================================
# Step 3: Run install.sh
# ============================================================================

echo -e "\n${YELLOW}[2/2] Installing + launching${NC}"

cd src

# Export mode so npm start and the orchestrator can read it
export CONTINUUM_UI_MODE="$MODE"
# Tell install.sh to auto-launch after install
export CONTINUUM_AUTO_LAUNCH=1

bash scripts/install.sh

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  Continuum is running!${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
case "$MODE" in
  browser)
    echo -e "  UI:        ${GREEN}http://localhost:9000${NC}"
    ;;
  cli)
    echo -e "  CLI:       ${GREEN}./jtag${NC}"
    ;;
  headless)
    echo -e "  Server:    ${GREEN}http://localhost:9000${NC} (API only)"
    ;;
esac
echo -e "  Stop:      ${GREEN}cd $INSTALL_DIR/src && npm stop${NC}"
echo -e "  Uninstall: ${GREEN}cd $INSTALL_DIR/src && bash scripts/uninstall.sh${NC}"
echo ""
