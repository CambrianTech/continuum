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

# Resolve the airc BINARY, the same way `resolve_continuum` above resolves ours.
#
# Hooks do NOT inherit the operator's interactive shell. They run under whatever
# environment spawned the agent runtime, and `airc` installs to `~/.local/bin` —
# routinely absent from that PATH. Measured 2026-08-09 on BigMama: `airc status`
# answered instantly in every terminal (daemon at 21h uptime, 1305/1305 acked)
# while the session-recall hook wrote `persona id unresolved (airc status down)`
# for two sessions running. The probe never observed the daemon at all; bare
# `airc` was not a program it could find, and `2>/dev/null` swallowed the
# "command not found" that would have said so.
#
# So engram recall was silently off for every session on this machine, which is
# precisely the invisible-death this bridge exists to prevent — arriving through
# the resolver instead of the daemon. Two answers to "find a binary" in ONE file,
# only one of them robust, is the drift; now there is one shape.
resolve_airc() {
  if [ -n "${AIRC_BIN:-}" ] && [ -x "${AIRC_BIN}" ]; then
    printf '%s' "${AIRC_BIN}"
    return 0
  fi
  if command -v airc >/dev/null 2>&1; then
    command -v airc
    return 0
  fi
  local candidate
  # `.exe` variants matter: on Windows the installed binary is airc.exe, and a
  # bare-name `-x` test does not find it. [[dir-opened-as-file-windows-only]]
  for candidate in "$HOME/.local/bin/airc" "$HOME/.local/bin/airc.exe" \
                   "$HOME/.cargo/bin/airc" "$HOME/.cargo/bin/airc.exe"; do
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
  local live cached="$BRIDGE_STATE_DIR/persona-id" airc_bin
  if airc_bin="$(resolve_airc)"; then
    live="$("$airc_bin" status 2>/dev/null | awk '/^peer_id:/{print $2; exit}')"
  else
    live=""
  fi
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

# WHY resolve_agent_persona failed, for the caller's receipt.
#
# A separate function that RE-DERIVES rather than a variable set inside
# `resolve_agent_persona`: callers invoke it as `$(resolve_agent_persona)`, a
# command substitution, which is a SUBSHELL — anything it assigns dies with it.
# The first version of this fix did exactly that and the receipt came out blank;
# the negative test caught it. Same subshell trap as piping `source`.
# [[absence-rendered-as-positive-fact]]
#
# Re-deriving is cheap (one `command -v`, a few `-x` tests) and only ever runs on
# the failure path, where a fraction of a millisecond buys a receipt that names
# the actual cause instead of guessing at the daemon's health.
#
# "No airc binary" and "airc ran and reported nothing" are different types, not
# two values of one type. Conflating them is what wrote `airc status down` into
# two sessions' receipts about a daemon at 21h uptime.
persona_failure_reason() {
  local airc_bin
  if ! airc_bin="$(resolve_airc)"; then
    printf 'no airc binary on PATH or in ~/.local/bin, ~/.cargo/bin (set AIRC_BIN to pin it)'
    return 0
  fi
  if [ -z "$("$airc_bin" status 2>/dev/null | awk '/^peer_id:/{print $2; exit}')" ]; then
    printf 'airc found at %s but it reported no peer_id — daemon down or not joined' "$airc_bin"
    return 0
  fi
  # Reached only if the id resolves NOW but did not a moment ago (a daemon that
  # came up in between). Say that, rather than inventing a cause.
  printf 'airc answers now (transient failure during the earlier probe)'
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
