#!/bin/bash
# Preflight — Shared prerequisite checks for all shell scripts.
#
# Source this file to get colors + preflight functions.
# All functions are idempotent and safe to call multiple times.
# Platform-aware: works on macOS, Linux, and WSL.
#
# Usage:
#   source "$(dirname "${BASH_SOURCE[0]}")/shared/preflight.sh"
#   preflight_check_build_tools   # platform-appropriate build tool checks
#   preflight_require jq          # exits if jq not found, suggests install
#   preflight_require curl wget   # exits if NEITHER is found (any-of check)

# ============================================================================
# Colors — auto-exported on source
# ============================================================================

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

# ============================================================================
# Platform detection
# ============================================================================

# Returns: macos, linux, wsl, or unknown
preflight_detect_platform() {
  case "$(uname -s)" in
    Darwin) echo "macos" ;;
    Linux)
      if grep -qi microsoft /proc/version 2>/dev/null; then
        echo "wsl"
      else
        echo "linux"
      fi
      ;;
    *) echo "unknown" ;;
  esac
}

# ============================================================================
# Package manager detection
# ============================================================================

# Returns the detected package manager name: brew, apt, yum, dnf, or ""
preflight_detect_pkg_manager() {
  if command -v brew &>/dev/null; then echo "brew"
  elif command -v apt-get &>/dev/null; then echo "apt"
  elif command -v dnf &>/dev/null; then echo "dnf"
  elif command -v yum &>/dev/null; then echo "yum"
  else echo ""
  fi
}

# Suggest an install command for a given tool
_preflight_suggest_install() {
  local tool="$1"
  local mgr
  mgr=$(preflight_detect_pkg_manager)
  case "$mgr" in
    brew) echo "brew install $tool" ;;
    apt)  echo "sudo apt-get install -y $tool" ;;
    dnf)  echo "sudo dnf install -y $tool" ;;
    yum)  echo "sudo yum install -y $tool" ;;
    *)    echo "(install $tool using your system package manager)" ;;
  esac
}

# ============================================================================
# preflight_pkg_install <pkg> — cross-platform package install
# ============================================================================

preflight_pkg_install() {
  local pkg="$1"
  local mgr
  mgr=$(preflight_detect_pkg_manager)

  case "$mgr" in
    brew) brew install "$pkg" ;;
    apt)  sudo apt-get update -qq && sudo apt-get install -y "$pkg" ;;
    dnf)  sudo dnf install -y "$pkg" ;;
    yum)  sudo yum install -y "$pkg" ;;
    *)
      echo -e "${RED}No supported package manager found. Install '$pkg' manually.${NC}"
      return 1
      ;;
  esac
}

# ============================================================================
# preflight_require <tool> [tool2 ...] — check tool availability
#
# With one arg: exits if that tool is missing.
# With multiple args: exits if NONE are found (any-of semantics).
# ============================================================================

preflight_require() {
  if [ $# -eq 0 ]; then
    echo -e "${RED}preflight_require: no tool name provided${NC}"
    return 1
  fi

  if [ $# -eq 1 ]; then
    local tool="$1"
    if ! command -v "$tool" &>/dev/null; then
      echo -e "${RED}❌ Required tool not found: ${tool}${NC}"
      echo -e "${YELLOW}   Install: $(_preflight_suggest_install "$tool")${NC}"
      exit 1
    fi
    return 0
  fi

  # Multiple args: any-of semantics (e.g., curl OR wget)
  for tool in "$@"; do
    if command -v "$tool" &>/dev/null; then
      return 0
    fi
  done

  echo -e "${RED}❌ None of these tools found: $*${NC}"
  echo -e "${YELLOW}   Install at least one: $(_preflight_suggest_install "$1")${NC}"
  exit 1
}

# ============================================================================
# preflight_check_build_tools — platform-aware build tool checks
#
# Routes to the right checks for the current platform.
# Friendly messages, never looks like a crash.
# ============================================================================

preflight_check_build_tools() {
  local platform
  platform=$(preflight_detect_platform)
  case "$platform" in
    macos) _preflight_check_macos_tools ;;
    linux|wsl) _preflight_check_linux_tools ;;
  esac
}

# Alias — callers that already use this name keep working
preflight_check_xcode() { preflight_check_build_tools; }

# --- macOS: Xcode license + CLI tools ---

_preflight_check_macos_tools() {
  # Capture exit code — `|| xcrun_exit=$?` prevents set -e from killing the script
  local xcrun_exit
  xcrun --sdk macosx --show-sdk-path >/dev/null 2>&1 || xcrun_exit=$?
  xcrun_exit=${xcrun_exit:-0}

  if [ "$xcrun_exit" -eq 0 ]; then
    return 0
  fi

  if [ "$xcrun_exit" -eq 69 ]; then
    echo ""
    echo -e "  ${YELLOW}One quick thing before we start:${NC}"
    echo -e "  Apple requires you to accept the Xcode license before"
    echo -e "  build tools (compilers, linkers) can run."
    echo ""
    echo -e "  Either:"
    echo -e "    ${GREEN}1.${NC} Open Xcode.app and accept the license from the prompt"
    echo -e "    ${GREEN}2.${NC} Or from terminal: ${GREEN}sudo xcodebuild -license accept${NC}"
    echo ""
    echo -e "  Don't have Xcode? That works too — just the free CLI tools:"
    echo -e "       ${GREEN}xcode-select --install${NC}"
    echo ""
    echo -e "  Then run ${GREEN}npm start${NC} again."
    echo ""
    exit 0
  else
    echo ""
    echo -e "  ${YELLOW}One quick thing before we start:${NC}"
    echo -e "  Command-line build tools are needed to compile native code."
    echo ""
    echo -e "  Either:"
    echo -e "    ${GREEN}1.${NC} Install Xcode from the App Store (includes everything)"
    echo -e "    ${GREEN}2.${NC} Run in terminal: ${GREEN}xcode-select --install${NC} (lighter, CLI tools only)"
    echo ""
    echo -e "  Then run ${GREEN}npm start${NC} again."
    echo ""
    exit 0
  fi
}

# --- Linux / WSL: gcc, make, pkg-config ---

_preflight_check_linux_tools() {
  local missing=()

  command -v gcc &>/dev/null || command -v cc &>/dev/null || missing+=("gcc")
  command -v make &>/dev/null || missing+=("make")
  command -v pkg-config &>/dev/null || missing+=("pkg-config")

  if [ ${#missing[@]} -eq 0 ]; then
    return 0
  fi

  local install_cmd
  local mgr
  mgr=$(preflight_detect_pkg_manager)
  case "$mgr" in
    apt) install_cmd="sudo apt-get install -y build-essential pkg-config" ;;
    dnf) install_cmd="sudo dnf groupinstall -y 'Development Tools' && sudo dnf install -y pkg-config" ;;
    yum) install_cmd="sudo yum groupinstall -y 'Development Tools' && sudo yum install -y pkg-config" ;;
    *)   install_cmd="(install build-essential or equivalent for your distro)" ;;
  esac

  echo ""
  echo -e "  ${YELLOW}One quick thing before we start:${NC}"
  echo -e "  Build tools are needed to compile native code."
  echo -e "  Missing: ${missing[*]}"
  echo ""
  echo -e "  Install them with:"
  echo -e "      ${GREEN}${install_cmd}${NC}"
  echo ""
  echo -e "  Then run ${GREEN}npm start${NC} again."
  echo ""
  exit 0
}

# ============================================================================
# preflight_check_cargo_output <build_output> — detect build tool issues
#   in cargo stderr
#
# Returns 0 if a known issue was detected (message printed), 1 if not.
# ============================================================================

preflight_check_cargo_output() {
  local output="$1"

  if echo "$output" | grep -q "exit status: 69"; then
    echo -e "  Build failed: Xcode license not accepted."
    echo -e "  Accept it, then try again: ${GREEN}sudo xcodebuild -license accept${NC}"
    return 0
  fi

  if echo "$output" | grep -q "xcrun"; then
    echo -e "  Build failed: command-line build tools not found."
    echo -e "  Install them, then try again: ${GREEN}xcode-select --install${NC}"
    return 0
  fi

  if echo "$output" | grep -q "linker.*not found\|cannot find -l"; then
    echo -e "  Build failed: missing system libraries."
    echo -e "  Install build dependencies: ${GREEN}$(_preflight_suggest_install "build-essential")${NC}"
    return 0
  fi

  return 1
}

# Keep old name working
preflight_check_cargo_xcode() { preflight_check_cargo_output "$@"; }

# ============================================================================
# preflight_check_all — run all checks for current platform
# ============================================================================

preflight_check_all() {
  preflight_check_build_tools
}
