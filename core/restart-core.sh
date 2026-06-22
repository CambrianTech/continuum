#!/usr/bin/env bash
# restart-core.sh — the ONE reliable, quick core restart.
#
# Stop → update (build) → relaunch with the FULL env → verify every aspect.
# This is the single source of truth for "bring the headless core up correctly"
# (the quick core-only path; start-workers.sh remains the full stack incl LiveKit).
# We want reliable starts to be automatic and fully checkable — so this script both
# launches AND verifies (socket, airc, ONNX, model, persona, unsloth) and exits
# non-zero if a load-bearing aspect failed.
#
# Usage: core/restart-core.sh [--skip-build]
set -uo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOCK="/tmp/continuum-core.sock"
LOG="/tmp/core.log"
export PATH="/opt/homebrew/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.continuum/cache/cargo-target}"
BIN="$CARGO_TARGET_DIR/debug/continuum-core-server"

# ── env: ONLY non-secret runtime vars. Secrets have ONE owner: ~/.continuum/config.env,
# which the core reads DIRECTLY (secrets.rs / config_env.rs). We do NOT source any
# config.env into the process env — that would be a second place for the same keys
# (the repo config.env is a duplicate smell; see task #34). [[config-env-single-owner]]
if [ -f /opt/homebrew/lib/libonnxruntime.dylib ]; then
  export ORT_DYLIB_PATH="/opt/homebrew/lib/libonnxruntime.dylib"
elif [ -f /usr/local/lib/libonnxruntime.so ]; then
  export ORT_DYLIB_PATH="/usr/local/lib/libonnxruntime.so"
fi

# ── stop the running core (preserve identity/engrams; just the process) ──
pkill -f "continuum-core-server $SOCK" 2>/dev/null && sleep 2
pkill -9 -f "continuum-core-server $SOCK" 2>/dev/null
rm -f "$SOCK"

# ── update: incremental build (skippable when the caller already built) ──
if [[ " $* " != *" --skip-build "* ]]; then
  echo "▸ building core (metal,accelerate)…"
  if ! (cd "$PROJECT_DIR/core" && cargo build -q -p continuum-core \
        --bin continuum-core-server --features metal,accelerate); then
    echo "✗ build failed" >&2; exit 1
  fi
fi

# ── relaunch (detached; cwd = project root so relative paths resolve) ──
echo "▸ launching core…"
( cd "$PROJECT_DIR" && nohup "$BIN" "$SOCK" >"$LOG" 2>&1 & )

# ── verify: every load-bearing aspect, fail loud ──
echo "▸ verifying launch…"
ok=1
for _ in $(seq 1 90); do [ -S "$SOCK" ] && break; sleep 1; done
if [ -S "$SOCK" ]; then echo "  ✓ socket up ($SOCK)"; else echo "  ✗ socket never appeared"; ok=0; fi
# wait for the intentional inference boot_status line (the authoritative signal)
for _ in $(seq 1 60); do grep -q "\] inference: " "$LOG" 2>/dev/null && break; sleep 1; done

grep -qE "airc: ✓|airc.*socket=" "$LOG" && echo "  ✓ airc connected" || { echo "  ✗ airc NOT connected"; ok=0; }
if grep -qiE "panic" "$LOG"; then echo "  ✗ PANIC in boot log:"; grep -iE "panic" "$LOG" | head -3 | sed 's/^/      /'; ok=0; else echo "  ✓ no panics"; fi
# Inference path — the AUTHORITATIVE intentional signal (the boot_status assertion).
# ✗ = unsloth gateway not registered → fail loud. No guessing from env.
INF=$(grep -E "\] inference: " "$LOG" | tail -1)
case "$INF" in
  *✓*) echo "  ${INF##*] }";;
  *⚠*) echo "  ${INF##*] }";;
  *✗*) echo "  ${INF##*] }"; ok=0;;
  *)   echo "  ✗ no inference boot line — core did not assert an inference path"; ok=0;;
esac

[ "$ok" = 1 ] && echo "✓ core up: $(pgrep -f "continuum-core-server $SOCK" | head -1)" || { echo "✗ launch incomplete — see $LOG"; exit 1; }
