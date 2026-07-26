#!/usr/bin/env bash
# lib.sh — shared helpers for the memory-bridge hooks/skills. Sourced, not run.
#
# ONE place to resolve the continuum CLI (compression principle): the binary is
# named `continuum` (NOT `cu` — that collides with the Unix UUCP tool on every
# mac/linux box, which silently shadowed it and made the whole bridge a no-op).
# It also may not be on PATH — the dev deploy builds into the shared cargo target
# under ~/.continuum, and install may not have symlinked it. Resolution order:
#   1. $CONTINUUM_BIN override (a runtime can pin the exact binary)
#   2. `continuum` on PATH (once install symlinks it, this wins)
#   3. known cargo-target build dirs — release first, then debug (the dev default)
# Prints the resolved path on stdout and returns 0, or returns 1 if not found.

resolve_continuum() {
  if [ -n "${CONTINUUM_BIN:-}" ] && [ -x "${CONTINUUM_BIN}" ]; then
    printf '%s' "${CONTINUUM_BIN}"
    return 0
  fi
  if command -v continuum >/dev/null 2>&1; then
    command -v continuum
    return 0
  fi
  local target="${CARGO_TARGET_DIR:-$HOME/.continuum/cache/cargo-target}"
  local candidate
  for candidate in "$target/release/continuum" "$target/debug/continuum"; do
    if [ -x "$candidate" ]; then
      printf '%s' "$candidate"
      return 0
    fi
  done
  return 1
}

# Resolve the agent's persona id (its airc peer id). $CONTINUUM_AGENT_PERSONA wins
# (lets a runtime pin identity); else derive from `airc status`. Prints it or nothing.
resolve_agent_persona() {
  if [ -n "${CONTINUUM_AGENT_PERSONA:-}" ]; then
    printf '%s' "${CONTINUUM_AGENT_PERSONA}"
    return 0
  fi
  airc status 2>/dev/null | awk '/^peer_id:/{print $2; exit}'
}

# The agent's memory lives IN the substrate: if the core is down at wakeup, recall
# comes back empty and the agent wakes AMNESIAC — the exact pain after a machine
# restart (the core wasn't back up yet). This waits, BRIEFLY and bounded, for the
# core to answer `ping`, so an agent waking DURING a (supervised) restart window
# still recalls its memory instead of forgetting.
#
# It deliberately does NOT start the core. Bringing the substrate up is the
# SUPERVISOR's job ([[system-owns-its-lifecycle-never-hand-manage-processes]] —
# install-service.sh); `continuum start` REBUILDS, far too heavy for a session
# hook. If the core is genuinely down (unsupervised cold box), the caller degrades
# gracefully rather than block the session — a memory system must never break one.
#
# $1 = resolved continuum binary. Budget: $CONTINUUM_WAKE_WAIT_SECS (default 4),
# well under the 10s SessionStart hook timeout. Returns 0 as soon as the core
# answers; 1 if it stays down through the budget.
wait_for_core() {
  local cont="$1" budget="${CONTINUUM_WAKE_WAIT_SECS:-4}" waited=0
  "$cont" ping >/dev/null 2>&1 && return 0
  while [ "$waited" -lt "$budget" ]; do
    sleep 1
    waited=$((waited + 1))
    "$cont" ping >/dev/null 2>&1 && return 0
  done
  return 1
}
