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
# The generated manifest uses bash-4 associative arrays (`declare -A`). macOS ships bash 3.2,
# where those are a hard `invalid option` error — and under this script's `set -e` a mid-file
# failure ABORTS the whole boot (regression from #2046 "serve on Windows", which regenerated
# manifest.macos.sh with `declare -A`; it silently broke every macOS reboot until the last
# long-running core died). Only source it on bash 4+. The manifest solely feeds the
# runtime-PATH augmentation below (a Windows/CUDA concern), whose own guard already tolerates
# absence — so skipping it on bash 3.2 costs macOS nothing and the boot proceeds to the build.
if [ -f "$_mf_runtime" ] && [ "${BASH_VERSINFO[0]:-0}" -ge 4 ]; then
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
PROFILE_FLAG=""
PROFILE_LABEL="debug"
if [ -n "$CONTINUUM_RELEASE" ]; then
  PROFILE_FLAG="--release"
  PROFILE_LABEL="release"
fi

# ── Windows: import the MSVC toolchain so cargo's CUDA (candle) build finds cl.exe ──────────
# candle compiles CUDA kernels (affine.cu, ...) via nvcc, which needs cl.exe as its host
# compiler plus INCLUDE/LIB. The cargo builds below run in THIS bash shell (unlike the
# llama-server cmake build, which runs inside a vcvars .bat), so without this nvcc fails with
# "Cannot find compiler 'cl.exe' in PATH" and the whole core build dies. Import it once: export
# INCLUDE/LIB verbatim (only cl.exe reads them) and prepend the EXACT MSVC + Windows SDK bin
# dirs (from vcvars, converted to unix) to PATH — nvcc finds cl.exe while bash's own PATH
# resolution stays intact (we add unix dirs, never overwrite PATH with the Windows one). VS2022
# (14.4x) is selected explicitly: nvcc 12.x rejects the newer 14.5x/VS18 toolset.
if [ "$_mf_os" = windows ] && command -v nvcc >/dev/null 2>&1 && ! command -v cl.exe >/dev/null 2>&1; then
  _vswhere="/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe"
  _vs=""
  [ -x "$_vswhere" ] && _vs="$("$_vswhere" -version "[17.0,18.0)" -products '*' \
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
        -property installationPath 2>/dev/null | head -1)"
  _vcvars_u=""
  [ -n "$_vs" ] && _vcvars_u="$(cygpath -u "$_vs\\VC\\Auxiliary\\Build\\vcvars64.bat" 2>/dev/null)"
  if [ -n "$_vcvars_u" ] && [ -f "$_vcvars_u" ]; then
    # Capture vcvars' env via a .bat FILE, not inline `cmd //c "call … && set"`: the vcvars path
    # has spaces AND parens, and escaping nested quotes through bash->cmd corrupts it (the same
    # reason install-llama-server.sh emits a .bat). The .bat calls vcvars then dumps `set`.
    _msbat="$(mktemp --suffix=.bat 2>/dev/null || echo "${TEMP:-/tmp}/_cc_vcvars_$$.bat")"
    printf '@echo off\r\ncall "%s" >nul 2>&1\r\nset\r\n' "$(cygpath -w "$_vcvars_u")" > "$_msbat"
    _msenv="$(cmd //c "$(cygpath -w "$_msbat")" 2>/dev/null | tr -d '\r')"
    rm -f "$_msbat"
    export INCLUDE="$(printf '%s\n' "$_msenv" | sed -n 's/^INCLUDE=//Ip' | head -1)"
    export LIB="$(printf '%s\n' "$_msenv" | sed -n 's/^LIB=//Ip' | head -1)"
    _vct="$(printf '%s\n' "$_msenv" | sed -n 's/^VCToolsInstallDir=//Ip' | head -1)"
    _sdkbin="$(printf '%s\n' "$_msenv" | sed -n 's/^WindowsSdkVerBinPath=//Ip' | head -1)"
    # Guard each conversion: only prepend a REAL existing directory. An empty/failed cygpath would
    # otherwise inject a bogus relative entry that corrupts the Windows-format PATH the cargo build
    # subprocess inherits, which silently breaks other tools' resolution (e.g. cmake "program not found").
    if [ -n "$_vct" ]; then _clb="$(cygpath -u "${_vct}bin\\Hostx64\\x64" 2>/dev/null)"; [ -d "$_clb" ] && PATH="$_clb:$PATH"; fi
    if [ -n "$_sdkbin" ]; then _sdb="$(cygpath -u "${_sdkbin}x64" 2>/dev/null)"; [ -d "$_sdb" ] && PATH="$_sdb:$PATH"; fi
    export PATH
    # Pin cmake explicitly: the cmake crate (core/llama build.rs) resolves cmake via the CMAKE env
    # var or PATH, but a manifest-provisioned cmake lives at ~/.continuum/tools/cmake/bin and is not
    # guaranteed on the build shell's PATH (measured: absent in a clean subshell -> "cmake not
    # found"). Point CMAKE at the known install (same resolution as install-llama-server.sh) and put
    # its dir on PATH for cmake's own sub-tools.
    _ccmk="$(command -v cmake 2>/dev/null || echo "${CONTINUUM_HOME:-$HOME/.continuum}/tools/cmake/bin/cmake.exe")"
    if [ -x "$_ccmk" ]; then
      export CMAKE="$(cygpath -w "$_ccmk" 2>/dev/null || echo "$_ccmk")"
      PATH="$(dirname "$_ccmk"):$PATH"; export PATH
    fi
    # Force a generator cmake actually knows. The cmake crate auto-picks the newest installed VS; on a
    # VS18-2026 box that is "Visual Studio 18 2026" - a generator cmake 3.30.x does NOT define ("Could
    # not create named generator"). Ninja is version-agnostic, uses the MSVC env imported above, and
    # matches the llama-server build. [[windows-build-env-drift]]
    _cninja="$(command -v ninja 2>/dev/null || echo "${CONTINUUM_HOME:-$HOME/.continuum}/tools/ninja/ninja.exe")"
    if [ -x "$_cninja" ]; then
      export CMAKE_GENERATOR="Ninja"
      # ninja must be ON PATH: the cmake crate ignores CMAKE_MAKE_PROGRAM env, so with -G Ninja it
      # searches PATH ("unable to find Ninja / CMAKE_MAKE_PROGRAM is not set" otherwise).
      PATH="$(dirname "$_cninja"):$PATH"; export PATH
    fi
    # CUDA_PATH must be set: cudarc/candle/pocket-tts read it to emit their link-search; without it the
    # link has NO CUDA search path (measured: LNK1181 cuda.lib). Point it at a cuda-* whose import-lib
    # dir actually has the libs (a provisioning split can leave the crate-detected dir EMPTY - cuda-env
    # /Library/lib/x64=0 vs cuda-13.2=12; the #6 provisioning fix unifies them, and this node's cuda-env
    # was completed by copying the sibling's libs in). candle finds nvcc via PATH independently, so a
    # libs-only CUDA_PATH is fine (build proven). Real fix: provision ONE complete toolkit (#6).
    if [ -z "$CUDA_PATH" ]; then
      for _cand in "${CONTINUUM_HOME:-$HOME/.continuum}"/cuda-env "${CONTINUUM_HOME:-$HOME/.continuum}"/cuda-*; do
        for _sub in Library/lib/x64 lib/x64; do
          if [ -f "$_cand/$_sub/curand.lib" ] && [ -f "$_cand/$_sub/cuda.lib" ]; then
            export CUDA_PATH="$(cygpath -w "$_cand" 2>/dev/null || echo "$_cand")"
            _culibw="$(cygpath -w "$_cand/$_sub" 2>/dev/null || echo "$_cand/$_sub")"
            # pocket-tts links cuda.lib but (unlike cudarc) emits no rustc-link-search, relying on the
            # linker's own search. rustc links with the newest VS's linker + ITS OWN LIB (not ours), so
            # the dir must reach link.exe via RUSTFLAGS -L. RUSTFLAGS env REPLACES (not merges) the
            # .cargo/config target rustflags, so re-include +crt-static (the task-#4 /MT fix) or the whole
            # GPU stack mislinks LNK2038 MT-vs-MD.
            case " $RUSTFLAGS " in *"-L native=${_culibw} "*) : ;; *) export RUSTFLAGS="-C target-feature=+crt-static -L native=${_culibw} ${RUSTFLAGS}" ;; esac
            echo "▶ CUDA_PATH=$CUDA_PATH + RUSTFLAGS -L $_culibw (import libs in $_sub)"
            break 2
          fi
        done
      done
      [ -z "$CUDA_PATH" ] && echo "⚠ no complete CUDA import-lib dir found (cuda-*/**/{cuda,curand}.lib) - core link WILL fail. Provisioning gap (#6)." >&2
    fi
    if command -v cl.exe >/dev/null 2>&1; then
      echo "▶ MSVC toolchain imported for the CUDA cargo build (cl.exe on PATH for nvcc/candle)"
    else
      echo "⚠ MSVC import ran but cl.exe still unresolved — the CUDA cargo build will fail" >&2
    fi
  else
    echo "✗ Windows+CUDA needs VS2022 (14.4x) C++ x64 toolset for nvcc's host compiler; vswhere/vcvars not found — the core build will fail. Install via the 'msvc' module." >&2
  fi
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
if [ -x "$CONTINUUM_CLI_BIN" ]; then
  CONTINUUM_LINK_DIR="$HOME/.local/bin"
  mkdir -p "$CONTINUUM_LINK_DIR"
  # A stale symlink from an earlier install would otherwise make `cp` follow it back into
  # the target dir — remove any existing entry first, then copy the real bytes.
  rm -f "$CONTINUUM_LINK_DIR/continuum"
  if cp "$CONTINUUM_CLI_BIN" "$CONTINUUM_LINK_DIR/continuum.tmp.$$" \
     && mv -f "$CONTINUUM_LINK_DIR/continuum.tmp.$$" "$CONTINUUM_LINK_DIR/continuum"; then
    :
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
  if [ ! -x "$BRIDGE_BIN" ]; then
    echo "▶ building livekit-bridge sidecar (release — links webrtc-sys, first build is slow)…"
    cargo build --manifest-path "$REPO_ROOT/core/livekit-bridge/Cargo.toml" --bin livekit-bridge --release \
      || { echo "⚠ livekit-bridge build failed — live avatar/voice unavailable"; return 0; }
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
echo ""

# Run the EXACT binary the freshness guard (#194) just verified — NOT `cargo run`,
# which re-runs cargo's build logic at launch and could second-guess (or re-stale)
# what we already verified. We built it, we checked it reflects source, we run it.
# Unambiguous: the process image is the verified $CORE_BIN. [[verify-the-build-actually-deployed]]
exec "$CORE_BIN" "$CONTINUUM_SOCKET"
