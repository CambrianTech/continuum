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
# (lets a runtime pin identity); else derive from `airc status`; else the cache.
#
# The cache is load-bearing, not an optimization. `airc status` is a LIVE probe of
# a daemon that legitimately goes down (reboot, update, sleep). Without a fallback,
# every airc outage silently disables memory for the whole session — the exact
# invisible-death class this bridge exists to avoid. Identity does not change when
# the daemon restarts, so a once-resolved id stays valid; we re-cache on every live
# success so a genuinely new identity supersedes it.
BRIDGE_STATE_DIR="${CONTINUUM_MEMORY_BRIDGE_DIR:-$HOME/.continuum/memory-bridge}"

resolve_agent_persona() {
  if [ -n "${CONTINUUM_AGENT_PERSONA:-}" ]; then
    printf '%s' "${CONTINUUM_AGENT_PERSONA}"
    return 0
  fi
  local live cached="$BRIDGE_STATE_DIR/persona-id"
  live="$(airc status 2>/dev/null | awk '/^peer_id:/{print $2; exit}')"
  if [ -n "${live:-}" ]; then
    mkdir -p "$BRIDGE_STATE_DIR" 2>/dev/null && printf '%s' "$live" > "$cached" 2>/dev/null
    printf '%s' "$live"
    return 0
  fi
  if [ -s "$cached" ]; then
    cat "$cached" 2>/dev/null
    return 0
  fi
  return 1
}

# bridge_receipt <hook> <status> [detail] — durable one-line JSONL receipt.
#
# The bridge's hooks MUST never break a session, so every failure path exits 0.
# That is correct AND it is how a memory system dies invisibly: "installed" and
# "working" become indistinguishable, for the user and for the agent. So every
# exit path — success and failure — leaves a receipt here, and session-recall
# reads them back so a broken bridge announces itself in the agent's own context.
# Best-effort by construction: a failure to write a receipt never fails a hook.
bridge_receipt() {
  local hook="${1:-?}" status="${2:-?}" detail="${3:-}"
  local log="$BRIDGE_STATE_DIR/receipts.jsonl"
  mkdir -p "$BRIDGE_STATE_DIR" 2>/dev/null || return 0
  # Keep the log bounded (every turn writes one line): trim to the last 500 on
  # roll-over. No new unbounded write path — CLAUDE.md's cache-class rule.
  if [ -f "$log" ] && [ "$(wc -l < "$log" 2>/dev/null || echo 0)" -gt 1000 ]; then
    tail -n 500 "$log" > "$log.trim" 2>/dev/null && mv -f "$log.trim" "$log" 2>/dev/null
  fi
  detail="${detail//\\/\\\\}"; detail="${detail//\"/\\\"}"; detail="${detail//$'\n'/ }"
  printf '{"ts":"%s","hook":"%s","status":"%s","detail":"%s"}\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$hook" "$status" "$detail" >> "$log" 2>/dev/null
  return 0
}
