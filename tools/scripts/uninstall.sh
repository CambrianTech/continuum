#!/bin/bash
# Continuum Tower Uninstall — Cleanly remove everything install.sh installed.
#
# Usage:
#   cd continuum/src
#   bash scripts/uninstall.sh          # interactive (asks before each step)
#   bash scripts/uninstall.sh --yes    # non-interactive (skip confirmations)
#
# Does NOT remove: system packages (build-essential, etc.), PostgreSQL, CUDA
# Safe to run multiple times (idempotent)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

source "$SCRIPT_DIR/shared/preflight.sh"

# Parse flags
AUTO_YES=false
for arg in "$@"; do
  case "$arg" in
    --yes|-y) AUTO_YES=true ;;
  esac
done

PLATFORM=$(preflight_detect_platform)

# ============================================================================
# Helpers
# ============================================================================

# Ask for confirmation. Returns 0 if confirmed, 1 if declined.
# Skipped (auto-yes) when --yes flag is set.
confirm() {
  local prompt="$1"
  if $AUTO_YES; then
    return 0
  fi
  echo -ne "  ${prompt} [y/N] "
  read -r answer
  case "$answer" in
    [yY]|[yY][eE][sS]) return 0 ;;
    *) return 1 ;;
  esac
}

# Print size of a directory in human-readable form, or "0B" if missing
dir_size() {
  local path="$1"
  if [ -d "$path" ]; then
    du -sh "$path" 2>/dev/null | awk '{print $1}'
  else
    echo "0B"
  fi
}

# Track total freed space (in KB) for summary
TOTAL_FREED_KB=0

# Add directory size (in KB) to running total, then remove it
remove_dir() {
  local path="$1"
  if [ -d "$path" ]; then
    local kb
    kb=$(du -sk "$path" 2>/dev/null | awk '{print $1}')
    TOTAL_FREED_KB=$((TOTAL_FREED_KB + kb))
    rm -rf "$path"
  fi
}

# Format KB as human-readable
format_kb() {
  local kb=$1
  if [ "$kb" -ge 1048576 ]; then
    echo "$(( kb / 1048576 ))GB"
  elif [ "$kb" -ge 1024 ]; then
    echo "$(( kb / 1024 ))MB"
  else
    echo "${kb}KB"
  fi
}

# ============================================================================
# Header
# ============================================================================

echo ""
echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${RED}  Continuum Tower Uninstall${NC}"
echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo -e "  Platform: ${GREEN}${PLATFORM}${NC}"
echo ""

# ============================================================================
# Survey what's installed
# ============================================================================

echo -e "${YELLOW}Scanning for installed components...${NC}"
echo ""

VENV_DIR="$HOME/.continuum/venv"
CONFIG_DIR="$HOME/.continuum"
NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
CARGO_DIR="$HOME/.cargo"
RUSTUP_DIR="$HOME/.rustup"
NODE_MODULES_DIR="$PROJECT_DIR/node_modules"

items=()

if [ -d "$VENV_DIR" ]; then
  items+=("venv")
  echo -e "  ${BLUE}Python venv${NC}       $VENV_DIR ($(dir_size "$VENV_DIR"))"
fi

if [ -d "$NODE_MODULES_DIR" ]; then
  items+=("node_modules")
  echo -e "  ${BLUE}node_modules${NC}      $NODE_MODULES_DIR ($(dir_size "$NODE_MODULES_DIR"))"
fi

if [ -d "$CONFIG_DIR" ]; then
  items+=("config")
  echo -e "  ${BLUE}Config dir${NC}        $CONFIG_DIR ($(dir_size "$CONFIG_DIR"))"
  if [ -f "$CONFIG_DIR/config.env" ]; then
    echo -e "    ${YELLOW}^ Contains config.env with API keys${NC}"
  fi
fi

if command -v rustup &>/dev/null && [ -d "$RUSTUP_DIR" ]; then
  items+=("rust")
  local_rust_size="$(dir_size "$CARGO_DIR") + $(dir_size "$RUSTUP_DIR")"
  echo -e "  ${BLUE}Rust (rustup)${NC}     $CARGO_DIR ($(dir_size "$CARGO_DIR")) + $RUSTUP_DIR ($(dir_size "$RUSTUP_DIR"))"
fi

case "$PLATFORM" in
  macos)
    if command -v node &>/dev/null && brew list node &>/dev/null 2>&1; then
      items+=("node")
      echo -e "  ${BLUE}Node.js${NC}           $(node --version) (brew)"
    fi
    ;;
  linux|wsl)
    if [ -d "$NVM_DIR" ]; then
      items+=("nvm")
      echo -e "  ${BLUE}Node.js (nvm)${NC}     $NVM_DIR ($(dir_size "$NVM_DIR"))"
    fi
    ;;
esac

echo ""

if [ ${#items[@]} -eq 0 ]; then
  echo -e "${GREEN}Nothing to uninstall. Continuum components not found.${NC}"
  echo ""
  exit 0
fi

echo -e "  ${YELLOW}Will NOT remove: system packages, PostgreSQL, CUDA${NC}"
echo ""

# ============================================================================
# Step 1: Stop the running system
# ============================================================================

echo -e "${YELLOW}[1/6] Stopping running system${NC}"

if [ -f "$SCRIPT_DIR/system-stop.sh" ]; then
  bash "$SCRIPT_DIR/system-stop.sh" 2>/dev/null || true
  echo -e "  ${GREEN}System stopped${NC}"
else
  echo -e "  ${YELLOW}system-stop.sh not found, skipping${NC}"
fi

# ============================================================================
# Step 2: Remove Python venv
# ============================================================================

echo -e "${YELLOW}[2/6] Python venv${NC}"

if [ -d "$VENV_DIR" ]; then
  echo -e "  Found: $VENV_DIR ($(dir_size "$VENV_DIR"))"
  if confirm "Remove Python venv?"; then
    remove_dir "$VENV_DIR"
    echo -e "  ${GREEN}Removed${NC}"
  else
    echo -e "  ${YELLOW}Skipped${NC}"
  fi
else
  echo -e "  ${GREEN}Not present${NC}"
fi

# ============================================================================
# Step 3: Remove node_modules
# ============================================================================

echo -e "${YELLOW}[3/6] node_modules${NC}"

if [ -d "$NODE_MODULES_DIR" ]; then
  echo -e "  Found: $NODE_MODULES_DIR ($(dir_size "$NODE_MODULES_DIR"))"
  if confirm "Remove node_modules?"; then
    remove_dir "$NODE_MODULES_DIR"
    echo -e "  ${GREEN}Removed${NC}"
  else
    echo -e "  ${YELLOW}Skipped${NC}"
  fi
else
  echo -e "  ${GREEN}Not present${NC}"
fi

# ============================================================================
# Step 4: Remove ~/.continuum config directory
# ============================================================================

echo -e "${YELLOW}[4/6] Config directory (~/.continuum)${NC}"

if [ -d "$CONFIG_DIR" ]; then
  echo -e "  Found: $CONFIG_DIR ($(dir_size "$CONFIG_DIR"))"
  if [ -f "$CONFIG_DIR/config.env" ]; then
    echo -e "  ${RED}WARNING: This contains config.env with your API keys!${NC}"
    echo -e "  ${YELLOW}Back up $CONFIG_DIR/config.env if you want to keep your keys.${NC}"
  fi
  if confirm "Remove $CONFIG_DIR? (contains sessions, logs, adapters, config)"; then
    remove_dir "$CONFIG_DIR"
    echo -e "  ${GREEN}Removed${NC}"
  else
    echo -e "  ${YELLOW}Skipped${NC}"
  fi
else
  echo -e "  ${GREEN}Not present${NC}"
fi

# ============================================================================
# Step 4b: Drop PostgreSQL database
# ============================================================================

echo -e "${YELLOW}[4b/6] PostgreSQL database${NC}"

if command -v psql &>/dev/null; then
  if psql -lqt 2>/dev/null | cut -d \| -f 1 | grep -qw continuum; then
    echo -e "  Found: database 'continuum'"
    if confirm "Drop database 'continuum'? (PostgreSQL server will NOT be removed)"; then
      dropdb continuum 2>/dev/null || sudo -u postgres dropdb continuum 2>/dev/null || true
      echo -e "  ${GREEN}Database dropped${NC}"
    else
      echo -e "  ${YELLOW}Skipped${NC}"
    fi
  else
    echo -e "  ${GREEN}Database 'continuum' does not exist${NC}"
  fi
else
  echo -e "  ${GREEN}psql not found — no database to clean${NC}"
fi

# ============================================================================
# Step 5: Offer to uninstall Rust (rustup)
# ============================================================================

echo -e "${YELLOW}[5/6] Rust toolchain${NC}"

if command -v rustup &>/dev/null && [ -d "$RUSTUP_DIR" ]; then
  echo -e "  Found: rustup with $(rustc --version 2>/dev/null | awk '{print $2}' || echo 'unknown version')"
  echo -e "  Dirs: $CARGO_DIR ($(dir_size "$CARGO_DIR")) + $RUSTUP_DIR ($(dir_size "$RUSTUP_DIR"))"
  echo -e "  ${YELLOW}Note: Only uninstall if Rust was installed by Continuum and you don't use it elsewhere.${NC}"
  if confirm "Uninstall Rust via rustup?"; then
    # Track sizes before removal
    cargo_kb=$(du -sk "$CARGO_DIR" 2>/dev/null | awk '{print $1}') || cargo_kb=0
    rustup_kb=$(du -sk "$RUSTUP_DIR" 2>/dev/null | awk '{print $1}') || rustup_kb=0
    TOTAL_FREED_KB=$((TOTAL_FREED_KB + cargo_kb + rustup_kb))
    rustup self uninstall -y
    echo -e "  ${GREEN}Rust uninstalled${NC}"
  else
    echo -e "  ${YELLOW}Skipped${NC}"
  fi
else
  echo -e "  ${GREEN}Not present (or not installed via rustup)${NC}"
fi

# ============================================================================
# Step 6: Offer to uninstall Node.js
# ============================================================================

echo -e "${YELLOW}[6/6] Node.js${NC}"

case "$PLATFORM" in
  macos)
    if command -v node &>/dev/null && brew list node &>/dev/null 2>&1; then
      echo -e "  Found: Node.js $(node --version) (installed via Homebrew)"
      echo -e "  ${YELLOW}Note: Only uninstall if Node.js was installed by Continuum and you don't use it elsewhere.${NC}"
      if confirm "Uninstall Node.js via brew?"; then
        brew uninstall node
        echo -e "  ${GREEN}Node.js uninstalled${NC}"
      else
        echo -e "  ${YELLOW}Skipped${NC}"
      fi
    else
      echo -e "  ${GREEN}Not installed via Homebrew (or not present)${NC}"
    fi
    ;;
  linux|wsl)
    if [ -d "$NVM_DIR" ]; then
      echo -e "  Found: nvm at $NVM_DIR ($(dir_size "$NVM_DIR"))"
      echo -e "  ${YELLOW}Note: Only uninstall if nvm was installed by Continuum and you don't use it elsewhere.${NC}"
      if confirm "Remove nvm and all Node.js versions?"; then
        remove_dir "$NVM_DIR"
        # Clean nvm lines from shell profiles
        for profile in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
          if [ -f "$profile" ]; then
            # Remove nvm-related lines (export NVM_DIR, sourcing nvm.sh, nvm bash_completion)
            sed -i.bak '/NVM_DIR/d;/nvm\.sh/d;/nvm.*bash_completion/d' "$profile"
            rm -f "${profile}.bak"
          fi
        done
        echo -e "  ${GREEN}nvm removed (shell profile entries cleaned)${NC}"
      else
        echo -e "  ${YELLOW}Skipped${NC}"
      fi
    else
      echo -e "  ${GREEN}nvm not present${NC}"
    fi
    ;;
esac

# ============================================================================
# Summary
# ============================================================================

echo ""
echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}  Uninstall complete${NC}"
echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
if [ "$TOTAL_FREED_KB" -gt 0 ]; then
  echo -e "  Disk space recovered: ${GREEN}~$(format_kb $TOTAL_FREED_KB)${NC}"
fi
echo ""
echo -e "  ${YELLOW}Not removed (shared/external):${NC}"
echo -e "    - System packages (build-essential, jq, cmake, etc.)"
echo -e "    - PostgreSQL server (database 'continuum' was dropped if you confirmed)"
echo -e "    - CUDA / NVIDIA drivers"
echo ""
echo -e "  To reinstall: ${GREEN}cd src && bash scripts/install.sh${NC}"
echo ""
