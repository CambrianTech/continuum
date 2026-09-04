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
#   bash scripts/start-server.sh                    # release build (the default)
#   CONTINUUM_DEBUG=1 bash scripts/start-server.sh  # debug build (fast iterate, live asserts)
#   CONTINUUM_SOCKET=/path bash scripts/start-server.sh
#
# Env vars (all optional — substrate auto-discovers where possible):
#   CONTINUUM_SOCKET        Unix socket for the substrate's IPC. Default
#                           /tmp/continuum-core.sock. Removed if stale.
#   CONTINUUM_DEBUG         Set non-empty for a debug build (release is the default).
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
# It must therefore own CARGO_TARGET_DIR so every `continuum start` — no matter which
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

# Launcher runtime PATH — manifest-driven. The install manifest
# (tools/scripts/install-manifest.toml) declares, per module, the directories the RUNNING
# binary needs on PATH to load its runtime DLLs/.so's (e.g. CUDA's cudart64_*.dll /
# cublas64_*.dll). Without this the native server on Windows is killed BEFORE main() with
# 0xC0000135 (STATUS_DLL_NOT_FOUND) — a silent zero-output exit. ONE declaration in the
# manifest is consumed by BOTH the installer (accept-check) and here (launch), so a fresh
# install is runnable by construction. We source the bash projection (the PS manifest is the
# installer's; both derive from the toml) and glob-expand each version-agnostic path.
# ── Build/runtime environment (ONE entry point) ──────────────────────────────
# Sets $_mf_os, prepends every manifest-declared runtime PATH dir (CUDA DLLs,
# etc), then imports the Windows MSVC/CUDA toolchain for the cargo build.
# These two were separate inline blocks ~200 lines apart until 2026-08-06, and
# the second SILENTLY depended on the first: its guard is `command -v nvcc`, and
# nvcc only reaches PATH via the manifest block. Extracting one without the
# other produced a shell where cargo built nothing and blamed a missing nvcc.
# shellcheck source=lib/windows-build-env.sh
source "$SCRIPT_DIR/lib/windows-build-env.sh"

# ── Per-platform feature flags ───────────────────────────────────────
# Mac Intel can't use Metal (task #131 — ggml_metal_device_init hangs on
# Intel + AMD discrete). Force mac-cpu-only on Intel Mac.
#
# CONTINUUM_CLI_FEATURES is the GPU-FREE set for the `continuum` CLI, which is a
# socket client and must never link a GPU runtime (see the CLI build below for why
# that made it unlaunchable on Windows). It is platform-shaped for the same reason
# the core's set is: a bare `--no-default-features` is NOT GPU-free-and-buildable
# everywhere. On macOS the unconditional `llama` dependency fires
#
#   compile_error!("llama crate built on macOS WITHOUT `--features metal`")
#
# so the plain flag has NEVER produced a CLI on a Mac — every `npm start` since it
# landed has hit the loud "⚠ GPU-free continuum build failed — retrying with the
# full feature set … Please report this" fallback, and shipped a GPU-linked CLI
# while reporting an anomaly nobody reported. `llama/mac-cpu-only` is that guard's
# OWN declared opt-in for a deliberately CPU-only build, which is exactly what a
# socket client wants.
case "$(uname -sm)" in
  "Darwin x86_64")
    CONTINUUM_FEATURES="--no-default-features --features livekit-webrtc,llama/mac-cpu-only"
    CONTINUUM_CLI_FEATURES="--no-default-features --features llama/mac-cpu-only"
    ;;
  "Darwin arm64")
    CONTINUUM_FEATURES="--features metal,accelerate"
    CONTINUUM_CLI_FEATURES="--no-default-features --features llama/mac-cpu-only"
    ;;
  *)
    # Source the existing detector for Linux/Windows.
    source "$SCRIPT_DIR/shared/cargo-features.sh"
    CONTINUUM_FEATURES="$CARGO_GPU_FEATURES"
    CONTINUUM_CLI_FEATURES="--no-default-features"
    ;;
esac

# ── llama-server (the inference engine the CORE owns) ────────────────
# llama.cpp's `llama-server` serves ONE model on /v1; the core's
# ServingDaemonModule OWNS its launch — "switch model" is a process relaunch
# the daemon performs, with `--alias` so the served id matches the registry id.
# There is NO Python Studio in the inference path anymore: a second server was a
# dual-owner race for the GPU (two copies of the same GGUF resident at once).
# This block's ONLY job is to make the core able to own that launch — it does
# NOT launch llama-server itself (the daemon does, after the core boots):
#   1. resolve the binary onto PATH so the daemon's `Command::new("llama-server")`
#      finds it (the core inherits this PATH as our child)
#   2. stop the excised Unsloth Studio so it isn't double-loading the GPU
# Robustness note: the core SCANS for a free serving port (it does not assume
# one), so a squatter never wedges it — stopping Studio is GPU/excision hygiene,
# not a correctness prerequisite [[llama-server-serves-v1-direct-python-gateway-optional]].

# (1) Resolve the binary WE OWN. The engine is ours now (unsloth is excised):
# we build llama-server from the llama.cpp submodule we vendor
# (core/vendor/llama.cpp — our fork) into ~/.continuum/bin via
# install-llama-server.sh. Resolution mirrors the core's own resolver
# (llama_server.rs::server_bin): LLAMA_SERVER_BIN override → our owned install →
# PATH. If our binary is absent (fresh clone / first run), BUILD it — that's
# ownership, not a fallback to someone else's build. Missing toolchain → the
# installer FAILs LOUD at the cause [[fallbacks-are-illegal-fail-loud]].
OWNED_BIN="${CONTINUUM_HOME:-$HOME/.continuum}/bin/llama-server"
if [ -n "${LLAMA_SERVER_BIN:-}" ] && [ -x "${LLAMA_SERVER_BIN}" ]; then
  # Explicit operator override — use verbatim, no sync (they own it).
  export PATH="$(dirname "${LLAMA_SERVER_BIN}"):$PATH"
else
  # Run the STAMP-GATED builder unconditionally — NOT only when the binary is missing.
  # install-llama-server.sh stamps the binary with the vendored-fork commit + backend and
  # skips instantly when it matches, but REBUILDS when the submodule moved. The old
  # `elif [ -x "$OWNED_BIN" ]` short-circuit used an EXISTING binary without checking the
  # stamp, so after a fork sync the serving binary silently drifted a month behind the
  # vendored lib — the daemon served with OLD llama-server (missing our cold-expert-ot /
  # get_tensor / upload_expert / MXFP4 patches) while continuum-core linked the NEW lib.
  # Always calling it is the llama-server twin of the #194 stale-check start-server already
  # does for continuum-core-server: one artifact, one fork, kept in lockstep by construction.
  if ! "$SCRIPT_DIR/install-llama-server.sh" >&2; then
    echo "⚠ install-llama-server.sh failed; falling back to any existing owned/PATH binary" >&2
  fi
  if [ -x "$OWNED_BIN" ]; then
    export PATH="$(dirname "$OWNED_BIN"):$PATH"
  fi
fi
if ! command -v llama-server >/dev/null 2>&1; then
  echo "" >&2
  echo "✗ FATAL: llama-server binary not found and could not be built." >&2
  echo "  The core's serving daemon needs it to bring up the inference engine." >&2
  echo "  Build it: tools/scripts/install-llama-server.sh" >&2
  echo "  (or set LLAMA_SERVER_BIN in ~/.continuum/config.env to an existing one)." >&2
  echo "" >&2
  exit 1
fi
echo "✓ llama-server: $(command -v llama-server) — the engine we own & launch" >&2

# ── BOOT OWNS THE PROCESS TREE (#452) ────────────────────────────────
# Joel, 2026-08-16: "start = enumerate → health-check → reap-or-ADOPT → spawn
# missing, for EVERY service — cores, llama lanes, airc daemon."
#
# The operative word is ADOPT. Boot used to do exactly two things to a service it
# found running: nothing (airc — a printed runbook line) or kill it (llama lanes —
# an unconditional pkill). Neither is ownership. A service that is already healthy
# should be KEPT; only an unhealthy one is reaped; only a missing one is spawned.
#
# `bounded_run` is the shared primitive all three rows need. macOS ships no
# coreutils `timeout`, and — the lesson that made #420's guard reachable — an
# unbounded probe against a WEDGED service does not fail, it HANGS: the kernel
# completes connect() into the listen backlog whether or not the process is ever
# scheduled, and the read then waits forever. A health check that can hang is not
# a health check; it is a boot that stops.
#
# Args: 1=budget seconds, 2+=command. Exit 0 iff the command exited 0 in time;
# 124 on timeout (the same code coreutils `timeout` uses, so callers read alike).
bounded_run() {
  local budget="$1"; shift
  "$@" >/dev/null 2>&1 &
  local pid=$! waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$((budget * 10))" ]; then
      kill -9 "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      return 124
    fi
    sleep 0.1
    waited=$((waited + 1))
  done
  wait "$pid"
}

# ── ROW: foreign inference servers ───────────────────────────────────
# Unsloth Studio is the EXCISED gateway — there is no healthy state for it to be
# adopted into, so it is reaped unconditionally. It is stopped before its backend
# because the parent would otherwise respawn one.
if pgrep -f 'studio run' >/dev/null 2>&1; then
  echo "  reaping excised Unsloth Studio (freeing GPU for the core's engine)" >&2
  pkill -f 'studio run' 2>/dev/null || true
fi

# ── ROW: llama lanes — ADOPT the healthy one ─────────────────────────
# This used to be `pkill -f llama-server`, unconditionally, on every boot. That
# was the #452 violation with the highest cost, and it made real code dead:
# `inference::lane_registry::sweep_in` already encodes the adopt rule — the
# `(LaneRole::Live, SweepMode::Boot) => false` arm deliberately LEAVES a live lane
# alone at boot and reaps it only at shutdown. The shell killed that lane seconds
# before the core could adopt it, so the Rust arm never once fired in production.
#
# What it cost: a cold model load on every single reboot. During that load the
# serving lane cannot prove it can decode, so hosting correctly parks (#363) and
# every citizen is REGISTERED BUT NOT RESIDENT — measured at ~15 minutes on
# 2026-08-18, which is the window a benchmark round was then staged into and
# produced zero turns (#455). Adopting a warm lane removes the window rather than
# teaching every caller to wait for it.
#
# Health is /health 200 on the lane's own port, which is a LIVENESS check, not a
# decode check — a wedged server can pass it (#363, exactly why the core verifies
# generation before attaching citizens). That is the correct division: the shell
# adopts a lane that is plausibly alive, and the core's `await_ready_serving`
# remains the authority that refuses to seat citizens on one that cannot decode,
# relaunching it if so. Adopting here can only cost a relaunch the core already
# knows how to do; reaping unconditionally costs a cold load every time.
adopt_or_reap_llama_lanes() {
  local pids adopted=0 reaped=0
  pids="$(pgrep -f 'llama-server' 2>/dev/null || true)"
  [ -z "$pids" ] && return 0
  local pid port
  for pid in $pids; do
    # The lane's port comes from its own cmdline — the only place it is recorded
    # for a process the shell did not spawn.
    port="$(ps -o command= -p "$pid" 2>/dev/null | sed -n 's/.*--port[ =]\([0-9]\{1,\}\).*/\1/p')"
    if [ -n "$port" ] && bounded_run 3 curl -sf "http://127.0.0.1:${port}/health"; then
      echo "  ✓ adopting healthy llama lane (pid $pid, port $port) — warm weights kept" >&2
      adopted=$((adopted + 1))
    else
      echo "  ✗ reaping unhealthy llama lane (pid $pid, port ${port:-unknown})" >&2
      kill -TERM "$pid" 2>/dev/null || true
      reaped=$((reaped + 1))
    fi
  done
  if [ "$reaped" -gt 0 ]; then
    # Give the OS a moment to release the listening socket before the core binds.
    sleep 1
    pkill -9 -f 'llama-server' 2>/dev/null || true
  fi
  echo "  llama lanes: $adopted adopted, $reaped reaped" >&2
}
adopt_or_reap_llama_lanes

# ── ROW: airc daemon — BOOT STARTS IT ────────────────────────────────
# This row did not exist. Boot printed "⚠ airc daemon not running. Start it with:
# airc daemon" and carried on — a runbook line where ownership belongs, and the
# one service whose absence makes the whole system inert: with no transport there
# are no rooms, so citizens have nothing to be resident IN and benchmarks are fed
# into a system that is not running.
#
# Two failure states, distinguished because their fixes differ (the same
# distinction `benchmark/dispatch` now draws between unregistered and
# not-resident):
#   - airc BINARY ABSENT → nothing to enumerate, adopt or spawn. Warn and carry
#     on; a box without airc installed is a different problem from a broken one,
#     and refusing to boot would strand CI and fresh clones.
#   - airc PRESENT but the daemon will not come up → FAIL LOUD, exit nonzero.
#     A core with no transport is not a running system, and reporting success for
#     one is the class of lie this whole card exists to end.
ensure_airc_daemon() {
  if ! command -v airc >/dev/null 2>&1; then
    # BOOT ACQUIRES ITS OWN TRANSPORT (2026-08-24, Joel: "this repo isn't for
    # ME — a new repo user without an agent"). A warn-and-carry-on here left a
    # fresh clone with mute citizens and a runbook line only an agent would
    # ever read. airc is OUR sibling repo; boot installs it the same way the
    # published instructions do, then proceeds. Offline/failed install falls
    # back to the old loud warning — degraded is honest, silent is not.
    echo "▶ airc not installed — installing (CambrianTech/airc, the substrate's transport)" >&2
    if bounded_run 300 sh -c 'curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash'        && command -v airc >/dev/null 2>&1; then
      echo "✓ airc installed ($(command -v airc))" >&2
    else
      # PATH may not include the fresh install dir in THIS shell — try the
      # conventional location before declaring absence.
      if [ -x "${HOME}/.local/bin/airc" ]; then
        export PATH="${HOME}/.local/bin:${PATH}"
        echo "✓ airc installed (${HOME}/.local/bin/airc — added to PATH for this boot)" >&2
      else
        echo "⚠  airc install FAILED — the substrate has no transport." >&2
        echo "   The core will launch, but citizens have no rooms and cannot hear each other." >&2
        echo "   Manual fix: curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash" >&2
        return 0
      fi
    fi
  fi

  if bounded_run 5 airc ping; then
    echo "✓ airc daemon: adopted (already answering)" >&2
    return 0
  fi

  # Not answering. If a daemon process exists it is WEDGED, and a wedged holder is
  # worse than none — it answers nothing AND owns the socket, so a fresh spawn
  # would lose the bind (airc's own start gives up on a contended lock, #355).
  # Reap before spawning: graceful verb first, then the process.
  if pgrep -f 'airc.*daemon' >/dev/null 2>&1; then
    echo "  airc daemon is wedged (holds the socket, answers nothing) — reaping" >&2
    bounded_run 5 airc stop || true
    if pgrep -f 'airc.*daemon' >/dev/null 2>&1; then
      pkill -f 'airc.*daemon' 2>/dev/null || true
      sleep 1
      pkill -9 -f 'airc.*daemon' 2>/dev/null || true
    fi
  fi

  local airc_log="${HOME}/.airc/runtime/daemon-boot.log"
  mkdir -p "$(dirname "$airc_log")" 2>/dev/null || true
  echo "  starting airc daemon (boot owns it, #452) → $airc_log" >&2
  nohup airc daemon >>"$airc_log" 2>&1 &
  disown 2>/dev/null || true

  local waited=0
  while [ "$waited" -lt 30 ]; do
    if bounded_run 5 airc ping; then
      echo "✓ airc daemon: started and answering (${waited}s)" >&2
      return 0
    fi
    sleep 1
    waited=$((waited + 1))
  done

  echo "❌ airc daemon did not answer within 30s of being started." >&2
  echo "   The substrate has no transport: no rooms, no resident citizens, and any" >&2
  echo "   benchmark dispatched now would post cards nobody can see. Last output:" >&2
  tail -20 "$airc_log" >&2 2>/dev/null || true
  return 1
}
if ! ensure_airc_daemon; then
  exit 1
fi

# ── Airc context ─────────────────────────────────────────────────────
# The daemon is guaranteed live by ensure_airc_daemon above, so this block now
# only DERIVES context from it. Substrate auto-discovers the airc daemon socket
# via `airc ipc-endpoint` (task #80). The default room/channel come from
# `airc room` so the personas land in the same scope Joel's terminal sees.
if [ -z "$AIRC_DEFAULT_CHANNEL" ] || [ -z "$AIRC_DEFAULT_ROOM_NAME" ]; then
  if airc status >/dev/null 2>&1; then
    ROOM_OUT="$(airc room 2>/dev/null || true)"
    # Never export an EMPTY value: a parse miss (daemon mid-start, output
    # format drift) exported "" here, and the core then failed the env var
    # as an invalid UUID instead of running its own discovery — a silent
    # empty masquerading as configuration (2026-07-30 boot refusal).
    # Leave unset on miss and say so; the core's own airc discovery is the
    # fallback authority.
    if [ -z "$AIRC_DEFAULT_ROOM_NAME" ]; then
      DERIVED_ROOM="$(awk '/^room:/{print $2}' <<<"$ROOM_OUT")"
      if [ -n "$DERIVED_ROOM" ]; then
        export AIRC_DEFAULT_ROOM_NAME="$DERIVED_ROOM"
      else
        echo "⚠  could not derive room name from 'airc room' output; leaving AIRC_DEFAULT_ROOM_NAME unset" >&2
      fi
    fi
    if [ -z "$AIRC_DEFAULT_CHANNEL" ]; then
      DERIVED_CHANNEL="$(awk '/^channel:/{print $2}' <<<"$ROOM_OUT")"
      if [ -n "$DERIVED_CHANNEL" ]; then
        export AIRC_DEFAULT_CHANNEL="$DERIVED_CHANNEL"
      else
        echo "⚠  could not derive channel from 'airc room' output; leaving AIRC_DEFAULT_CHANNEL unset" >&2
      fi
    fi
  else
    # Reachable ONLY when airc is not installed at all — `ensure_airc_daemon`
    # above has already adopted, reaped-and-restarted, or exited nonzero, so a
    # present-but-down daemon can no longer get this far. It used to say "start
    # it with: airc daemon", which is the runbook line #452 replaced with the
    # boot actually doing it.
    echo "⚠  no airc daemon to derive room/channel from (airc is not installed)" >&2
    echo "   the core will launch, but personas have no rooms and cannot talk." >&2
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

# ── Restart: stop any core already serving this socket ───────────────
# This script is the ONE rebuild+relaunch command ([[validate-via-pure-rust-not-npm-jtag]]):
# it OWNS the socket, so a prior continuum-core-server holding it must be stopped
# before we bind a fresh one. Without this, removing the socket out from under a
# live core orphans it and two processes fight over one socket (line ~59 incident).
# SIGTERM first (graceful drain), then SIGKILL fallback, then clear the socket.
# Called AFTER the build (below) so downtime is ~0: the new binary is ready, we
# stop the old, and exec immediately.
stop_existing_core() {
  local pids
  pids="$(pgrep -f "continuum-core-server" 2>/dev/null | grep -v "^$$\$" || true)"
  if [ -n "$pids" ]; then
    echo "▶ stopping existing core (pids: $(echo $pids | tr '\n' ' '))"
    # shellcheck disable=SC2086
    kill -TERM $pids 2>/dev/null || true
    for _ in $(seq 1 15); do
      pgrep -f "continuum-core-server" >/dev/null 2>&1 || break
      sleep 1
    done
    if pgrep -f "continuum-core-server" >/dev/null 2>&1; then
      echo "  graceful stop timed out — SIGKILL"
      pkill -9 -f "continuum-core-server" 2>/dev/null || true
      sleep 1
    fi
  fi
  rm -f "$CONTINUUM_SOCKET"
}

# ── Launch ───────────────────────────────────────────────────────────
# RELEASE IS THE DEFAULT (Joel, 2026-08-27): the core that hosts citizens and
# takes benchmarks runs optimized unless someone is actively debugging.
# CONTINUUM_DEBUG=1 opts into a debug build (fast compiles, live
# debug_asserts, overflow panics); CONTINUUM_RELEASE stays honored for
# explicitness in scripts that already set it.
PROFILE_FLAG="--release"
PROFILE_LABEL="release"
if [ -n "$CONTINUUM_DEBUG" ] && [ -z "$CONTINUUM_RELEASE" ]; then
  PROFILE_FLAG=""
  PROFILE_LABEL="debug"
fi


# ── #296 swept-artifact guard ────────────────────────────────────────
# Cache eviction is a SUPPORTED event (CLAUDE.md disk doctrine): the shared
# cargo cache's $PROFILE_LABEL/ dir can be swept while build/ + deps/ survive,
# and an incremental `cargo build` then prints "Finished" from a warm
# fingerprint WITHOUT restoring the missing binary (2026-08-01 incident:
# `continuum reboot` reported success, `exec "$CORE_BIN"` died with "No such
# file or directory", serving stayed down — fail-loud worked, self-heal
# didn't). This is the self-heal: if the expected artifact file is missing,
# say so loudly and re-run the SAME build invocation the script already uses
# for that bin (a missing output forces cargo to re-link). Returns non-zero
# if the artifact STILL doesn't exist — the caller decides fatality; no
# silent fallback [[fallbacks-are-illegal-fail-loud]].
# Args: 1=expected artifact path, 2=bin name, 3=manifest path,
#       4+=profile/feature flags (pre-split by the caller, as elsewhere).
ensure_unswept_bin() {
  local bin_path="$1" bin_name="$2" manifest="$3"
  shift 3
  if [ -f "$bin_path" ]; then
    return 0
  fi
  echo "▶ $bin_name missing from cargo cache (swept?) — rebuilding --bin $bin_name" >&2
  cargo build --manifest-path "$manifest" --bin "$bin_name" "$@" \
    || echo "⚠ swept-cache rebuild of $bin_name failed" >&2
  [ -f "$bin_path" ]
}

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

# ── Build the continuum CLI client ──────────────────────────────────────────
# `continuum` is the pure-Rust CLI client (replaces the Node `./jtag`): `continuum ping`,
# `continuum <command> [json]` over the core IPC socket via the uniform Connection.
# Built here so the headless start produces the client on disk too.
# SELF-BUILD GUARD. `continuum reboot` runs THIS script from inside the running
# `continuum` binary. On Windows a running .exe cannot be replaced — the image is
# locked — so this build fails with
#
#   error: failed to remove file `...\cargo-target\debug\continuum.exe`
#
# and cargo returns non-zero for the WHOLE invocation. Every later build in this
# script is skipped, continuum-core-server is never rebuilt, and `continuum reboot`
# dies after its full timeout with "the start script exited (exit code: 1) without
# the core coming up". Measured on BIGMAMA 2026-08-05: 772s to that failure. This
# is why reboot has never worked on Windows — the verb was trying to overwrite
# itself mid-run. (On Unix it silently works: unlink leaves the running inode.)
#
# The caller sets CONTINUUM_SKIP_SELF_BUILD when it IS the continuum binary AND the
# platform locks a running image — `runtime::deploy_provenance::cli_self_build` owns
# that decision and is unit-tested on both rows. It used to be set unconditionally,
# which charged this Windows-only constraint to every operator and made `reboot`
# structurally unable to ship a fix living in the CLI (#422): proven on a Mac
# 2026-08-14, when `stop`'s split-brain reap had merged and the installed CLI still
# did not have it, leaving a core alive and silent under a green deploy-verify.
# Where the image is replaceable the build below runs and the install further down
# swaps it in, so the deploy loop closes with no operator step. Skipping is stated
# out loud, never silent — a skipped build that looks like a completed one is how
# stale binaries survive a "successful" deploy.
if [ -n "${CONTINUUM_SKIP_SELF_BUILD:-}" ]; then
  echo "▶ skipping continuum CLI build — this script was invoked BY the running"
  echo "  continuum binary, which cannot replace its own image while executing."
  echo "  The CORE is still rebuilt below. To update the CLI itself: npm start"
else
  # Build the CLI WITHOUT the GPU feature set. It is an IPC client: it opens the
  # core's socket, sends a command, prints the reply. It never touches a GPU.
  #
  # Every bin in a crate shares one feature set, so building it alongside the core
  # linked CUDA into it — and on Windows that made the CLI UNLAUNCHABLE. Measured:
  # `continuum models/list` exited 127 with ZERO bytes of output, because Windows
  # resolves an executable's imports at LOAD time, before main(). Nothing inside
  # the program can report that, which is why it read as "there is no CLI on
  # Windows" rather than as a link problem. `dumpbin //DEPENDENTS` showed it
  # importing cublas64_13.dll — CUDA 13 — on a box whose CUDA_PATH pointed at a
  # CUDA 12 tree, because with several trees present the linker binds whichever
  # sits earliest on PATH (#6). Joel hit the GUI form of the same thing:
  # "cublas64_12.dll was not found".
  #
  # Colocating the DLLs beside the binary was tried and rejected: the direct
  # imports copy fine and the binary STILL will not load, because those DLLs have
  # their own transitive imports. Chasing the closure ships a CUDA runtime with a
  # socket client.
  #
  # Not linking CUDA into a program that does not use it removes the problem
  # instead of packaging it — and it is what makes the CLI work for people who are
  # not us: a repo user on a laptop with NO NVIDIA card can now run `continuum`
  # to talk to a core over the grid. Before this they could not run it at all.
  #
  # Fall back loudly rather than silently: if the reduced build fails on some
  # platform, the featured build still produces a working CLI on that platform,
  # and the warning names exactly what the user gets instead.
  echo "▶ building continuum (Rust CLI client — GPU-free: it is a socket client)"
  if ! cargo build --manifest-path "$CORE_MANIFEST" --bin continuum $PROFILE_FLAG $CONTINUUM_CLI_FEATURES; then
    echo "⚠ GPU-free continuum build failed — retrying with the full feature set." >&2
    echo "  The CLI will then carry GPU link deps and may fail to launch on a box" >&2
    echo "  without a matching CUDA runtime on PATH. Please report this." >&2
    cargo build --manifest-path "$CORE_MANIFEST" --bin continuum $PROFILE_FLAG $CONTINUUM_FEATURES \
      || echo "⚠ continuum build failed — CLI client unavailable (core still launches)" >&2
  fi
fi

# Put `continuum` on PATH so it works like any installed CLI — self-provisioning, the
# managed-product principle ([[managed-product-everything-self-provisions-no-operator-steps]]).
# COPY the just-built binary into ~/.local/bin (user-writable, conventionally on PATH) —
# do NOT symlink into the cargo target dir. That dir is an ephemeral BUILD artifact: cargo
# replaces the binary mid-rebuild, `cargo clean` and rust-analyzer's feature-mismatched
# rebuilds delete it, and a symlink then DANGLES — the exact flaky mess where `continuum`
# vanishes post-boot ([[deploy-cli-binary-deleted-from-target-dir-post-boot]]). A real copy
# is decoupled from that churn; it only changes when we deploy. NEVER named `cu` — that is
# /usr/bin/cu, the Unix UUCP tool, which shadows it. Idempotent; refreshes each deploy so
# PATH always points at the current build. Copy atomically (temp + mv) so a `continuum`
# invocation concurrent with a deploy never sees a half-written binary.
CONTINUUM_CLI_BIN="$CARGO_TARGET_DIR/$PROFILE_LABEL/continuum"
# #296: a swept cache can leave the build above "Finished" with no binary on
# disk — restore it so the copy below has real bytes. Non-fatal (matches the
# build's own warn): a missing CLI doesn't block core boot, and the installed
# ~/.local/bin copy from the last deploy keeps working.
if ! ensure_unswept_bin "$CONTINUUM_CLI_BIN" continuum "$CORE_MANIFEST" $PROFILE_FLAG $CONTINUUM_CLI_FEATURES; then
  echo "⚠ continuum CLI still missing after swept-cache rebuild — CLI install skipped (core still launches)" >&2
fi
if [ -x "$CONTINUUM_CLI_BIN" ]; then
  CONTINUUM_LINK_DIR="$HOME/.local/bin"
  mkdir -p "$CONTINUUM_LINK_DIR"
  # A stale symlink from an earlier install would otherwise make `cp` follow it back into
  # the target dir — remove any existing entry first, then copy the real bytes.
  rm -f "$CONTINUUM_LINK_DIR/continuum"
  if cp "$CONTINUUM_CLI_BIN" "$CONTINUUM_LINK_DIR/continuum.tmp.$$" \
     && mv -f "$CONTINUUM_LINK_DIR/continuum.tmp.$$" "$CONTINUUM_LINK_DIR/continuum"; then
    # `uu` — THE official short alias (Joel, 2026-08-01): the double-U of
    # contin-UU-m. One name on every platform, so recipes/docs stay portable —
    # `cu` is /usr/bin/cu (UUCP) on Unix and `co` is RCS checkout wherever rcs
    # is installed; per-box aliases are how the grid silently forked once
    # already. Symlink to OUR installed copy (stable across deploys, unlike the
    # cargo target dir). Squatter guard: refuse loudly if `uu` resolves to a
    # binary that isn't ours — never shadow, never silently skip.
    UU_LINK="$CONTINUUM_LINK_DIR/uu"
    UU_RESOLVED="$(command -v uu 2>/dev/null || true)"
    if [ -n "$UU_RESOLVED" ] && [ "$UU_RESOLVED" != "$UU_LINK" ]; then
      echo "  ⚠ 'uu' already resolves to $UU_RESOLVED — NOT installing the alias (rename or remove the squatter)" >&2
    else
      ln -sfn "$CONTINUUM_LINK_DIR/continuum" "$UU_LINK"
    fi
  else
    rm -f "$CONTINUUM_LINK_DIR/continuum.tmp.$$"
    echo "  ⚠ could not install continuum CLI into $CONTINUUM_LINK_DIR" >&2
  fi
  case ":$PATH:" in
    *":$CONTINUUM_LINK_DIR:"*) : ;;
    *) echo "  ⚠ $CONTINUUM_LINK_DIR is not on PATH — add it so \`continuum\` resolves directly" >&2 ;;
  esac
fi

# ── Build the forge-custodian sidecar ────────────────────────────────
# Like continuum-mcp, this bin is SPAWNED by the core (not launched by us): the
# genome loop's `forge/export` self-provisions it on demand via
# `forge::custodian_supervisor::ensure_local_custodian`, which resolves the binary
# as a SIBLING of the core exe. So it must exist on disk after the build, or the
# self-improvement loop fails loud at "custodian binary not found" the first time a
# trained gene needs converting to a pageable gguf-lora. Same manifest/features/
# profile as the core → a fast incremental once the core is built. Non-fatal: a
# missing custodian only blocks gene conversion, not core boot; the supervisor
# already surfaces an actionable error. ([[managed-product-everything-self-provisions-no-operator-steps]], #52/#25)
echo "▶ building forge-custodian (Rust gguf-lora export sidecar)"
cargo build --manifest-path "$CORE_MANIFEST" --bin forge-custodian $PROFILE_FLAG $CONTINUUM_FEATURES \
  || echo "⚠ forge-custodian build failed — genome gene-conversion unavailable (core still launches)" >&2
# #296: the core spawns this bin later as a sibling of its own exe — a swept
# cache that survives the build above would fail loud only at first gene
# conversion. Restore it now; non-fatal, matching the build's own warn.
if ! ensure_unswept_bin "$CARGO_TARGET_DIR/$PROFILE_LABEL/forge-custodian" forge-custodian "$CORE_MANIFEST" $PROFILE_FLAG $CONTINUUM_FEATURES; then
  echo "⚠ forge-custodian still missing after swept-cache rebuild — genome gene-conversion unavailable (core still launches)" >&2
fi

# WINDOWS: stop the old core BEFORE building it.
#
# The build-first ordering below is Unix-shaped. Only there can you replace the
# FILE of a running executable — unlink leaves the running inode, so the old core
# keeps serving out of memory while cargo writes the new binary. Windows LOCKS a
# running image, so building over a live core fails with
#
#   error: failed to remove file `...\continuum-core-server.exe`
#   Caused by: Access is denied. (os error 5)
#
# and the FATAL below aborts the whole start. This is not a `continuum reboot`
# bug — it is in the shared build path, so `npm start` has it too: on Windows the
# core could never be rebuilt while a core was running. That is the real scope of
# "start/reboot/pull never worked on Windows". Measured on BIGMAMA 2026-08-05,
# reproduced both from `continuum reboot` AND from a standalone run of this
# script, which is what proved it was not reboot-specific.
#
# So on Windows: stop first, then build, then launch. Zero downtime is an explicit
# NON-goal here — restarts are commonplace, stop-build-launch is the model, and
# the grid absorbs the churn. Unix keeps the overlapping build for its free ~0
# downtime. Loud either way: a silent stop would look like a hang during a long
# compile.
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    if tasklist //FI "IMAGENAME eq continuum-core-server.exe" 2>/dev/null | grep -qi continuum-core-server; then
      echo "▶ Windows: stopping the running core before rebuilding it — a running .exe is"
      echo "  locked, so build-then-swap cannot work here (downtime is expected, not a fault)"
      taskkill //F //T //IM continuum-core-server.exe >/dev/null 2>&1 || true
      # A dead pid is not a closed handle. Poll until the image is actually
      # released; building one tick early reproduces the exact os error 5.
      for _ in $(seq 1 60); do
        tasklist //FI "IMAGENAME eq continuum-core-server.exe" 2>/dev/null \
          | grep -qi continuum-core-server || break
        sleep 0.5
      done
      if tasklist //FI "IMAGENAME eq continuum-core-server.exe" 2>/dev/null | grep -qi continuum-core-server; then
        echo "✗ FATAL: core still running after 30s — refusing to build over a locked image" >&2
        echo "  (that path fails with os error 5 and silently leaves you on the OLD binary)" >&2
        exit 1
      fi
    fi
    ;;
esac

# Build the server binary BEFORE stopping the old core, so the running core keeps
# serving through the (cached, fast) compile and downtime is ~0. (Unix ordering —
# see the Windows stop-first block above.)
echo "▶ building continuum-core-server"
cargo build --manifest-path "$CORE_MANIFEST" --bin continuum-core-server $PROFILE_FLAG $CONTINUUM_FEATURES \
  || { echo "✗ FATAL: continuum-core-server build failed — leaving the running core untouched" >&2; exit 1; }

# ── #194 FRESHNESS GUARD: never launch (or report "ready" on) a STALE binary ──
# cargo's incremental fingerprint can MISS a source edit (mtime granularity, an
# editor writing a non-advancing mtime, or a prior `cargo check` updating the
# fingerprint without codegen) and emit a no-op "Finished" that leaves the OLD
# continuum-core-server on disk. The deploy then reports "core ready" while stale
# code runs — the verify-the-build-actually-deployed trap (observed 2026-07-18:
# a committed one-line change never shipped; every headless live validation lied).
# Assert the invariant DIRECTLY: the built binary must be at least as new as every
# tracked source. If a source is newer, cargo missed it — bust the fingerprint by
# touching the crate src and rebuild ONCE; if STILL stale, FAIL LOUD rather than
# launch a lie. [[verify-the-build-actually-deployed]], [[fallbacks-are-illegal-fail-loud]].
CORE_SRC_DIR="$(dirname "$CORE_MANIFEST")/src"
CORE_BIN="$CARGO_TARGET_DIR/$PROFILE_LABEL/continuum-core-server"
# #296: THE incident bin. A swept debug/ dir let the build above print
# "Finished" with no binary on disk, and `exec "$CORE_BIN"` died at the very
# end while the deploy read as green. Restore it HERE — before the #194
# freshness guard, so #194 asserts against a real file — and if the rebuild
# STILL can't produce it, fail loud now instead of at exec.
if ! ensure_unswept_bin "$CORE_BIN" continuum-core-server "$CORE_MANIFEST" $PROFILE_FLAG $CONTINUUM_FEATURES; then
  echo "✗ FATAL #296: continuum-core-server missing at $CORE_BIN even after a swept-cache rebuild — refusing to exec a nonexistent binary (leaving any running core untouched)" >&2
  exit 1
fi
core_bin_is_stale() { [ -f "$CORE_BIN" ] && [ -n "$(find "$CORE_SRC_DIR" -name '*.rs' -type f -newer "$CORE_BIN" 2>/dev/null | head -1)" ]; }
if core_bin_is_stale; then
  echo "⚠ #194: continuum-core-server is STALE (a source is newer than the binary) — cargo missed an edit; busting fingerprint + rebuilding" >&2
  find "$CORE_SRC_DIR" -name '*.rs' -type f -exec touch {} +
  cargo build --manifest-path "$CORE_MANIFEST" --bin continuum-core-server $PROFILE_FLAG $CONTINUUM_FEATURES \
    || { echo "✗ FATAL #194: forced rebuild failed — leaving the running core untouched" >&2; exit 1; }
  if core_bin_is_stale; then
    echo "✗ FATAL #194: continuum-core-server STILL stale after a forced rebuild — refusing to launch old code (verify-the-build-actually-deployed)" >&2
    exit 1
  fi
  echo "✓ #194: forced a fresh continuum-core-server rebuild — binary now reflects source"
fi

# ── LiveKit avatar rail (voice/video calls) ──────────────────────────
# The persona's talking avatar is Bevy-rendered and published to a LiveKit room via
# the livekit-bridge sidecar; the browser's "Go live" subscribes to that room. Neither
# the SFU nor the bridge was started with the core, so "Go live" produced no avatar —
# glass-boxed 2026-07-28: LiveKit :7880 down → get_or_create_agent fails → no video pump,
# and only a native test-pattern reached clients. Bring the rail up here, idempotently
# and NON-FATALLY: a missing rail leaves the core fully functional (chat/cognition/
# serving); only live A/V is unavailable. Sidecar because it links webrtc-sys, which we
# keep OUT of the core process ([[gpu-is-non-negotiable...]] resource isolation).
start_livekit_rail() {
  case "$(uname -s)" in Darwin|Linux) ;; *) return 0 ;; esac  # bridge speaks a unix-socket
  local LK_LOG_DIR="$HOME/.continuum/logs"; mkdir -p "$LK_LOG_DIR"
  local SOCK="$HOME/.continuum/sockets/livekit-bridge.sock"; mkdir -p "$(dirname "$SOCK")"
  local LK_URL="${LIVEKIT_URL:-ws://localhost:7880}"
  # 1) SFU. Dev creds (devkey/secret) match the bridge's defaults. Missing binary = warn+skip.
  if command -v livekit-server >/dev/null 2>&1; then
    if ! nc -z 127.0.0.1 7880 2>/dev/null; then
      echo "▶ livekit-server (avatar SFU) starting on :7880"
      nohup livekit-server --dev --bind 127.0.0.1 >"$LK_LOG_DIR/livekit-server.log" 2>&1 &
      for _ in $(seq 1 20); do nc -z 127.0.0.1 7880 2>/dev/null && break; sleep 0.3; done
    fi
  else
    echo "⚠ livekit-server not installed — live avatar/voice unavailable (tools/scripts/install-livekit.sh)"
    return 0
  fi
  # 2) livekit-bridge sidecar — ALWAYS release (a stable, rarely-changing sidecar that
  #    benefits from optimization and only builds once, independent of the core's debug/
  #    release profile). Started BEFORE the core so the socket its bridge_client dials
  #    exists at boot.
  local BRIDGE_BIN="$CARGO_TARGET_DIR/release/livekit-bridge"
  # Rebuild when STALE, not just when missing (2026-09-01: an Aug 5 bridge ran
  # against a core speaking the new binary media plane — "rarely-changing
  # sidecar" is an assumption, not a contract; the deploy path must verify it
  # like everything else, #194). Any source newer than the binary → rebuild +
  # restart the running sidecar.
  local BRIDGE_STALE=""
  if [ -x "$BRIDGE_BIN" ]; then
    BRIDGE_STALE=$(find "$REPO_ROOT/core/livekit-bridge" "$REPO_ROOT/core/livekit-protocol" \
      \( -name '*.rs' -o -name 'Cargo.toml' \) -newer "$BRIDGE_BIN" 2>/dev/null | head -1)
  fi
  if [ ! -x "$BRIDGE_BIN" ] || [ -n "$BRIDGE_STALE" ]; then
    [ -n "$BRIDGE_STALE" ] && echo "▶ livekit-bridge stale (newer: ${BRIDGE_STALE#"$REPO_ROOT"/}) — rebuilding"
    echo "▶ building livekit-bridge sidecar (release — links webrtc-sys, first build is slow)…"
    cargo build --manifest-path "$REPO_ROOT/core/livekit-bridge/Cargo.toml" --bin livekit-bridge --release \
      || { echo "⚠ livekit-bridge build failed — live avatar/voice unavailable"; return 0; }
    # #296: a swept cache can let cargo report success without the binary on
    # disk — verify before nohup'ing a nonexistent path (rail is non-fatal).
    if [ ! -x "$BRIDGE_BIN" ]; then
      echo "⚠ livekit-bridge still missing at $BRIDGE_BIN after rebuild (swept cache?) — live avatar/voice unavailable"
      return 0
    fi
    # A stale sidecar may still be running the OLD wire — stop it so the spawn
    # block below relaunches the fresh binary (core's bridge_client redials).
    if [ -n "$BRIDGE_STALE" ] && pgrep -f "livekit-bridge .*${SOCK}" >/dev/null 2>&1; then
      echo "▶ restarting livekit-bridge with fresh binary"
      pkill -f "livekit-bridge .*${SOCK}" 2>/dev/null || true
      sleep 0.5
    fi
  fi
  if ! pgrep -f "livekit-bridge .*${SOCK}" >/dev/null 2>&1; then
    echo "▶ livekit-bridge sidecar starting ($SOCK → $LK_URL)"
    rm -f "$SOCK"
    LIVEKIT_API_KEY="${LIVEKIT_API_KEY:-devkey}" LIVEKIT_API_SECRET="${LIVEKIT_API_SECRET:-secret}" \
      nohup "$BRIDGE_BIN" "$SOCK" --livekit-url "$LK_URL" >"$LK_LOG_DIR/livekit-bridge.log" 2>&1 &
    for _ in $(seq 1 20); do [ -S "$SOCK" ] && break; sleep 0.3; done
  fi
}
start_livekit_rail

# ── The eye-node rail: perception is part of the stack, not an operator chore.
# Every reboot used to orphan it (manual `npx tsx` each time); now the boot
# path owns it ([[boot-owns-the-process-tree]]). Spawned BEFORE the core execs —
# the eye-node dials with retry until the socket binds, and its transport
# re-provides across core restarts, so ordering is free. Non-fatal: no
# eye-node = `perception/observe` fails loud, core still boots.
start_eye_node_rail() {
  local EYE_DIR="$REPO_ROOT/apps/eye-node"
  [ -f "$EYE_DIR/package.json" ] || return 0
  command -v npx >/dev/null 2>&1 || { echo "⚠ npx missing — eye-node (perception) unavailable"; return 0; }
  # Full path in the cmdline makes the process identifiable (pgrep) and the
  # spawn idempotent across reboots.
  if ! pgrep -f "eye-node/src/index.ts" >/dev/null 2>&1; then
    local EYE_LOG_DIR="$HOME/.continuum/logs"; mkdir -p "$EYE_LOG_DIR"
    echo "▶ eye-node (perception provider) starting"
    (cd "$EYE_DIR" && nohup npx tsx "$EYE_DIR/src/index.ts" >"$EYE_LOG_DIR/eye-node.log" 2>&1 &)
  fi
}
start_eye_node_rail

# Now the new binary is ready: stop the old core (if any) and take the socket.
stop_existing_core

echo "▶ continuum-core-server starting"
echo "  profile:  $PROFILE_LABEL"
echo "  features: $CONTINUUM_FEATURES"
echo "  socket:   $CONTINUUM_SOCKET"
echo "  airc:     room=${AIRC_DEFAULT_ROOM_NAME:-?} channel=${AIRC_DEFAULT_CHANNEL:-?}"

# Voice/model artifacts (TTS/STT ONNX, VAD, phonemizer data) live under the
# gitignored download root `tools/models/`. Inject it ABSOLUTELY so the audio
# adapters resolve models regardless of the core's CWD — killing the process-CWD
# dependency they used to have (#195). The core's `voice_model_root()` honors this
# env; config.env can still override it per operator.
export CONTINUUM_MODELS_DIR="${CONTINUUM_MODELS_DIR:-$REPO_ROOT/tools/models}"
echo "  models:   $CONTINUUM_MODELS_DIR"

# ── STT model rail (#291: a fresh clone HEARS with zero manual steps). The
# local moonshine engine (sherpa-onnx int8, ~286MB once) is what makes citizens
# able to listen in live calls without any API key; without this rail a new
# machine boots deaf behind a warning. One-time fetch, non-fatal, loud on miss.
ensure_moonshine() {
  local DIR="$CONTINUUM_MODELS_DIR/moonshine/base"
  local BASE="https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-base-en-int8/resolve/main"
  local files=(preprocess.onnx encode.int8.onnx uncached_decode.int8.onnx cached_decode.int8.onnx tokens.txt)
  local missing=0
  for f in "${files[@]}"; do [ -s "$DIR/$f" ] || missing=1; done
  [ "$missing" = 0 ] && return 0
  command -v curl >/dev/null 2>&1 || { echo "  ⚠ curl missing — STT (hearing) unavailable until moonshine models are placed in $DIR" >&2; return 0; }
  echo "→ first boot: fetching the local STT model (moonshine base int8, ~286MB once)…"
  mkdir -p "$DIR"
  local ok=1
  for f in "${files[@]}"; do
    [ -s "$DIR/$f" ] && continue
    curl -sfL -o "$DIR/$f.tmp" "$BASE/$f" && mv "$DIR/$f.tmp" "$DIR/$f" || { ok=0; rm -f "$DIR/$f.tmp"; }
  done
  [ "$ok" = 1 ] && echo "  STT model ready — citizens can hear" \
    || echo "  ⚠ moonshine fetch incomplete — STT unavailable this boot (retries next boot)" >&2
}
ensure_moonshine
echo ""

# PUBLISH the verified artifact to the installed location, the same way this script
# already publishes the CLI a few dozen lines up — and for the identical reason.
#
# `continuum start` execs the INSTALLED continuum-core-server (building is reserved
# for `reboot`), and its resolver checks ~/.continuum/bin BEFORE any cargo target
# dir. That copy was written once by install.sh and never refreshed by a deploy, so
# on a machine that has been deploying for a month, `continuum start` silently boots
# a month-old core while the fresh build sits unused in the cache. Measured on the M5
# on 2026-08-13: installed artifact dated Jul 13, running build 4705, HEAD 4712 — and
# a stray auto-start off that stale copy mid-reboot is what tripped the #194 mismatch
# and cost an hour of misreading.
#
# Publishing HERE (after the #194 freshness guard, before exec) means the installed
# artifact is only ever replaced by a binary we just proved matches source — never a
# half-built or stale one. Atomic temp+mv so a concurrent `continuum start` never
# execs a half-written file. Non-fatal: failing to publish doesn't block this boot,
# which runs $CORE_BIN directly either way.
# [[managed-product-everything-self-provisions-no-operator-steps]], #194, #291
CORE_INSTALL_DIR="$HOME/.continuum/bin"
if mkdir -p "$CORE_INSTALL_DIR" 2>/dev/null; then
  if cp "$CORE_BIN" "$CORE_INSTALL_DIR/continuum-core-server.tmp.$$" 2>/dev/null \
     && mv -f "$CORE_INSTALL_DIR/continuum-core-server.tmp.$$" \
              "$CORE_INSTALL_DIR/continuum-core-server" 2>/dev/null; then
    echo "  installed: $CORE_INSTALL_DIR/continuum-core-server (refreshed from this build)"
  else
    rm -f "$CORE_INSTALL_DIR/continuum-core-server.tmp.$$" 2>/dev/null || true
    echo "  ⚠ could not refresh $CORE_INSTALL_DIR/continuum-core-server — \`continuum start\` may boot an OLDER core than this one" >&2
  fi
fi

# Run the EXACT binary the freshness guard (#194) just verified — NOT `cargo run`,
# which re-runs cargo's build logic at launch and could second-guess (or re-stale)
# what we already verified. We built it, we checked it reflects source, we run it.
# Unambiguous: the process image is the verified $CORE_BIN. [[verify-the-build-actually-deployed]]
# ── The desktop display manager's dist (Joel: 'should work like a Display
# Manager'). Build the web client so the core can serve it — ALWAYS current
# by construction: this runs on every start/reboot, so the greeter and the
# core deploy as one generation. Non-fatal: a failed UI build boots a
# headless core (desktop.dm.dist_missing probes the fix) rather than no core.
if [ -f "$REPO_ROOT/apps/web/package.json" ] && command -v npm >/dev/null 2>&1; then
  # Fresh clone (#291): the workspaces' node_modules must exist before the web
  # build or the eye-node rail can run — without this, a first boot warned
  # "desktop build failed" + spawned an eye-node that could not resolve tsx,
  # and the new machine got a headless, eyeless core with no manual step named.
  if [ ! -d "$REPO_ROOT/node_modules" ]; then
    echo "→ first boot: installing workspace deps (npm ci)…"
    (cd "$REPO_ROOT" && npm ci >/dev/null 2>&1 || npm install >/dev/null 2>&1) \
      || echo "  ⚠ npm install failed — desktop + eye-node unavailable (run npm ci to diagnose)" >&2
  fi
  # NEVER IN FRONT (2026-09-02, Joel: "Desktop is optional… depends on core
  # being up of course so must initiate if necessary"). The desktop is ONE
  # optional client of a headless core — a web build has no business gating
  # boot, fresh clone included: the core comes up NOW, and the dist lands in
  # the background a minute later (desktop.dm.dist_missing probes the window;
  # `continuum desktop` before it lands says the build is in flight rather
  # than showing a broken page). This replaced a 1–2 minute SERIAL build in
  # front of exec on EVERY boot.
  export CONTINUUM_UI_DIST="$REPO_ROOT/apps/web/dist"
  if [ -d "$REPO_ROOT/apps/web/dist" ]; then
    echo "→ desktop: serving the existing dist now; rebuilding in the background…"
  else
    echo "→ desktop: no dist yet — core boots headless now; building in the background…"
  fi
  (cd "$REPO_ROOT" && npm run build -w @continuum/web >/dev/null 2>&1 \
    && echo "  desktop build landed (reload / continuum desktop to open it)" \
    || echo "  ⚠ background desktop build failed — run npm run build -w @continuum/web to diagnose" >&2) &
fi

exec "$CORE_BIN" "$CONTINUUM_SOCKET"
