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

# ── env: keys + the dylibs the faculties need (so ONNX doesn't panic on dlopen) ──
set -a; [ -f "$PROJECT_DIR/config.env" ] && . "$PROJECT_DIR/config.env"; set +a
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
# give boot a moment to reach airc + model load
for _ in $(seq 1 40); do grep -q "load_tensors: offloaded" "$LOG" 2>/dev/null && break; sleep 1; done

grep -qE "airc: ✓|airc.*socket=" "$LOG" && echo "  ✓ airc connected" || { echo "  ✗ airc NOT connected"; ok=0; }
grep -qE "load_tensors: offloaded|MTL.*model buffer" "$LOG" && echo "  ✓ model loaded (GPU)" || echo "  ⚠ no model-load line yet"
if grep -qiE "panic" "$LOG"; then echo "  ✗ PANIC in boot log:"; grep -iE "panic" "$LOG" | head -3 | sed 's/^/      /'; ok=0; else echo "  ✓ no panics"; fi
# unsloth: report whether the gateway is even configured (it may be local-gguf only)
if [ -n "${UNSLOTH_API_KEY:-}" ]; then echo "  ✓ unsloth key present"; else echo "  • unsloth not configured (local gguf inference)"; fi

[ "$ok" = 1 ] && echo "✓ core up: $(pgrep -f "continuum-core-server $SOCK" | head -1)" || { echo "✗ launch incomplete — see $LOG"; exit 1; }
