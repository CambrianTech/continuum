#!/usr/bin/env bash
# install-common.sh — shared primitives for the Continuum install scripts.
#
# Sourced (not executed) by both:
#   - src/scripts/install.sh    (canonical, Dev's npm-start path and Carl's delegate)
#   - install.sh (repo root)    (Carl's curl target, clones repo then delegates)
#
# Defines:
#   - Log primitives: info / ok / warn / die
#   - Module primitives: module_skip / module_start / module_done / module_fail
#   - The ONE legal sudo prompt source: ensure_sudo_warmed
#
# Contract (see docs/infrastructure/INSTALL-ARCHITECTURE.md):
#   - A fresh install that needs sudo N times prompts for the password EXACTLY
#     ONCE. Every subsequent sudo uses the warmed cache. Re-runs that do no
#     work prompt for zero passwords. This holds for Carl, Dev, and anyone.
#   - Every install step is a function `mod_*` with a self-guarded
#     idempotency check; re-running is safe and silent when nothing to do.

# Idempotent source guard — don't redefine if already sourced.
if [ "${_CONTINUUM_INSTALL_COMMON_LOADED:-0}" = "1" ]; then
  return 0
fi
_CONTINUUM_INSTALL_COMMON_LOADED=1

# ── Colors ──────────────────────────────────────────────────
if [ -t 1 ]; then
  _C_GREEN='\033[0;32m'
  _C_YELLOW='\033[1;33m'
  _C_RED='\033[0;31m'
  _C_BLUE='\033[0;34m'
  _C_DIM='\033[0;90m'
  _C_RESET='\033[0m'
else
  _C_GREEN=''; _C_YELLOW=''; _C_RED=''; _C_BLUE=''; _C_DIM=''; _C_RESET=''
fi

# ── Generic log primitives ──────────────────────────────────
info()  { printf '%b→%b %s\n'  "$_C_BLUE"   "$_C_RESET" "$*" ; }
ok()    { printf '%b✓%b %s\n'  "$_C_GREEN"  "$_C_RESET" "$*" ; }
warn()  { printf '%b!%b %s\n'  "$_C_YELLOW" "$_C_RESET" "$*" >&2 ; }
die()   { printf '%b✗%b %s\n'  "$_C_RED"    "$_C_RESET" "$*" >&2 ; exit 1 ; }
# `fail` is a pre-existing synonym for `die` in some call sites; alias here
# so both names work regardless of which script sourced the lib first.
fail()  { die "$@"; }

# ── Module primitives ───────────────────────────────────────
# Every install step (`mod_*` function) uses these so output is uniform
# and Carl can read top-to-bottom to understand what's happening.
#
# Usage:
#   module_skip  <name> <reason-it-was-already-done>
#   module_start <name> <what-we-are-about-to-do-in-plain-english>
#   module_done  <name>
#   module_fail  <name> <clear-next-step-for-the-user>

module_skip()  { printf '%b✓%b [%s] %s %b(skipped)%b\n' "$_C_GREEN"  "$_C_RESET" "$1" "$2" "$_C_DIM" "$_C_RESET" ; }
module_start() { printf '%b⧗%b [%s] %s\n'               "$_C_BLUE"   "$_C_RESET" "$1" "$2" ; }
module_done()  { printf '%b✓%b [%s] done\n'             "$_C_GREEN"  "$_C_RESET" "$1" ; }
module_fail()  { printf '%b✗%b [%s] %s\n'               "$_C_RED"    "$_C_RESET" "$1" "$2" >&2 ; exit 1 ; }

# ── The one legal sudo prompt source ────────────────────────
#
# Lazy-warm the sudo cache. First call prompts (once). Subsequent calls are
# no-ops. Modules that need sudo MUST call this BEFORE invoking `sudo …`,
# and NEVER prompt for a password themselves.
#
# The keepalive loop refreshes the sudo timestamp every 50s so the whole
# install run stays in cache (sudo's default timeout is 5 min). It dies
# with the parent via EXIT trap — no stray processes left behind.
#
# Safe to call from any module: idempotent guard + already-warmed check
# make repeated calls free.
ensure_sudo_warmed() {
  # Already root? Nothing to do.
  [ "$(id -u)" -eq 0 ] && return 0
  # Passwordless sudo (NOPASSWD rule already grants us everything)?
  sudo -n true 2>/dev/null && return 0
  # Warmed earlier in this same run?
  [ "${_SUDO_WARMED:-0}" = "1" ] && return 0
  # No terminal — we cannot prompt. Fail loud with a specific fix.
  if [ ! -t 0 ]; then
    die "Install needs sudo but stdin is not a terminal. Re-run in an interactive shell, or use the docker-compose path which needs no sudo."
  fi

  info "Admin access needed — prompting once now; no further password prompts this run."
  if ! sudo -v; then
    die "sudo authentication failed. Re-run after fixing your password or sudoers."
  fi

  # Arm the keepalive: refresh the sudo timestamp until this script exits.
  ( while true; do sudo -n true 2>/dev/null || exit; sleep 50; done ) &
  _SUDO_KEEPALIVE_PID=$!
  # Trap EXIT so we don't leave an orphaned refresher. Preserve any prior trap.
  _prev_exit_trap=$(trap -p EXIT | sed "s/^trap -- '//; s/' EXIT$//")
  if [ -n "$_prev_exit_trap" ]; then
    trap "${_prev_exit_trap}; _sudo_cleanup" EXIT
  else
    trap '_sudo_cleanup' EXIT
  fi

  _SUDO_WARMED=1
}

_sudo_cleanup() {
  if [ -n "${_SUDO_KEEPALIVE_PID:-}" ]; then
    kill "$_SUDO_KEEPALIVE_PID" 2>/dev/null || true
    unset _SUDO_KEEPALIVE_PID
  fi
}

# ═════════════════════════════════════════════════════════════
# Shared modules — see docs/infrastructure/INSTALL-ARCHITECTURE.md
#
# Modules that BOTH Carl's root install.sh and Dev's canonical
# src/scripts/install.sh use. Every module:
#   - Self-guarded (idempotency check first; re-run = no-op)
#   - Self-applicable (skip early when not this platform/mode)
#   - Lazy sudo via ensure_sudo_warmed (prompt at most once per run)
#   - Plain-English announce so Carl knows what we're doing
# ═════════════════════════════════════════════════════════════

# ── mod_submodules_init ─────────────────────────────────────
# Vendored substrates (llama.cpp, whisper.cpp) live as git submodules.
# Docker build AND dev build both need them populated. No sudo. Expected
# cwd: the repo root (or any subdir whose parent contains .gitmodules).
mod_submodules_init() {
  # Find repo root by walking up for .gitmodules.
  local repo_root; repo_root="$(git rev-parse --show-toplevel 2>/dev/null)"
  if [ -z "$repo_root" ] || [ ! -f "$repo_root/.gitmodules" ]; then
    module_skip "submodules" "no .gitmodules at repo root"
    return 0
  fi
  if [ -f "$repo_root/src/workers/vendor/llama.cpp/CMakeLists.txt" ] && \
     [ -f "$repo_root/src/workers/vendor/whisper.cpp/CMakeLists.txt" ]; then
    module_skip "submodules" "llama.cpp + whisper.cpp already populated"
    return 0
  fi
  module_start "submodules" "Populating vendored llama.cpp + whisper.cpp"
  (cd "$repo_root" && git submodule update --init --recursive 2>&1 | grep -vE '^(Submodule.*registered|Cloning into)') \
    || module_fail "submodules" "git submodule update failed. Run manually: git submodule update --init --recursive"
  module_done "submodules"
}

# ── mod_docker_wsl_integration ──────────────────────────────
# On WSL2, Docker Desktop's "Enable integration with my default WSL distro"
# toggle sometimes isn't hooked for the active distro — the daemon is up
# on the Windows side but /var/run/docker.sock is missing in Linux. This
# module fixes it in-place: edit the Windows user's own settings-store.json
# (no Linux sudo needed), bounce Docker Desktop, poll for the socket.
# Skip on any non-WSL2 host.
mod_docker_wsl_integration() {
  # Applicability guards.
  if ! grep -qi microsoft /proc/version 2>/dev/null; then
    module_skip "docker-wsl-integration" "not WSL2"
    return 0
  fi
  if [ ! -d /mnt/wsl/docker-desktop ]; then
    module_skip "docker-wsl-integration" "Docker Desktop shared mount not present"
    return 0
  fi
  # Idempotency: socket already reachable.
  if [ -S /var/run/docker.sock ] && docker info &>/dev/null; then
    module_skip "docker-wsl-integration" "already reachable"
    return 0
  fi

  local distro="${WSL_DISTRO_NAME:-$(grep '^NAME=' /etc/os-release | cut -d\" -f2 | head -1)}"
  local settings
  settings=$(ls /mnt/c/Users/*/AppData/Roaming/Docker/settings-store.json 2>/dev/null | head -1)
  if [ -z "$settings" ] || [ ! -w "$settings" ]; then
    module_fail "docker-wsl-integration" "Cannot locate writable Docker Desktop settings-store.json. Is Docker Desktop installed for the Windows user?"
  fi

  module_start "docker-wsl-integration" "Adding '$distro' to Docker Desktop IntegratedWslDistros + bouncing Docker Desktop"
  python3 - "$settings" "$distro" <<'PY' || module_fail "docker-wsl-integration" "Failed to edit Docker Desktop settings. Open Docker Desktop → Settings → Resources → WSL Integration → enable your distro manually."
import json, sys
path, distro = sys.argv[1], sys.argv[2]
with open(path) as f:
    cfg = json.load(f)
distros = cfg.setdefault("IntegratedWslDistros", [])
if distro not in distros:
    distros.append(distro)
    with open(path, "w") as f:
        json.dump(cfg, f, indent=2)
PY

  local pwsh="/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"
  if [ -x "$pwsh" ]; then
    "$pwsh" -Command 'Stop-Process -Name "Docker Desktop" -Force -ErrorAction SilentlyContinue; Start-Sleep 2; Start-Process -FilePath "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe"' >/dev/null 2>&1 || true
  fi

  # Poll for the socket — up to 60s. Carl's launch budget.
  for i in $(seq 1 30); do
    [ -S /var/run/docker.sock ] && docker info &>/dev/null && { module_done "docker-wsl-integration"; return 0; }
    sleep 2
  done
  module_fail "docker-wsl-integration" "Docker Desktop didn't expose /var/run/docker.sock after 60s. Run 'wsl --shutdown' from Windows PowerShell, then re-run this installer."
}
