#!/usr/bin/env bash
# ensure-docker.sh — make "Docker is up" dependable.
#
# Continuum's grid IS Docker-based (sandboxed single-machine clusters, the
# AWS-containers / k8s shape), so a wedged Docker engine isn't an
# inconvenience — it's the grid being down. This preflight probes the
# daemon and, if it's not responding, RECOVERS it, then verifies. Loud and
# bounded throughout: a hung daemon can never stall the caller, and a
# genuine failure exits non-zero with the exact state + next step.
#
# The Windows recovery is the recipe proven by hand on BIGMAMA, where
# Docker Desktop's GUI/backend processes hang such that a normal
# restart/close "does nothing" (they don't respond) — only a force-kill +
# clean WSL cycle + fresh relaunch clears it.
#
# Usage:
#   scripts/ensure-docker.sh            # probe; recover if down
#   PROBE_TIMEOUT=8 BOOT_WAIT=120 scripts/ensure-docker.sh
#   scripts/ensure-docker.sh --probe-only   # exit 0/1 on liveness, no recovery
#
# Exit: 0 = Docker responding (already, or after recovery); 1 = still down.

set -uo pipefail

PROBE_TIMEOUT="${PROBE_TIMEOUT:-8}"   # seconds; a wedged `docker` hangs, so cap it
BOOT_WAIT="${BOOT_WAIT:-120}"         # seconds to wait for the engine after a recovery kick
POLL="${POLL:-4}"                     # seconds between post-recovery probes

log() { printf 'ensure-docker: %s\n' "$*" >&2; }

_os() {
  case "$(uname -s 2>/dev/null)" in
    MINGW*|MSYS*|CYGWIN*) echo windows ;;
    Darwin)               echo macos ;;
    Linux)                echo linux ;;
    *)                    echo unknown ;;
  esac
}

# Bounded liveness probe. A wedged daemon makes `docker version` hang
# indefinitely, so we run it in the background and reap it after
# PROBE_TIMEOUT. Deliberately NOT using `timeout(1)` — it's absent on stock
# macOS, and depending on it would defeat the point of being dependable.
_docker_responds() {
  ( docker version --format '{{.Server.Version}}' >/dev/null 2>&1 ) &
  local pid=$! waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$PROBE_TIMEOUT" ]; then
      kill -TERM "$pid" 2>/dev/null
      sleep 1
      kill -KILL "$pid" 2>/dev/null
      wait "$pid" 2>/dev/null || true
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  wait "$pid"
}

_server_version() {
  # Best-effort, also bounded via the probe convention; empty on failure.
  ( docker version --format '{{.Server.Version}}' 2>/dev/null ) &
  local pid=$! waited=0 out
  while kill -0 "$pid" 2>/dev/null; do
    [ "$waited" -ge "$PROBE_TIMEOUT" ] && { kill -KILL "$pid" 2>/dev/null; break; }
    sleep 1; waited=$((waited + 1))
  done
  wait "$pid" 2>/dev/null || true
}

_recover_windows() {
  log "Docker Desktop appears hung — force-killing its processes (a normal restart can't clear a hung process)…"
  # Force-kill the USER-owned GUI/backend processes. com.docker.service and
  # wslservice are privileged (need elevation) — we skip them; a fresh
  # Docker Desktop launch drives the already-installed service itself.
  powershell.exe -NoProfile -NonInteractive -Command '
    foreach ($n in @("Docker Desktop","com.docker.backend","com.docker.build","com.docker.dev-envs","com.docker.cli","vpnkit","dockerd")) {
      Get-Process -Name $n -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    }' >/dev/null 2>&1 || true

  log "wsl --shutdown (clean engine-distro state)…"
  wsl.exe --shutdown >/dev/null 2>&1 || true

  log "relaunching Docker Desktop…"
  local dd
  for dd in \
    "/c/Program Files/Docker/Docker/Docker Desktop.exe" \
    "$LOCALAPPDATA/Docker/Docker Desktop.exe" \
    "/c/Program Files/Docker/Docker/frontend/Docker Desktop.exe"; do
    if [ -n "${dd:-}" ] && [ -x "$dd" ]; then
      ( "$dd" >/dev/null 2>&1 & )
      return 0
    fi
  done
  # Fall back to letting Windows resolve the install path.
  powershell.exe -NoProfile -NonInteractive -Command 'Start-Process "Docker Desktop"' >/dev/null 2>&1 \
    || log "could not locate Docker Desktop.exe — launch it manually"
}

_recover_macos() {
  log "(re)launching Docker Desktop…"
  pkill -f "Docker Desktop" >/dev/null 2>&1 || true   # clear a possible hang
  open -a Docker >/dev/null 2>&1 \
    || open -a "Docker Desktop" >/dev/null 2>&1 \
    || log "could not open the Docker app — launch it manually"
}

_recover_linux() {
  log "starting the docker daemon…"
  if command -v systemctl >/dev/null 2>&1; then
    sudo -n systemctl start docker >/dev/null 2>&1 \
      || systemctl --user start docker >/dev/null 2>&1 \
      || log "could not start docker (try: sudo systemctl start docker)"
  else
    log "no systemctl found — start your docker daemon manually (e.g. 'sudo service docker start')"
  fi
}

_diagnose_windows() {
  log "WSL distro state (docker-desktop should be 'Running'):"
  wsl.exe -l -v 2>/dev/null | tr -d '\000' | sed 's/^/  /' >&2 || true
  log "next: open Docker Desktop manually; if it stays stuck, Troubleshoot (🐞) → Restart, then Reset to factory defaults."
}

main() {
  if [ "${1:-}" = "--probe-only" ]; then
    if _docker_responds; then log "Docker is up (server $(_server_version))"; exit 0; fi
    log "Docker not responding"; exit 1
  fi

  if _docker_responds; then
    log "Docker already up (server $(_server_version)) — nothing to do."
    return 0
  fi

  local os; os="$(_os)"
  log "Docker not responding — attempting recovery on ${os}…"
  case "$os" in
    windows) _recover_windows ;;
    macos)   _recover_macos ;;
    linux)   _recover_linux ;;
    *)       log "unknown OS — cannot auto-recover Docker"; return 1 ;;
  esac

  log "waiting up to ${BOOT_WAIT}s for the engine to come up…"
  local waited=0
  while [ "$waited" -lt "$BOOT_WAIT" ]; do
    if _docker_responds; then
      log "Docker is up after recovery (server $(_server_version)). ✔"
      return 0
    fi
    sleep "$POLL"
    waited=$((waited + POLL))
    log "…still booting (${waited}s/${BOOT_WAIT}s)"
  done

  log "ERROR: Docker still not responding after ${BOOT_WAIT}s of recovery."
  [ "$os" = "windows" ] && _diagnose_windows
  return 1
}

main "$@"
