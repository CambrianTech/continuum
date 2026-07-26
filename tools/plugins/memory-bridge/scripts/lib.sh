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
