#!/bin/bash
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)
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
    # Git Bash / MSYS2 / Cygwin: a Unix-like shell on the Windows host, NOT a
    # Linux userland. No apt/Linux toolchain here, so the from-source dev
    # install can't run — callers redirect to WSL2 (or the Docker install.ps1).
    MINGW*|MSYS*|CYGWIN*) echo "windows-shell" ;;
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
  # Last, not first: WSL2 has apt AND can see winget via interop, and apt is
  # the right answer there. Only a NATIVE Windows shell (Git Bash) falls
  # through to winget.
  elif command -v winget.exe &>/dev/null || command -v winget &>/dev/null; then echo "winget"
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
    winget)
      # Native Windows (Git Bash). winget is Microsoft's stock manager on
      # Win10 21H2+/Win11 — present on exactly the boxes that hit this arm.
      # --id with the canonical package id, not a fuzzy name: fuzzy matching
      # prompts interactively, and there is no operator at an install.
      # Measured 2026-09-04: `npm run setup:rust` on a native Windows box died
      # right here ("No supported package manager found. Install 'jq'
      # manually.") — an install script telling the USER to install things is
      # the failure mode this repo exists to remove.
      local winget_id
      case "$pkg" in
        jq)    winget_id="jqlang.jq" ;;
        cmake) winget_id="Kitware.CMake" ;;
        *)     winget_id="$pkg" ;;
      esac
      winget install --id "$winget_id" --exact --silent         --accept-package-agreements --accept-source-agreements
      # winget installs land in a PATH the CURRENT Git Bash session may not
      # see (User PATH updates apply to new shells). Re-probe and say so
      # rather than letting the caller's next `command -v` fail mysteriously.
      hash -r 2>/dev/null || true
      if ! command -v "$pkg" &>/dev/null; then
        echo -e "${YELLOW}'$pkg' installed via winget but not on THIS shell's PATH yet — open a new terminal (or re-run) to pick it up.${NC}"
      fi
      ;;
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
# preflight_check_tailscale_ssh — auto-detect and re-enable Tailscale SSH
# ============================================================================
#
# A user-facing example of "let `npm start` self-heal." If Tailscale is
# installed AND authenticated AND the user is in a grid context, but the
# --ssh flag has been dropped (commonly by a plain `tailscale up` after a
# reboot or network change), re-add it idempotently.
#
# This means: every time anyone runs `npm start`, their Tailscale SSH state
# converges back to "on" without them having to remember scripts/install-
# tailscale.sh exists. No new manual ritual.
#
# Skipped when:
#   - Tailscale is not installed (single-machine local user — nothing to do)
#   - Tailscale is not authenticated (let install-tailscale.sh handle that)
#   - Tailscale is already running with --ssh on (no-op, fast probe)
#   - The user explicitly opted out: CONTINUUM_NO_TAILSCALE_PREFLIGHT=1
#   - We're not in a grid context (CONTINUUM_GRID is empty AND there are
#     no peer entries, so this is a single-machine-only setup)

preflight_check_tailscale_ssh() {
  [ "${CONTINUUM_NO_TAILSCALE_PREFLIGHT:-0}" = "1" ] && return 0
  command -v tailscale >/dev/null 2>&1 || return 0

  # Authenticated? (Has an IP.) If not, this isn't our job — the user
  # hasn't logged in to Tailscale yet, and we don't want to hijack
  # `npm start` with a sudo-required browser-auth flow.
  local ts_ip
  ts_ip=$(tailscale ip -4 2>/dev/null | head -1)
  [ -z "$ts_ip" ] && return 0

  # Probe RunSSH from prefs. Tolerate JSON shape changes across versions.
  local ssh_state
  ssh_state=$(tailscale debug prefs 2>/dev/null | python3 -c "
import sys, json
try:
    p = json.load(sys.stdin)
    print('on' if (p.get('RunSSH') or p.get('Prefs', {}).get('RunSSH')) else 'off')
except Exception:
    print('unknown')
" 2>/dev/null)

  if [ "$ssh_state" = "on" ]; then
    return 0   # already correct, silent no-op
  fi

  # Off (or probe inconclusive). Re-enable. Use sudo non-interactively
  # if a tty's available; otherwise emit the one-liner the user can run.
  echo ""
  echo "🔧 Tailscale is up but --ssh is off (peers can't reach you without per-device keys)."
  if [ -t 0 ] && command -v sudo >/dev/null 2>&1; then
    echo "   Re-enabling: sudo tailscale up --ssh --accept-routes"
    if sudo tailscale up --ssh --accept-routes; then
      echo "✅ Tailscale SSH re-enabled."
    else
      echo "⚠️  Re-enable failed. Run manually:"
      echo "   sudo tailscale up --ssh --accept-routes"
    fi
  else
    # Non-interactive (CI, background, etc.) — don't block, just instruct.
    echo "   Run when you're at a terminal:"
    echo "   sudo tailscale up --ssh --accept-routes"
  fi
}

# ============================================================================
# preflight_check_all — run all checks for current platform
# ============================================================================

preflight_check_all() {
  preflight_check_build_tools
  preflight_check_tailscale_ssh
}
