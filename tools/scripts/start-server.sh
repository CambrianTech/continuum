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
# Repo root is two up from tools/scripts/. (Was `dirname SCRIPT_DIR`, which
# resolved to tools/ — stale since this script moved under tools/scripts/.)
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
# continuum-core crate manifest. Restructured workers/continuum-core →
# core/continuum-core (commit 2cb63e019); cwd-independent --manifest-path so the
# headless start works from any directory.
CORE_MANIFEST="$REPO_ROOT/core/continuum-core/Cargo.toml"

# ── PATH + config ────────────────────────────────────────────────────
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
[ -f "$HOME/.continuum/config.env" ] && { set -a; source "$HOME/.continuum/config.env"; set +a; }

# Locate cargo deterministically. A background task / detached shell does NOT
# inherit the interactive PATH, so cargo (rustup at ~/.cargo/bin OR homebrew at
# /opt/homebrew/bin) may be invisible. Prepend the known install dirs, then
# require cargo to exist — fail LOUD rather than printing "core still launches"
# and then dying at `exec cargo` (the silent-fallthrough this script did on
# 2026-06-22). [[fallbacks-are-illegal-fail-loud]].
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ FATAL: cargo not found on PATH (looked in ~/.cargo/bin, /opt/homebrew/bin," >&2
  echo "  /usr/local/bin, and the inherited PATH). Install Rust, then re-run." >&2
  exit 1
fi

# ── Single-owner build target ────────────────────────────────────────
# This script is the ONE start path ([[validate-via-pure-rust-not-npm-jtag]]).
# It must therefore own CARGO_TARGET_DIR so every `cu start` — no matter which
# shell or background task invokes it — builds into and runs from the SAME
# binary. Without this, a shell that lacks the export builds a 396MB ghost into
# the repo's ./target while another shell ran from ~/.continuum/cache, leaving
# two diverging continuum-core-server processes fighting over one socket (the
# "more than one shell running" / 18GB ghost-target incident, 2026-06-22).
# An explicit per-shell export still wins (deliberate one-shot against a clean
# target); we only supply the default so the unattended path can't diverge.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.continuum/cache/cargo-target}"

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

# ── Unsloth Studio ───────────────────────────────────────────────────
# Unsloth Studio is the inference + training gateway. All persona inference
# routes through it (/v1 OpenAI-compatible API). The core will fail loud on
# the first LLM call if Studio isn't up — catch it here so the error lands
# at startup, not buried in a per-persona log [[fallbacks-are-illegal-fail-loud]].
UNSLOTH_HOST="${UNSLOTH_BASE_URL:-http://127.0.0.1:8888}"
UNSLOTH_PORT="${UNSLOTH_HOST##*:}"      # extract port from base URL
UNSLOTH_PORT="${UNSLOTH_PORT%%/*}"      # strip any trailing path

if curl -sf "${UNSLOTH_HOST}/health" >/dev/null 2>&1 \
   || curl -sf "${UNSLOTH_HOST}/v1/models" >/dev/null 2>&1; then
  echo "✓ Unsloth Studio is running at ${UNSLOTH_HOST}" >&2
else
  # Not up. Try to start it in the background.
  UNSLOTH_BIN="$HOME/.local/bin/unsloth"
  if [ -x "$UNSLOTH_BIN" ] && [ -n "$UNSLOTH_MODEL" ]; then
    echo "▶ Starting Unsloth Studio (model: $UNSLOTH_MODEL, port: $UNSLOTH_PORT) …" >&2
    nohup "$UNSLOTH_BIN" studio run \
      --model "$UNSLOTH_MODEL" \
      --port "$UNSLOTH_PORT" \
      --no-cloudflare \
      >/tmp/unsloth-studio.log 2>&1 &
    UNSLOTH_PID=$!
    echo "  PID $UNSLOTH_PID  —  log: /tmp/unsloth-studio.log" >&2
    # Wait up to 30 seconds for the server to accept connections.
    WAITED=0
    until curl -sf "${UNSLOTH_HOST}/v1/models" >/dev/null 2>&1; do
      sleep 1; WAITED=$((WAITED+1))
      if [ $WAITED -ge 30 ]; then
        echo "✗ FATAL: Unsloth Studio did not come up after 30s." >&2
        echo "  Start it manually:  unsloth studio run --model \$UNSLOTH_MODEL" >&2
        echo "  Then retry:  cu start" >&2
        exit 1
      fi
    done
    echo "✓ Unsloth Studio ready (${WAITED}s)." >&2
  else
    echo "" >&2
    echo "✗ FATAL: Unsloth Studio is not running and cannot be auto-started." >&2
    echo "  UNSLOTH_BASE_URL = ${UNSLOTH_HOST}" >&2
    if [ -z "$UNSLOTH_MODEL" ]; then
      echo "  UNSLOTH_MODEL is not set in ~/.continuum/config.env" >&2
    fi
    if [ ! -x "$UNSLOTH_BIN" ]; then
      echo "  unsloth binary not found at $UNSLOTH_BIN (install: https://unsloth.ai)" >&2
    fi
    echo "" >&2
    echo "  Start it manually:  unsloth studio run --model <gguf-path> --api-only" >&2
    echo "  Then retry:  cu start" >&2
    echo "" >&2
    exit 1
  fi
fi

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

# ── Build the continuum-mcp bin ──────────────────────────────────────
# The MCP server is a separate stdio bin that MCP clients (unsloth Studio,
# Claude Code) SPAWN — it isn't launched by us, so it must exist on disk after
# `npm start`. Build it here (same crate/manifest/features/profile as the core,
# so it's a fast incremental once the core is built) rather than via a raw
# `cargo build` — all Rust bins build through the npm start path. It replaces
# the Node `src/mcp-server.ts`; an MCP client config points at the built binary.
echo "▶ building continuum-mcp (Rust MCP server bin)"
cargo build --manifest-path "$CORE_MANIFEST" --bin continuum-mcp $PROFILE_FLAG $CONTINUUM_FEATURES \
  || echo "⚠ continuum-mcp build failed — MCP server unavailable (core still launches)" >&2

# ── Build the cu CLI client ──────────────────────────────────────────
# `cu` is the pure-Rust CLI client (replaces the Node `./jtag`): `cu ping`,
# `cu <command> [json]` over the core IPC socket via the uniform Connection.
# Built here so the headless start produces the client on disk too.
echo "▶ building cu (Rust CLI client)"
cargo build --manifest-path "$CORE_MANIFEST" --bin cu $PROFILE_FLAG $CONTINUUM_FEATURES \
  || echo "⚠ cu build failed — CLI client unavailable (core still launches)" >&2

echo "▶ continuum-core-server starting"
echo "  profile:  $PROFILE_LABEL"
echo "  features: $CONTINUUM_FEATURES"
echo "  socket:   $CONTINUUM_SOCKET"
echo "  airc:     room=${AIRC_DEFAULT_ROOM_NAME:-?} channel=${AIRC_DEFAULT_CHANNEL:-?}"
echo ""

exec cargo run --manifest-path "$CORE_MANIFEST" --bin continuum-core-server $PROFILE_FLAG $CONTINUUM_FEATURES -- "$CONTINUUM_SOCKET"
