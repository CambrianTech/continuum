#!/usr/bin/env bash
set -o pipefail  # a failing command in a pipeline must not read as success (card aad30dee)
# install-common.sh — shared primitives for the Continuum install scripts.
#
# Sourced (not executed) by both:
#   - tools/scripts/install.sh    (canonical, Dev's npm-start path and Carl's delegate)
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

  # Caller signalled this is a non-fatal probe (e.g. parallel-start.sh
  # re-running install.sh with CONTINUUM_DEPS_ONLY=1) — return non-zero
  # and let the caller decide whether to skip-and-warn or fail loud.
  if [ "${CONTINUUM_NONFAIL_SUDO:-0}" = "1" ]; then
    return 1
  fi

  # Stdin not a TTY (curl | bash pattern, or background-launched scripts).
  # Try /dev/tty as a fallback — many shells preserve a controlling
  # terminal even when stdin is piped. This is what makes
  #   curl ... install.sh | bash
  # actually work for sudo prompts.
  local sudo_in
  if [ -t 0 ]; then
    sudo_in=""           # stdin already a TTY — pass-through
  elif [ -r /dev/tty ] && [ -w /dev/tty ]; then
    sudo_in="< /dev/tty" # piped stdin but controlling TTY exists — recover
  else
    die "Install needs sudo but no TTY is available (stdin not a terminal and /dev/tty unreadable). Re-run from an interactive shell, or set CONTINUUM_NONFAIL_SUDO=1 to make sudo failures non-fatal."
  fi

  info "Admin access needed — prompting once now; no further password prompts this run."
  if ! eval "sudo -v $sudo_in"; then
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
# tools/scripts/install.sh use. Every module:
#   - Self-guarded (idempotency check first; re-run = no-op)
#   - Self-applicable (skip early when not this platform/mode)
#   - Lazy sudo via ensure_sudo_warmed (prompt at most once per run)
#   - Plain-English announce so Carl knows what we're doing
# ═════════════════════════════════════════════════════════════

# ── mod_cold_storage ────────────────────────────────────────
# Linux/macOS twin of the Windows Mod-ColdStorage. DEFAULT behavior: auto-detect
# the roomiest mounted filesystem that ISN'T the one holding $HOME and route cold
# artifacts (model cache, CONTINUUM_STORAGE_PATH, cargo build cache) there,
# migrating what's already on the home filesystem. Generic — roomiest by free
# space, never a hardcoded mount. Reconfigure later via ~/.continuum/config.env.
# No sudo (all under $HOME + a user-writable mount).
#
# NOTE: run-verify pending on a real multi-drive Linux node; this mirrors the
# Windows module proven end-to-end on the RTX 5090 (Model 1/21 present from D:).
_COLD_MIN_FREE_KB=$((256 * 1024 * 1024))   # 256 GB, in df -Pk (1K) units

# Print the roomiest eligible mountpoint (>= min free, not the $HOME fs, not a
# pseudo fs), or nothing.
#
# WSL-aware (the primary target: a Linux core under Windows). Windows drives
# mount at /mnt/<letter> via drvfs; /mnt/c is the Windows SYSTEM drive (exclude
# like /), /mnt/d.. are the large data drives (a 16TB D: shows up here). Writing
# to /mnt/d/continuum-cold IS the Windows D: drive, reachable from both WSL and
# native Windows — cold storage + the genome pager's backing store land on the
# big drive automatically under WSL too.
_cold_drive() {
  local home_src; home_src="$(df -Pk "$HOME" 2>/dev/null | awk 'NR==2{print $1}')"
  df -Pk 2>/dev/null | awk -v home="$home_src" -v min="$_COLD_MIN_FREE_KB" '
    NR>1 && $1!=home && $4>=min {
      mp=$6
      if (mp ~ /^\/(proc|sys|dev|run|snap|boot)(\/|$)/) next
      if (mp=="/") next
      if (mp ~ /^\/mnt\/wsl(\/|$)/) next   # WSL-internal mounts, never storage
      if (mp=="/mnt/c") next               # Windows SYSTEM drive under WSL — exclude like /
      if ($4>max) { max=$4; best=mp }
    }
    END { if (best!="") print best }'
}

# Migrate a cold dir to the big drive. Idempotent: skips when absent, a symlink,
# or already relocated. mv first (fast on same fs); cp -a + rm across filesystems.
_cold_migrate() {
  local src="$1" dst="$2"
  [ -d "$src" ] || return 0
  [ -L "$src" ] && return 0
  if [ -e "$dst" ]; then info "  cold: $dst already present -- leaving source"; return 0; fi
  mkdir -p "$(dirname "$dst")"
  info "  cold: migrating $src -> $dst"
  if ! mv "$src" "$dst" 2>/dev/null; then cp -a "$src" "$dst" && rm -rf "$src"; fi
}

# Persist + export the cold-storage config. config.env carries
# CONTINUUM_STORAGE_PATH (the core reads it from there). HF_HOME + CARGO_TARGET_DIR
# are exported for this session; cross-shell persistence on Linux is via config.env
# (the core reads CONTINUUM_STORAGE_PATH from it) — HF_HOME cross-launch parity is
# tracked with M5 (teach the core to read HF_HOME from config.env too).
_cold_export() {
  local cold_root="$1" hf="$1/huggingface" cargo="$1/cargo-target"
  local config_env="$HOME/.continuum/config.env"
  mkdir -p "$HOME/.continuum"
  {
    echo "# Continuum storage config -- auto-generated by install (cold-storage module)."
    echo "# Cold artifacts (models, genome, build cache) live on a large drive."
    echo "# Reconfigure by editing CONTINUUM_STORAGE_PATH; re-running install respects it."
    # SINGLE-QUOTED, and that is load-bearing. This file is `source`d by bash
    # (start-server.sh), which treats a backslash in an unquoted value as an escape
    # character. A Windows path therefore does not survive the round trip:
    #   HF_HOME=D:\continuum-cold\huggingface   sources as   D:continuum-coldhuggingface
    # which Windows resolves, drive-relative, into an entirely separate cache root.
    # MEASURED: a 76 GB model download landed in D:\continuum-coldhuggingface\ while
    # every resolver looked under D:\continuum-cold\huggingface\ and found nothing.
    echo "CONTINUUM_STORAGE_PATH='$cold_root'"
    echo "HF_HOME='$hf'"
  } > "$config_env"
  export CONTINUUM_STORAGE_PATH="$cold_root" HF_HOME="$hf" CARGO_TARGET_DIR="$cargo"
}

mod_cold_storage() {
  local config_env="$HOME/.continuum/config.env"
  # Already routed to a present path? re-export + skip (idempotent).
  if [ -f "$config_env" ]; then
    local existing; existing="$(grep -E '^[[:space:]]*CONTINUUM_STORAGE_PATH[[:space:]]*=' "$config_env" 2>/dev/null | head -1 | cut -d= -f2- | xargs)"
    if [ -n "$existing" ] && [ -d "$(dirname "$existing")" ]; then
      _cold_export "$existing"
      module_skip "cold-storage" "already routed to $existing (edit ~/.continuum/config.env to change)"
      return 0
    fi
  fi
  local mp; mp="$(_cold_drive)"
  if [ -z "$mp" ]; then
    module_skip "cold-storage" "no large secondary drive (>= 256GB free) -- staying on the home filesystem"
    return 0
  fi
  local cold_root="$mp/continuum-cold"
  module_start "cold-storage" "routing cold artifacts to $mp -- auto-detected"
  mkdir -p "$cold_root"
  _cold_migrate "$HOME/.cache/huggingface"            "$cold_root/huggingface"
  _cold_migrate "$HOME/.continuum/genome"             "$cold_root/genome"
  _cold_migrate "$HOME/.continuum/cache/cargo-target" "$cold_root/cargo-target"
  _cold_export "$cold_root"
  module_done "cold-storage"
  ok "cold storage -> $cold_root (models, genome, build cache). Reconfigure: ~/.continuum/config.env"
}

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
  if [ -f "$repo_root/core/vendor/llama.cpp/CMakeLists.txt" ] && \
     [ -f "$repo_root/core/vendor/whisper.cpp/CMakeLists.txt" ]; then
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

# ── mod_continuum_bin_link ──────────────────────────────────
# Place the `continuum` CLI on PATH. Tries (in order):
#   1. /usr/local/bin/continuum if writable without sudo
#   2. /usr/local/bin/continuum via ensure_sudo_warmed (one-prompt contract)
#   3. ~/.local/bin/continuum (user-space fallback, no sudo)
#
# On a headless install (no TTY), step 2 cleanly degrades to step 3 instead
# of crashing on `sudo: a terminal is required` — that was the fail mode
# Carl hit on the first BigMama dry-run.
#
# Args:
#   $1 — absolute path to the source `continuum` script (typically
#        $INSTALL_DIR/bin/continuum).
mod_continuum_bin_link() {
  local src="$1"
  if [ -z "$src" ] || [ ! -f "$src" ]; then
    module_fail "continuum-bin" "source binary missing at: $src"
  fi

  # Idempotency: if /usr/local/bin/continuum already points at this src
  # (or is byte-identical), skip. Same for the user-space fallback.
  if [ -x "/usr/local/bin/continuum" ] && cmp -s "$src" "/usr/local/bin/continuum" 2>/dev/null; then
    module_skip "continuum-bin" "/usr/local/bin/continuum already current"
    return 0
  fi
  if [ -x "$HOME/.local/bin/continuum" ] && cmp -s "$src" "$HOME/.local/bin/continuum" 2>/dev/null; then
    module_skip "continuum-bin" "~/.local/bin/continuum already current"
    return 0
  fi

  # Tier 1: writable system path (root, devcontainer, etc.)
  if [ -w "/usr/local/bin" ]; then
    module_start "continuum-bin" "Linking continuum CLI → /usr/local/bin/continuum"
    cp "$src" "/usr/local/bin/continuum" && chmod +x "/usr/local/bin/continuum" \
      || module_fail "continuum-bin" "cp to /usr/local/bin failed (filesystem read-only?)"
    module_done "continuum-bin"
    return 0
  fi

  # Tier 2: try sudo if we have a TTY (ensure_sudo_warmed handles the
  # no-TTY case by routing us to the user-space fallback below).
  if command -v sudo &>/dev/null && [ -t 0 ]; then
    module_start "continuum-bin" "Linking continuum CLI → /usr/local/bin/continuum (needs sudo)"
    ensure_sudo_warmed
    sudo cp "$src" "/usr/local/bin/continuum" && sudo chmod +x "/usr/local/bin/continuum" \
      || module_fail "continuum-bin" "sudo cp to /usr/local/bin failed"
    module_done "continuum-bin"
    return 0
  fi

  # Tier 3: user-space fallback. No sudo, no surprises. Add ~/.local/bin
  # to PATH if it isn't there (warn the user once).
  module_start "continuum-bin" "Linking continuum CLI → ~/.local/bin/continuum (user-space fallback, no sudo)"
  mkdir -p "$HOME/.local/bin"
  cp "$src" "$HOME/.local/bin/continuum" && chmod +x "$HOME/.local/bin/continuum" \
    || module_fail "continuum-bin" "cp to ~/.local/bin failed"
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) warn "~/.local/bin is not in your PATH. Add: export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
  esac
  module_done "continuum-bin"
}

# ── mod_jtag_bin_link ───────────────────────────────────────
# Place the `jtag` CLI on PATH. SYMLINK (not cp) because src/jtag is a
# bash launcher that uses `dirname "${BASH_SOURCE[0]}"` to locate
# dist/cli-bundle.js relative to its own directory — `cp` would put
# the launcher at /usr/local/bin/jtag where SCRIPT_DIR resolves to
# /usr/local/bin and the bundle lookup fails. A symlink preserves
# BASH_SOURCE traversal back to the install dir's src/, so the
# launcher finds dist/cli-bundle.js correctly.
#
# Bug origin: airc-8a5e 2026-05-03 Carl-UX QA caught that
# CLAUDE.md / skill docs reference `./jtag` and `jtag <command>` as
# the chat surface, but install.sh only ever symlinked `continuum` —
# `jtag` was at $INSTALL_DIR/src/jtag with no PATH entry. Users hit
# command-not-found and never got to the chat probe at all.
#
# Same tier-fallback shape as mod_continuum_bin_link: try writable
# system path, then sudo, then user-space fallback. Idempotent re-run
# (skip when symlink already current).
#
# Args:
#   $1 — absolute path to the source jtag launcher (typically
#        $INSTALL_DIR/src/jtag).
mod_jtag_bin_link() {
  local src="$1"
  if [ -z "$src" ] || [ ! -f "$src" ]; then
    # The old Node `jtag` CLI moved to legacy/ (#1840) and is NOT part of a
    # headless-core install. Its absence must not abort the installer — the
    # core plus the `continuum` / `cu` CLIs are the deliverable. Skip, don't
    # fail (this is the exact fatal that reddened carl-install-smoke: install.sh
    # died here on `✗ [jtag-bin] source binary missing at .../src/jtag`).
    module_skip "jtag-bin" "old Node jtag CLI not present ($src) — use 'cu' / 'continuum'"
    return 0
  fi

  # Idempotency: existing symlink already points at this src.
  if [ -L "/usr/local/bin/jtag" ] && [ "$(readlink "/usr/local/bin/jtag")" = "$src" ]; then
    module_skip "jtag-bin" "/usr/local/bin/jtag already symlinked to $src"
    return 0
  fi
  if [ -L "$HOME/.local/bin/jtag" ] && [ "$(readlink "$HOME/.local/bin/jtag")" = "$src" ]; then
    module_skip "jtag-bin" "~/.local/bin/jtag already symlinked to $src"
    return 0
  fi

  # Tier 1: writable system path.
  if [ -w "/usr/local/bin" ]; then
    module_start "jtag-bin" "Symlinking jtag CLI → /usr/local/bin/jtag"
    ln -sf "$src" "/usr/local/bin/jtag" \
      || module_fail "jtag-bin" "ln -s to /usr/local/bin failed"
    module_done "jtag-bin"
    return 0
  fi

  # Tier 2: sudo with TTY.
  if command -v sudo &>/dev/null && [ -t 0 ]; then
    module_start "jtag-bin" "Symlinking jtag CLI → /usr/local/bin/jtag (needs sudo)"
    ensure_sudo_warmed
    sudo ln -sf "$src" "/usr/local/bin/jtag" \
      || module_fail "jtag-bin" "sudo ln -s to /usr/local/bin failed"
    module_done "jtag-bin"
    return 0
  fi

  # Tier 3: user-space fallback.
  module_start "jtag-bin" "Symlinking jtag CLI → ~/.local/bin/jtag (user-space fallback, no sudo)"
  mkdir -p "$HOME/.local/bin"
  ln -sf "$src" "$HOME/.local/bin/jtag" \
    || module_fail "jtag-bin" "ln -s to ~/.local/bin failed"
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) warn "~/.local/bin is not in your PATH. Add: export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
  esac
  module_done "jtag-bin"
}

# ── mod_tailscale_check ─────────────────────────────────────
# Tailscale powers cross-machine peer discovery + TLS for the grid
# story. Optional for pure-localhost installs but the install-time
# detect saves a debugging session later when "why can't BigMama see
# my widget" hits. No sudo. No install — just detect + nudge.
mod_tailscale_check() {
  if ! command -v tailscale &>/dev/null; then
    module_skip "tailscale" "not installed (optional — install from tailscale.com if you want grid/cross-machine)"
    return 0
  fi
  if ! tailscale status &>/dev/null; then
    module_start "tailscale" "Installed but not running — start it via 'sudo tailscale up' (one-time)"
    warn "Continuing — Continuum works locally without Tailscale."
    return 0
  fi
  local ts_ip; ts_ip=$(tailscale ip -4 2>/dev/null | head -1)
  module_skip "tailscale" "active at $ts_ip"
}

# ── mod_docker_check ────────────────────────────────────────
# Hard requirement for Carl path; useful sanity-check for Dev too. Three
# tiers: not installed → fail with install URL; installed-not-running →
# fail with start instructions; running but WSL not integrated →
# delegate to mod_docker_wsl_integration above.
mod_docker_check() {
  if ! command -v docker &>/dev/null; then
    case "${PLATFORM:-$(uname -s)}" in
      Linux|linux|wsl)
        module_fail "docker" "Not installed. Install: curl -fsSL https://get.docker.com | sh; then 'sudo usermod -aG docker \$USER'; log out + back in."
        ;;
      Darwin|macos)
        module_fail "docker" "Not installed. Install Docker Desktop: https://docker.com/products/docker-desktop or Rancher Desktop: https://rancherdesktop.io"
        ;;
      *)
        module_fail "docker" "Not installed. Install Docker for your platform."
        ;;
    esac
  fi
  # Daemon reachability: if WSL2 + Docker Desktop integration broken,
  # mod_docker_wsl_integration will repair it. Otherwise this is a
  # 'start Docker Desktop' nudge.
  if ! docker info &>/dev/null 2>&1; then
    if grep -qi microsoft /proc/version 2>/dev/null && [ -d /mnt/wsl/docker-desktop ]; then
      module_start "docker" "Daemon unreachable on WSL2 — delegating to mod_docker_wsl_integration"
      mod_docker_wsl_integration
      docker info &>/dev/null 2>&1 \
        || module_fail "docker" "Daemon still unreachable after WSL integration repair. Try 'wsl --shutdown' from Windows PowerShell, then re-run."
      return 0
    fi
    module_fail "docker" "Daemon not reachable. Start Docker Desktop / Rancher Desktop, then re-run."
  fi
  module_skip "docker" "$(docker version --format '{{.Server.Version}}' 2>/dev/null) reachable"
}

# ============================================================================
# Hardware detection + GPU/inference runtime decision (shared by Carl + Dev)
# ============================================================================
#
# Both install entry points (public `install.sh` curl-target, and
# `tools/scripts/install.sh` Dev/parallel-start target) call these functions
# so detection lives in ONE place. Yesterday's session surfaced a pattern:
# the same detection duplicated across both install scripts drifted out
# of sync. Centralizing here kills that drift.
#
# What "detection" produces (globals, no subshell needed by callers):
#   IC_PLATFORM   : macos | linux | wsl
#   IC_ARCH       : arm64 | x86_64 | other
#   IC_RAM_GB     : integer
#   IC_RAM_MIB    : integer (computed once, used for Docker VM sizing)
#   IC_GPU_KIND   : metal | cuda | vulkan | rocm | none
#   IC_GPU_NAME   : human-readable (e.g. "Apple Silicon", "NVIDIA RTX 5090")
#   IC_VRAM_GB    : integer (0 if unknown or unified-memory device)
#
# What "decision" produces:
#   IC_GPU_PATH   : dmr-metal | dmr-cuda | dmr-rocm | llama-vulkan | unsupported
#   IC_DMR_BACKEND   : vllm | llama.cpp | "" (when not DMR-path)
#   IC_DMR_GPU_FLAG  : cuda | rocm | "" (Mac's vllm-metal needs no flag)
#   IC_MODEL_TIER    : 4b   (universal default; higher tiers later)
#
# CPU path is INTENTIONALLY UNSUPPORTED here. Continuum's contract is
# GPU-always for chat (Metal on Mac, Vulkan/CUDA/ROCm elsewhere). When
# a future CPU adapter lands it will be its own IC_GPU_PATH value,
# gated on `CONTINUUM_ALLOW_CPU_INFERENCE=1`, and explicit in logs.

ic_detect_hardware() {
  # Platform
  case "$(uname -s)" in
    Darwin) IC_PLATFORM="macos" ;;
    Linux)
      if grep -qi microsoft /proc/version 2>/dev/null; then
        IC_PLATFORM="wsl"
      else
        IC_PLATFORM="linux"
      fi
      ;;
    MINGW*|MSYS*|CYGWIN*)
      # Native Windows under Git Bash / MSYS2 / Cygwin. uname -s returns
      # MINGW64_NT-10.0-... or similar. Bug-fixed 2026-04-24 — previously
      # fell through to "unknown", which caused install.sh to silently skip
      # the model pull (Carl's first chat then errored on missing models).
      IC_PLATFORM="windows"
      ;;
    *) IC_PLATFORM="unknown" ;;
  esac
  IC_ARCH="$(uname -m)"

  # RAM
  case "$IC_PLATFORM" in
    macos)
      IC_RAM_MIB=$(( $(sysctl -n hw.memsize) / 1048576 ))
      ;;
    linux|wsl)
      IC_RAM_MIB=$(awk '/^MemTotal:/ {printf "%d", $2/1024}' /proc/meminfo)
      ;;
    windows)
      # Git Bash inherits PowerShell's wmic / Get-CimInstance. wmic is the
      # most portable across Windows versions (Win10 + Win11). Total physical
      # memory in bytes → MiB.
      if command -v wmic >/dev/null 2>&1; then
        local total_bytes
        total_bytes="$(wmic computersystem get TotalPhysicalMemory /value 2>/dev/null | tr -d '\r' | awk -F= '/TotalPhysicalMemory=/{print $2}')"
        IC_RAM_MIB=$(( ${total_bytes:-0} / 1048576 ))
      else
        IC_RAM_MIB=0
      fi
      ;;
    *)
      IC_RAM_MIB=0
      ;;
  esac
  IC_RAM_GB=$(( IC_RAM_MIB / 1024 ))

  # GPU
  IC_GPU_KIND="none"
  IC_GPU_NAME=""
  IC_VRAM_GB=0

  case "$IC_PLATFORM" in
    macos)
      if sysctl -n machdep.cpu.brand_string 2>/dev/null | grep -qi apple; then
        IC_GPU_KIND="metal"
        IC_GPU_NAME="Apple Silicon"
        IC_VRAM_GB="$IC_RAM_GB"   # Apple unified memory — GPU shares with CPU
      fi
      ;;
    windows)
      # nvidia-smi.exe is on PATH for any machine with NVIDIA drivers
      # installed (system32). Vulkan via vulkaninfo.exe (Vulkan SDK or
      # bundled with most modern GPU drivers).
      if command -v nvidia-smi >/dev/null 2>&1 && nvidia-smi --query-gpu=name --format=csv,noheader >/dev/null 2>&1; then
        IC_GPU_KIND="cuda"
        IC_GPU_NAME="$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 | tr -d '\r')"
        local vram_mib="$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1 | tr -d '\r')"
        IC_VRAM_GB=$(( ${vram_mib:-0} / 1024 ))
      elif command -v vulkaninfo >/dev/null 2>&1 && vulkaninfo --summary 2>/dev/null | grep -q deviceName; then
        IC_GPU_KIND="vulkan"
        IC_GPU_NAME="$(vulkaninfo --summary 2>/dev/null | awk -F= '/deviceName/{gsub(/^[[:space:]]*/,"",$2);print $2;exit}' | tr -d '\r')"
      fi
      ;;
    linux|wsl)
      # nvidia-smi — easiest signal. Works on Linux + WSL2 when CUDA drivers installed.
      local smi=""
      if command -v nvidia-smi &>/dev/null; then
        smi="nvidia-smi"
      elif [ -f /usr/lib/wsl/lib/nvidia-smi ]; then
        smi="/usr/lib/wsl/lib/nvidia-smi"
      fi
      if [ -n "$smi" ] && "$smi" --query-gpu=name --format=csv,noheader >/dev/null 2>&1; then
        IC_GPU_KIND="cuda"
        IC_GPU_NAME="$("$smi" --query-gpu=name --format=csv,noheader 2>/dev/null | head -1)"
        local vram_mib="$("$smi" --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1)"
        IC_VRAM_GB=$(( vram_mib / 1024 ))
      elif command -v rocminfo &>/dev/null && rocminfo >/dev/null 2>&1; then
        IC_GPU_KIND="rocm"
        IC_GPU_NAME="$(rocminfo 2>/dev/null | awk '/Marketing Name:/{sub(/.*Name:[[:space:]]*/,"");print;exit}')"
        # rocm-smi VRAM parsing — best-effort, not all AMD stacks report uniformly
        if command -v rocm-smi &>/dev/null; then
          local vram_bytes="$(rocm-smi --showmeminfo vram 2>/dev/null | awk '/vram.*Total.*Memory/{for(i=1;i<=NF;i++)if($i~/^[0-9]+$/){print $i;exit}}')"
          IC_VRAM_GB=$(( ${vram_bytes:-0} / 1073741824 ))
        fi
      elif command -v vulkaninfo &>/dev/null && vulkaninfo --summary 2>/dev/null | grep -q deviceName; then
        # Vulkan-only case: GPU exists, no CUDA/ROCm drivers. Common on Intel
        # Arc / older AMD / mixed hardware. Use our vendored llama.cpp with
        # --features=vulkan (shipped natively, not through DMR which doesn't
        # expose a Vulkan flag).
        IC_GPU_KIND="vulkan"
        IC_GPU_NAME="$(vulkaninfo --summary 2>/dev/null | awk -F= '/deviceName/{gsub(/^[[:space:]]*/,"",$2);print $2;exit}')"
        # Vulkan VRAM query is nontrivial; leave IC_VRAM_GB=0 for now.
      fi
      ;;
  esac
}

ic_decide_gpu_path() {
  # Requires ic_detect_hardware to have run.
  case "$IC_PLATFORM:$IC_GPU_KIND" in
    macos:metal)
      IC_GPU_PATH="dmr-metal"
      IC_DMR_BACKEND="vllm"     # Docker Desktop bundles vllm-metal
      IC_DMR_GPU_FLAG=""         # --gpu not applicable on Desktop
      ;;
    linux:cuda|wsl:cuda)
      IC_GPU_PATH="dmr-cuda"
      IC_DMR_BACKEND="llama.cpp"
      IC_DMR_GPU_FLAG="cuda"
      ;;
    linux:rocm)
      IC_GPU_PATH="dmr-rocm"
      IC_DMR_BACKEND="llama.cpp"
      IC_DMR_GPU_FLAG="rocm"
      ;;
    linux:vulkan|wsl:vulkan|windows:vulkan)
      IC_GPU_PATH="llama-vulkan"
      IC_DMR_BACKEND=""   # not DMR; handled by continuum-core's llama adapter
      IC_DMR_GPU_FLAG=""
      ;;
    windows:cuda)
      # Native Windows + NVIDIA. Docker Desktop on Windows supports NVIDIA
      # passthrough via WSL2 backend; same DMR/llama.cpp path as linux:cuda.
      IC_GPU_PATH="dmr-cuda"
      IC_DMR_BACKEND="llama.cpp"
      IC_DMR_GPU_FLAG="cuda"
      ;;
    *)
      IC_GPU_PATH="unsupported"
      IC_DMR_BACKEND=""
      IC_DMR_GPU_FLAG=""
      ;;
  esac

  # Model tier from RAM. 4B Q4 GGUF is ~2.7GB on disk; needs ~6-8GB active.
  # Below 20GB physical RAM, Continuum can't run the full stack cleanly.
  if [ "$IC_RAM_GB" -lt 20 ]; then
    IC_MODEL_TIER="too-small"
  else
    IC_MODEL_TIER="4b"
  fi
}

# One-liner for humans — prints the detected + decided state for logging.
ic_describe_hardware() {
  printf '  Platform:   %s (%s)\n'        "$IC_PLATFORM" "$IC_ARCH"
  printf '  RAM:        %sGB\n'           "$IC_RAM_GB"
  if [ -n "$IC_GPU_NAME" ]; then
    if [ "$IC_VRAM_GB" -gt 0 ]; then
      printf '  GPU:        %s (%s, %sGB VRAM)\n' "$IC_GPU_NAME" "$IC_GPU_KIND" "$IC_VRAM_GB"
    else
      printf '  GPU:        %s (%s)\n'    "$IC_GPU_NAME" "$IC_GPU_KIND"
    fi
  else
    printf '  GPU:        none detected\n'
  fi
  printf '  GPU path:   %s\n'             "${IC_GPU_PATH:-undecided}"
  printf '  Model tier: %s\n'             "${IC_MODEL_TIER:-undecided}"
}
