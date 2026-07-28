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
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) _mf_os=windows ;;
  Darwin)               _mf_os=macos ;;
  *)                    _mf_os=linux ;;
esac
_mf_runtime="$SCRIPT_DIR/generated/manifest.${_mf_os}.sh"
if [ -f "$_mf_runtime" ]; then
  # shellcheck source=/dev/null
  source "$_mf_runtime"
  if declare -p MOD_RUNTIME_PATH >/dev/null 2>&1; then
    for _mid in "${!MOD_RUNTIME_PATH[@]}"; do
      IFS=':' read -ra _rp_dirs <<<"${MOD_RUNTIME_PATH[$_mid]}"
      for _rp in "${_rp_dirs[@]}"; do
        # eval expands ~ and the version glob (cuda-*); prepend each existing match once.
        for _rp_hit in $(eval echo "$_rp"); do
          [ -d "$_rp_hit" ] || continue
          case ":$PATH:" in *":$_rp_hit:"*) ;; *) export PATH="$_rp_hit:$PATH" ;; esac
        done
      done
    done
  fi
fi

# ── Windows-CUDA build env (2026-07-28, BigMama) ─────────────────────
# The core's `cuda` feature needs the MSVC toolchain visible to cargo's build
# scripts (candle-kernels runs nvcc→cl.exe; the llama crate links cuda.lib).
# On a stock Git-Bash shell NONE of that is on PATH, so cargo-features.sh
# (below) silently degrades to directml-only — and the core then mis-detects
# a 32GB card as ~4GB (no detect_cuda) and serves a toy model. Establish the
# env HERE, before feature detection, so a fresh Windows+NVIDIA box gets the
# real build with zero hand steps:
#   1. cl.exe absent but VS2022 BuildTools installed → re-exec this script
#      once through vcvars64 (same cmd/.bat bridge install-llama-server.sh
#      uses; VS2022 pinned — nvcc rejects VS18/14.5x toolsets).
#   2. MSVC link.exe must BEAT Git's /usr/bin/link.exe (coreutils) or every
#      build-script link dies — prepend the cl.exe dir to PATH.
#   3. cuda.lib/cublas.lib etc live in the toolkit's lib/x64 which vcvars
#      does NOT add — prepend to LIB for link.exe (LNK1181 otherwise).
#   4. bindgen reads LIBCLANG_PATH (the env var — PATH is not enough).
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*)
    if command -v nvidia-smi >/dev/null 2>&1 && ! command -v cl.exe >/dev/null 2>&1 \
       && [ -z "${CONTINUUM_MSVC_REENTER:-}" ]; then
      _vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
      _vs_path=""
      [ -x "$_vswhere" ] && _vs_path="$("$_vswhere" -version "[17.0,18.0)" -products '*' \
          -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
          -property installationPath 2>/dev/null | head -1)"
      if [ -n "$_vs_path" ]; then
        echo "→ NVIDIA GPU + VS2022 present but cl.exe not on PATH — re-entering via vcvars64 for the CUDA build" >&2
        _reenter_bat="$(mktemp --suffix=.bat 2>/dev/null || echo "${TMPDIR:-/tmp}/continuum-msvc-reenter.bat")"
        _win_bash="$(cygpath -w "$(command -v bash)")"
        _script_unix="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
        {
          echo "@echo off"
          echo "call \"$(cygpath -w "$_vs_path")\\VC\\Auxiliary\\Build\\vcvars64.bat\" >nul || exit /b 1"
          echo "set CONTINUUM_MSVC_REENTER=1"
          echo "\"$_win_bash\" \"$_script_unix\" || exit /b 1"
        } > "$_reenter_bat"
        exec cmd //c "$(cygpath -w "$_reenter_bat")"
      else
        echo "⚠  NVIDIA GPU present but VS2022 BuildTools not found — core builds directml-only" >&2
        echo "   (install-manifest 'msvc' module provisions it; cuda serving needs it)" >&2
      fi
    fi
    if command -v cl.exe >/dev/null 2>&1; then
      # (2) MSVC linker precedence over Git's coreutils link.exe.
      _cl_dir="$(dirname "$(command -v cl.exe)")"
      case ":$PATH:" in "$_cl_dir":*) ;; *) export PATH="$_cl_dir:$PATH" ;; esac
      # (3) CUDA import libs onto LIB (version-agnostic glob, mirrors runtime_path).
      for _cuda_lib in "$HOME/.continuum"/cuda-*/Library/lib/x64 "$HOME/.continuum"/cuda-*/lib/x64; do
        [ -d "$_cuda_lib" ] && export LIB="$(cygpath -w "$_cuda_lib");${LIB:-}"
      done
      # (4) bindgen's libclang (manifest llvm-libclang install location).
      [ -f "$HOME/.continuum/tools/llvm/bin/libclang.dll" ] \
        && export LIBCLANG_PATH="$HOME/.continuum/tools/llvm/bin"
    fi
    ;;
esac

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

# (2) Clear any FOREIGN inference server so the core starts from a clean slate
# and gets the preferred port with the GPU to itself. At this point in a reboot
# the old core is already dead, so any live llama-server is an orphan (its parent
# gone) and any Unsloth Studio is the excised gateway — both safe to stop.
#   - the Studio parent would respawn its own backend, so stop it first;
#   - its llama-server child is orphaned (reparented to init) when the parent
#     dies and would keep holding the port + GPU, so stop that too.
# The core's fresh llama-server is launched afterward by the serving daemon, on a
# port it SCANS for — so this is GPU/excision hygiene, not a correctness gate.
if pgrep -f 'studio run' >/dev/null 2>&1; then
  echo "  stopping excised Unsloth Studio (freeing GPU for the core's engine)" >&2
  pkill -f 'studio run' 2>/dev/null || true
fi
if pgrep -f 'llama-server' >/dev/null 2>&1; then
  echo "  clearing orphaned llama-server backend(s) so the core owns the engine" >&2
  pkill -f 'llama-server' 2>/dev/null || true
  # Give the OS a moment to release the listening socket before the core binds.
  sleep 1
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

# ── Restart: stop any core already serving this socket ───────────────
# This script is the ONE rebuild+relaunch command ([[validate-via-pure-rust-not-npm-jtag]]):
# it OWNS the socket, so a prior continuum-core-server holding it must be stopped
# before we bind a fresh one. Without this, removing the socket out from under a
# live core orphans it and two processes fight over one socket (line ~59 incident).
# SIGTERM first (graceful drain), then SIGKILL fallback, then clear the socket.
# Called AFTER the build (below) so downtime is ~0: the new binary is ready, we
# stop the old, and exec immediately.
stop_existing_core() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      # POSIX signals are a SILENT NO-OP against native Windows exes from
      # Git-Bash: pgrep -f matches nothing and kill/pkill can't touch them, so
      # this block never stopped anything on Windows — every `npm start` boot
      # then died fighting the immortal old core for its ports (observed
      # 2026-07-28 BigMama: an 8:22AM core survived three "restarts"; each new
      # boot failed binding 0.0.0.0:7117 and exited, masked as "leaving the
      # running core untouched"). Use tasklist/taskkill — the native tools.
      if tasklist 2>/dev/null | grep -qi "continuum-core-server.exe"; then
        echo "▶ stopping existing core (taskkill)"
        # No graceful console signal exists for a detached native service from
        # here (WM_CLOSE is ignored by console apps); the core's state is
        # crash-safe by design ([[no-fallbacks-ever]] boot contract), so /F.
        taskkill //F //IM continuum-core-server.exe >/dev/null 2>&1 || true
        for _ in $(seq 1 10); do
          tasklist 2>/dev/null | grep -qi "continuum-core-server.exe" || break
          sleep 1
        done
      fi
      ;;
    *)
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
      ;;
  esac
  rm -f "$CONTINUUM_SOCKET"
}

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

# ── Build the continuum CLI client ──────────────────────────────────────────
# `continuum` is the pure-Rust CLI client (replaces the Node `./jtag`): `continuum ping`,
# `continuum <command> [json]` over the core IPC socket via the uniform Connection.
# Built here so the headless start produces the client on disk too.
echo "▶ building continuum (Rust CLI client)"
cargo build --manifest-path "$CORE_MANIFEST" --bin continuum $PROFILE_FLAG $CONTINUUM_FEATURES \
  || echo "⚠ continuum build failed — CLI client unavailable (core still launches)" >&2

# Put `continuum` on PATH so it works like any installed CLI — self-provisioning, the
# managed-product principle ([[managed-product-everything-self-provisions-no-operator-steps]]).
# Symlink the just-built binary into ~/.local/bin (user-writable, conventionally on PATH).
# NEVER named `cu` — that is /usr/bin/cu, the Unix UUCP tool, which shadows it. Idempotent;
# refreshes each deploy so PATH always points at the current build.
CONTINUUM_CLI_BIN="$CARGO_TARGET_DIR/$PROFILE_LABEL/continuum"
if [ -x "$CONTINUUM_CLI_BIN" ]; then
  CONTINUUM_LINK_DIR="$HOME/.local/bin"
  mkdir -p "$CONTINUUM_LINK_DIR"
  ln -sf "$CONTINUUM_CLI_BIN" "$CONTINUUM_LINK_DIR/continuum"
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

# Build the server binary BEFORE stopping the old core, so the running core keeps
# serving through the (cached, fast) compile and downtime is ~0.
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

# Now the new binary is ready: stop the old core (if any) and take the socket.
stop_existing_core

echo "▶ continuum-core-server starting"
echo "  profile:  $PROFILE_LABEL"
echo "  features: $CONTINUUM_FEATURES"
echo "  socket:   $CONTINUUM_SOCKET"
echo "  airc:     room=${AIRC_DEFAULT_ROOM_NAME:-?} channel=${AIRC_DEFAULT_CHANNEL:-?}"
echo ""

# Run the EXACT binary the freshness guard (#194) just verified — NOT `cargo run`,
# which re-runs cargo's build logic at launch and could second-guess (or re-stale)
# what we already verified. We built it, we checked it reflects source, we run it.
# Unambiguous: the process image is the verified $CORE_BIN. [[verify-the-build-actually-deployed]]
exec "$CORE_BIN" "$CONTINUUM_SOCKET"
