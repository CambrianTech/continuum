#!/bin/bash
# start-server.sh — headless Rust deploy. No Node, no TS, no widgets.
#
# Per Joel 2026-06-02: repeatable start matching the headless-Rust-canonical
# doctrine ([[headless-rust-is-canonical-many-uis-optional]] /
# [[rust-is-the-core-node-is-the-shell]]). Runs continuum-core-server
# directly via cargo run with the right per-platform features. The Node
# orchestrator stays out of the loop.
#
# Usage:
#   bash scripts/start-server.sh                    # cargo run (debug, fast iterate)
#   CONTINUUM_RELEASE=1 bash scripts/start-server.sh # release build
#   CONTINUUM_SOCKET=/path bash scripts/start-server.sh
#
# Env vars (all optional — substrate auto-discovers where possible):
#   CONTINUUM_SOCKET        Unix socket for the substrate's IPC. Default
#                           /tmp/continuum-core.sock. Removed if stale.
#   CONTINUUM_RELEASE       Set non-empty for --release build.
#   AIRC_DAEMON_SOCKET      Explicit airc daemon socket. Otherwise the
#                           substrate auto-discovers via `airc ipc-endpoint`.
#   AIRC_DEFAULT_CHANNEL    Override default room. Otherwise derived from
#                           `airc room` output.
#   AIRC_DEFAULT_ROOM_NAME  Override default room name. Otherwise from
#                           `airc room`.

set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# ── PATH + config ────────────────────────────────────────────────────
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
[ -f "$HOME/.continuum/config.env" ] && { set -a; source "$HOME/.continuum/config.env"; set +a; }

if [ -z "$ORT_DYLIB_PATH" ]; then
  if [ -f "$HOME/.continuum/lib/libonnxruntime.so" ]; then
    export ORT_DYLIB_PATH="$HOME/.continuum/lib/libonnxruntime.so"
  elif [ -f "/opt/homebrew/lib/libonnxruntime.dylib" ]; then
    export ORT_DYLIB_PATH="/opt/homebrew/lib/libonnxruntime.dylib"
  fi
fi

# ── Per-platform feature flags ───────────────────────────────────────
# Mac Intel can't use Metal (task #131 — ggml_metal_device_init hangs on
# Intel + AMD discrete). Force mac-cpu-only on Intel Mac.
case "$(uname -sm)" in
  "Darwin x86_64")
    CONTINUUM_FEATURES="--no-default-features --features livekit-webrtc,llama/mac-cpu-only"
    ;;
  "Darwin arm64")
    CONTINUUM_FEATURES="--features metal,accelerate"
    ;;
  *)
    # Source the existing detector for Linux/Windows.
    source "$SCRIPT_DIR/shared/cargo-features.sh"
    CONTINUUM_FEATURES="$CARGO_GPU_FEATURES"
    ;;
esac

# ── Airc context ─────────────────────────────────────────────────────
# Substrate auto-discovers airc daemon socket via `airc ipc-endpoint`
# (task #80). The default room/channel come from `airc room` so the
# personas land in the same scope Joel's terminal sees.
if [ -z "$AIRC_DEFAULT_CHANNEL" ] || [ -z "$AIRC_DEFAULT_ROOM_NAME" ]; then
  if airc status >/dev/null 2>&1; then
    ROOM_OUT="$(airc room 2>/dev/null || true)"
    if [ -z "$AIRC_DEFAULT_ROOM_NAME" ]; then
      export AIRC_DEFAULT_ROOM_NAME="$(awk '/^room:/{print $2}' <<<"$ROOM_OUT")"
    fi
    if [ -z "$AIRC_DEFAULT_CHANNEL" ]; then
      export AIRC_DEFAULT_CHANNEL="$(awk '/^channel:/{print $2}' <<<"$ROOM_OUT")"
    fi
  else
    echo "⚠  airc daemon not running. Start it with: airc daemon" >&2
    echo "   continuum-core-server will still launch but personas can't talk." >&2
  fi
fi

# Auto-derive airc daemon socket from the running daemon process if the
# binary doesn't expose `airc ipc-endpoint` yet (task #79 in flight).
# Substrate prefers `airc ipc-endpoint` per task #80's discoverer; this is
# the fallback when the airc binary predates that subcommand.
if [ -z "$AIRC_DAEMON_SOCKET" ]; then
  # airc's per-machine persistent daemon socket lives at
  # ~/.airc/runtime/airc-machine-*-v5.sock. Other airc-*-v5.sock files
  # are session-scoped (per-Claude-session, etc) and not what the
  # substrate wants to attach to. Pick the most recently modified
  # machine socket — that's the live daemon.
  AIRC_DAEMON_SOCKET="$(
    ls -1t "$HOME"/.airc/runtime/airc-machine-*-v5.sock 2>/dev/null \
      | grep -v '\.lock$' \
      | head -1
  )"
  if [ -n "$AIRC_DAEMON_SOCKET" ]; then
    export AIRC_DAEMON_SOCKET
    echo "ℹ  AIRC_DAEMON_SOCKET auto-derived: $AIRC_DAEMON_SOCKET" >&2
  fi
fi

# ── Socket ───────────────────────────────────────────────────────────
CONTINUUM_SOCKET="${CONTINUUM_SOCKET:-/tmp/continuum-core.sock}"
rm -f "$CONTINUUM_SOCKET"

# ── Launch ───────────────────────────────────────────────────────────
PROFILE_FLAG=""
PROFILE_LABEL="debug"
if [ -n "$CONTINUUM_RELEASE" ]; then
  PROFILE_FLAG="--release"
  PROFILE_LABEL="release"
fi

# Shared cargo target dir — every continuum invocation across every
# worktree drains into ONE cache instead of N copies of the same
# dependency tree. Pre-this-fix a single continuum-core debug build
# with metal+accelerate produced ~42 GB of target/ artifacts per
# worktree, with most of the cost being identical compiled deps
# (llama.cpp, whisper.cpp, candle, bevy). 14 worktrees × duplicate
# deps == 99% disk at $466 GB.
#
# CARGO_TARGET_DIR is the canonical cargo lever for this — setting
# it env-side means EVERY cargo invocation (build, test, run, check)
# from this script and anything it spawns picks it up. `~` is not
# expanded by cargo's [env] config so we set it in the wrapper
# script where shell expansion runs.
#
# Operators can still get per-worktree isolation for a one-shot
# build by exporting `CARGO_TARGET_DIR=<somewhere>` BEFORE invoking
# this script; the `:-` fallback below honors that override.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.continuum/cache/cargo-target}"
mkdir -p "$CARGO_TARGET_DIR"

echo "▶ continuum-core-server starting"
echo "  profile:    $PROFILE_LABEL"
echo "  features:   $CONTINUUM_FEATURES"
echo "  socket:     $CONTINUUM_SOCKET"
echo "  target dir: $CARGO_TARGET_DIR"
echo "  airc:       room=${AIRC_DEFAULT_ROOM_NAME:-?} channel=${AIRC_DEFAULT_CHANNEL:-?}"
echo ""

cd "$PROJECT_DIR/workers/continuum-core"
exec cargo run --bin continuum-core-server $PROFILE_FLAG $CONTINUUM_FEATURES -- "$CONTINUUM_SOCKET"
